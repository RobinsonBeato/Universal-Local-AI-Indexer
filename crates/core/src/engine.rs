use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result};
use rayon::prelude::*;
use regex::Regex;
use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, FieldType, Schema, Value, FAST, STORED, STRING, TEXT};
use tantivy::{doc, Index, ReloadPolicy, TantivyDocument, Term};
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use crate::config::LupaConfig;
use crate::extractors::{extract_docx_text, extract_pdf_text};
use crate::metadata::{FileRecord, MetadataStore};

#[derive(Debug, Clone, Serialize)]
pub struct IndexStats {
    pub scanned: usize,
    pub indexed_new: usize,
    pub indexed_updated: usize,
    pub skipped_unchanged: usize,
    pub removed: usize,
    pub errors: usize,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub query: String,
    pub total_hits: usize,
    pub took_ms: u128,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: usize,
    pub path_prefix: Option<String>,
    pub regex: Option<String>,
    pub highlight: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 20,
            path_prefix: None,
            regex: None,
            highlight: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub project_root: String,
    pub data_dir: String,
    pub index_dir: String,
    pub db_path: String,
    pub threads: usize,
    pub checks: Vec<String>,
}

#[derive(Clone)]
struct Fields {
    path: Field,
    name: Field,
    content: Field,
    mtime: Field,
}

struct QuerySignals {
    query_lower: String,
    terms: Vec<String>,
}

pub struct LupaEngine {
    project_root: PathBuf,
    data_dir: PathBuf,
    index_dir: PathBuf,
    db_path: PathBuf,
    config: LupaConfig,
}

struct FileSnapshot {
    path: PathBuf,
    path_str: String,
    mtime: i64,
    size: u64,
    prev: Option<FileRecord>,
}

struct PreparedDoc {
    record: FileRecord,
    name: String,
    content: String,
    is_new: bool,
}

impl LupaEngine {
    pub fn new(project_root: PathBuf, config: LupaConfig) -> Result<Self> {
        let data_dir = LupaConfig::data_dir(&project_root);
        let index_dir = data_dir.join("index");
        let db_path = data_dir.join("metadata.db");

        std::fs::create_dir_all(&index_dir)?;

        Ok(Self {
            project_root,
            data_dir,
            index_dir,
            db_path,
            config,
        })
    }

    pub fn build_incremental(&self) -> Result<IndexStats> {
        let start = Instant::now();
        let (index, fields) = self.ensure_index()?;
        let mut writer = self.acquire_writer_with_retry(&index)?;

        let mut store = MetadataStore::open(&self.db_path)?;
        let existing_records = store
            .all_records()?
            .into_iter()
            .map(|r| (r.path.clone(), r))
            .collect::<HashMap<_, _>>();

        let snapshots = self.collect_snapshots(&existing_records);
        let scanned = snapshots.len();
        let skipped_unchanged = snapshots
            .iter()
            .filter(|s| {
                s.prev.is_some()
                    && s.prev
                        .as_ref()
                        .is_some_and(|p| p.mtime == s.mtime && p.size == s.size)
            })
            .count();
        let scanned_set = snapshots
            .iter()
            .map(|s| s.path_str.clone())
            .collect::<HashSet<_>>();

        let candidates = snapshots
            .into_iter()
            .filter(|s| {
                if let Some(prev) = &s.prev {
                    !(prev.mtime == s.mtime && prev.size == s.size)
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();

        let prepared_results = candidates
            .par_iter()
            .map(|snapshot| self.prepare_doc(snapshot))
            .collect::<Vec<_>>();

        let mut errors = 0usize;
        let mut prepared = Vec::new();
        for result in prepared_results {
            match result {
                Ok(Some(doc)) => prepared.push(doc),
                Ok(None) => {}
                Err(_) => errors += 1,
            }
        }

        let mut indexed_new = 0usize;
        let mut indexed_updated = 0usize;
        let mut upserts = Vec::new();
        for prepared_doc in prepared {
            writer.delete_term(Term::from_field_text(
                fields.path,
                &prepared_doc.record.path,
            ));
            let doc = doc!(
                fields.path => prepared_doc.record.path.clone(),
                fields.name => prepared_doc.name,
                fields.content => prepared_doc.content,
                fields.mtime => prepared_doc.record.mtime
            );
            writer.add_document(doc)?;

            if prepared_doc.is_new {
                indexed_new += 1;
            } else {
                indexed_updated += 1;
            }
            upserts.push(prepared_doc.record);
        }

        let removed_paths = existing_records
            .keys()
            .filter(|p| !scanned_set.contains(*p))
            .cloned()
            .collect::<Vec<_>>();
        for p in &removed_paths {
            writer.delete_term(Term::from_field_text(fields.path, p));
        }

        writer.commit()?;

        if !upserts.is_empty() {
            store.upsert_many(&upserts)?;
        }
        if !removed_paths.is_empty() {
            store.remove_many(&removed_paths)?;
        }

        Ok(IndexStats {
            scanned,
            indexed_new,
            indexed_updated,
            skipped_unchanged,
            removed: removed_paths.len(),
            errors,
            duration_ms: start.elapsed().as_millis(),
        })
    }

    pub fn apply_dirty_paths(&self, dirty_paths: &[PathBuf]) -> Result<IndexStats> {
        let start = Instant::now();
        let (index, fields) = self.ensure_index()?;
        let mut writer = self.acquire_writer_with_retry(&index)?;
        let mut store = MetadataStore::open(&self.db_path)?;

        let paths = self.collect_files_from_dirty_input(dirty_paths);
        let mut scanned = 0usize;
        let mut indexed_new = 0usize;
        let mut indexed_updated = 0usize;
        let mut skipped_unchanged = 0usize;
        let mut removed = 0usize;
        let mut errors = 0usize;

        let mut upserts = Vec::new();
        let mut removals = Vec::new();

        for path in paths {
            let path_str = normalize_path(&path);
            scanned += 1;

            if !path.exists() {
                writer.delete_term(Term::from_field_text(fields.path, &path_str));
                removals.push(path_str);
                removed += 1;
                continue;
            }

            if self.config.should_exclude(&path) || !path.is_file() {
                continue;
            }

            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => {
                    errors += 1;
                    continue;
                }
            };
            let mtime = match meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            {
                Some(d) => d.as_secs() as i64,
                None => {
                    errors += 1;
                    continue;
                }
            };
            let size = meta.len();

            let prev = store.get_record(&path_str)?;
            if let Some(prev) = &prev {
                if prev.mtime == mtime && prev.size == size {
                    skipped_unchanged += 1;
                    continue;
                }
            }

            let snapshot = FileSnapshot {
                path: path.clone(),
                path_str: path_str.clone(),
                mtime,
                size,
                prev,
            };

            match self.prepare_doc(&snapshot) {
                Ok(Some(prepared_doc)) => {
                    writer.delete_term(Term::from_field_text(
                        fields.path,
                        &prepared_doc.record.path,
                    ));
                    let doc = doc!(
                        fields.path => prepared_doc.record.path.clone(),
                        fields.name => prepared_doc.name,
                        fields.content => prepared_doc.content,
                        fields.mtime => prepared_doc.record.mtime
                    );
                    writer.add_document(doc)?;
                    if prepared_doc.is_new {
                        indexed_new += 1;
                    } else {
                        indexed_updated += 1;
                    }
                    upserts.push(prepared_doc.record);
                }
                Ok(None) => {
                    skipped_unchanged += 1;
                }
                Err(_) => {
                    errors += 1;
                }
            }
        }

        writer.commit()?;
        if !upserts.is_empty() {
            store.upsert_many(&upserts)?;
        }
        if !removals.is_empty() {
            store.remove_many(&removals)?;
        }

        Ok(IndexStats {
            scanned,
            indexed_new,
            indexed_updated,
            skipped_unchanged,
            removed,
            errors,
            duration_ms: start.elapsed().as_millis(),
        })
    }

    pub fn search(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult> {
        let start = Instant::now();
        let (index, fields) = self.ensure_index()?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let searcher = reader.searcher();
        let parser = QueryParser::for_index(&index, vec![fields.name, fields.path, fields.content]);
        let q = parser
            .parse_query(query)
            .with_context(|| format!("query inválida: {query}"))?;

        let oversample = opts.limit.saturating_mul(4).max(opts.limit);
        let top_docs = searcher.search(&q, &TopDocs::with_limit(oversample))?;

        let regex = match &opts.regex {
            Some(pattern) => {
                Some(Regex::new(pattern).with_context(|| format!("regex inválida: {pattern}"))?)
            }
            None => None,
        };

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_secs() as i64;
        let signals = build_query_signals(query);

        let mut hits = Vec::new();
        for (score, addr) in top_docs {
            let retrieved: TantivyDocument = searcher.doc(addr)?;
            let path = retrieved
                .get_first(fields.path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let name = retrieved
                .get_first(fields.name)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let mtime = retrieved
                .get_first(fields.mtime)
                .and_then(|v| v.as_i64())
                .unwrap_or_default();

            if let Some(prefix) = &opts.path_prefix {
                if !path.starts_with(prefix) {
                    continue;
                }
            }

            if let Some(re) = &regex {
                let content = self
                    .load_query_content(Path::new(&path))
                    .unwrap_or_default();
                let hay = format!("{}\n{}", path, content);
                if !re.is_match(&hay) {
                    continue;
                }
            }

            let final_score = rerank_score(score, &name, &path, mtime, &signals, now_unix);

            hits.push(SearchHit {
                path,
                score: final_score,
                snippet: None,
            });
        }

        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(opts.limit);

        if opts.highlight {
            for hit in &mut hits {
                let content = self
                    .load_query_content(Path::new(&hit.path))
                    .unwrap_or_default();
                hit.snippet = Some(highlight_snippet(&content, query));
            }
        }

        Ok(SearchResult {
            query: query.to_string(),
            total_hits: hits.len(),
            took_ms: start.elapsed().as_millis(),
            hits,
        })
    }

    pub fn doctor(&self) -> Result<DoctorReport> {
        std::fs::create_dir_all(&self.data_dir)?;
        let mut checks = Vec::new();

        if self.project_root.exists() {
            checks.push("project_root_exists:ok".to_string());
        } else {
            checks.push("project_root_exists:fail".to_string());
        }

        let probe = self.data_dir.join(".write_probe");
        match std::fs::write(&probe, b"ok") {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                checks.push("data_dir_writable:ok".to_string());
            }
            Err(_) => checks.push("data_dir_writable:fail".to_string()),
        }

        match MetadataStore::open(&self.db_path) {
            Ok(_) => checks.push("sqlite_open:ok".to_string()),
            Err(_) => checks.push("sqlite_open:fail".to_string()),
        }

        match self.ensure_index() {
            Ok(_) => checks.push("tantivy_open:ok".to_string()),
            Err(_) => checks.push("tantivy_open:fail".to_string()),
        }

        Ok(DoctorReport {
            project_root: self.project_root.display().to_string(),
            data_dir: self.data_dir.display().to_string(),
            index_dir: self.index_dir.display().to_string(),
            db_path: self.db_path.display().to_string(),
            threads: self.config.effective_threads(),
            checks,
        })
    }

    fn ensure_index(&self) -> Result<(Index, Fields)> {
        if self.index_dir.join("meta.json").exists() {
            let index = Index::open_in_dir(&self.index_dir)?;
            let schema = index.schema();
            if let Ok(fields) = resolve_fields(&schema) {
                if is_schema_compatible(&schema, &fields) {
                    return Ok((index, fields));
                }
            }

            // Schema viejo/incompatible: recrear índice local y resetear metadata incremental.
            std::fs::remove_dir_all(&self.index_dir)?;
            std::fs::create_dir_all(&self.index_dir)?;
            if self.db_path.exists() {
                let _ = std::fs::remove_file(&self.db_path);
            }
        }

        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("path", STRING | STORED);
        schema_builder.add_text_field("name", TEXT | STORED);
        schema_builder.add_text_field("content", TEXT);
        schema_builder.add_i64_field("mtime", FAST | STORED);
        let schema = schema_builder.build();
        let index = Index::create_in_dir(&self.index_dir, schema.clone())?;
        Ok((index, resolve_fields(&schema)?))
    }

    fn collect_snapshots(
        &self,
        existing_records: &HashMap<String, FileRecord>,
    ) -> Vec<FileSnapshot> {
        self.walk_files()
            .into_iter()
            .filter_map(|path| {
                let meta = std::fs::metadata(&path).ok()?;
                let mtime = meta
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_secs() as i64;
                let size = meta.len();
                let path_str = normalize_path(&path);
                let prev = existing_records.get(&path_str).cloned();
                Some(FileSnapshot {
                    path,
                    path_str,
                    mtime,
                    size,
                    prev,
                })
            })
            .collect()
    }

    fn walk_files(&self) -> Vec<PathBuf> {
        WalkDir::new(&self.project_root)
            .into_iter()
            .filter_entry(|entry| !self.config.should_exclude(entry.path()))
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>()
    }

    fn collect_files_from_dirty_input(&self, dirty_paths: &[PathBuf]) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        for input in dirty_paths {
            let p = if input.is_absolute() {
                input.clone()
            } else {
                self.project_root.join(input)
            };

            if self.config.should_exclude(&p) {
                continue;
            }

            if p.is_dir() {
                for entry in WalkDir::new(&p)
                    .into_iter()
                    .filter_entry(|e| !self.config.should_exclude(e.path()))
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                {
                    let file_path = entry.path().to_path_buf();
                    let normalized = normalize_path(&file_path);
                    if seen.insert(normalized) {
                        out.push(file_path);
                    }
                }
            } else {
                let normalized = normalize_path(&p);
                if seen.insert(normalized) {
                    out.push(p);
                }
            }
        }

        out
    }

    fn prepare_doc(&self, snapshot: &FileSnapshot) -> Result<Option<PreparedDoc>> {
        let name = snapshot
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| snapshot.path_str.clone());

        let mut hash = None;
        let mut content = String::new();
        let mut preloaded_bytes = None;

        if snapshot.size <= self.config.hash_small_file_threshold {
            let bytes = std::fs::read(&snapshot.path)
                .with_context(|| format!("no se pudo leer {}", snapshot.path.display()))?;
            hash = Some(format!("{:x}", xxh3_64(&bytes)));
            preloaded_bytes = Some(bytes);
        }

        if let Some(prev) = &snapshot.prev {
            if hash.is_some() && prev.hash == hash {
                return Ok(None);
            }
        }

        if self
            .config
            .allows_content_extract(&snapshot.path, snapshot.size)
        {
            content = self.extract_indexable_content(&snapshot.path, preloaded_bytes.as_deref())?;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_secs() as i64;
        let record = FileRecord {
            path: snapshot.path_str.clone(),
            mtime: snapshot.mtime,
            size: snapshot.size,
            hash,
            indexed_at: now,
        };

        Ok(Some(PreparedDoc {
            record,
            name,
            content,
            is_new: snapshot.prev.is_none(),
        }))
    }

    fn extract_indexable_content(
        &self,
        path: &Path,
        preloaded_bytes: Option<&[u8]>,
    ) -> Result<String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if self.config.is_text_extension(path) {
            let bytes = match preloaded_bytes {
                Some(bytes) => bytes.to_vec(),
                None => std::fs::read(path)
                    .with_context(|| format!("no se pudo leer {}", path.display()))?,
            };

            if bytes.contains(&0) {
                return Ok(String::new());
            }
            return Ok(String::from_utf8_lossy(&bytes).to_string());
        }

        if ext == "docx" {
            return Ok(extract_docx_text(path).unwrap_or_default());
        }

        if ext == "pdf" {
            return Ok(extract_pdf_text(path).unwrap_or_default());
        }

        Ok(String::new())
    }

    fn load_query_content(&self, path: &Path) -> Result<String> {
        let meta = std::fs::metadata(path)?;
        if !self.config.allows_content_extract(path, meta.len()) {
            return Ok(String::new());
        }
        self.extract_indexable_content(path, None)
    }

    fn acquire_writer_with_retry(&self, index: &Index) -> Result<tantivy::IndexWriter> {
        let mut last_err = None;
        for attempt in 0..10 {
            match index.writer(50_000_000) {
                Ok(writer) => return Ok(writer),
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("LockBusy") || msg.contains("Failed to acquire index lock") {
                        last_err = Some(err);
                        let backoff_ms = 100 + (attempt * 100) as u64;
                        thread::sleep(Duration::from_millis(backoff_ms));
                        continue;
                    }
                    return Err(err.into());
                }
            }
        }

        Err(anyhow::anyhow!(
            "No se pudo adquirir lock de índice tras reintentos: {}",
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "desconocido".to_string())
        ))
    }
}

fn resolve_fields(schema: &Schema) -> Result<Fields> {
    let path = schema.get_field("path")?;
    let name = schema.get_field("name")?;
    let content = schema.get_field("content")?;
    let mtime = schema.get_field("mtime")?;
    Ok(Fields {
        path,
        name,
        content,
        mtime,
    })
}

fn is_schema_compatible(schema: &Schema, fields: &Fields) -> bool {
    let content_is_not_stored = match schema.get_field_entry(fields.content).field_type() {
        FieldType::Str(opts) => !opts.is_stored(),
        _ => true,
    };
    content_is_not_stored
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn highlight_snippet(content: &str, query: &str) -> String {
    let q = query.to_lowercase();
    let lower = content.to_lowercase();
    if let Some(idx) = lower.find(&q) {
        let start = idx.saturating_sub(40);
        let end = (idx + q.len() + 80).min(content.len());
        return content[start..end].replace('\n', " ");
    }

    content.chars().take(120).collect()
}

fn build_query_signals(query: &str) -> QuerySignals {
    let query_lower = query.trim().to_lowercase();
    let terms = query_lower
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    QuerySignals { query_lower, terms }
}

fn rerank_score(
    base_score: f32,
    name: &str,
    path: &str,
    mtime: i64,
    q: &QuerySignals,
    now_unix: i64,
) -> f32 {
    if q.query_lower.is_empty() {
        return base_score;
    }

    let name_l = name.to_lowercase();
    let path_l = path.to_lowercase();

    let term_count = q.terms.len().max(1) as f32;
    let name_hits = q
        .terms
        .iter()
        .filter(|t| name_l.contains(t.as_str()))
        .count() as f32;
    let path_hits = q
        .terms
        .iter()
        .filter(|t| path_l.contains(t.as_str()))
        .count() as f32;

    let name_ratio = name_hits / term_count;
    let path_ratio = path_hits / term_count;

    let exact_name_bonus = if name_l.contains(&q.query_lower) {
        1.8
    } else {
        0.0
    };
    let exact_path_bonus = if path_l.contains(&q.query_lower) {
        0.9
    } else {
        0.0
    };

    let age_secs = (now_unix - mtime).max(0);
    let age_days = age_secs / 86_400;
    let recency_bonus = if age_days <= 1 {
        0.9
    } else if age_days <= 7 {
        0.6
    } else if age_days <= 30 {
        0.3
    } else if age_days <= 180 {
        0.1
    } else {
        0.0
    };

    base_score
        + (name_ratio * 2.2)
        + (path_ratio * 1.0)
        + exact_name_bonus
        + exact_path_bonus
        + recency_bonus
}

#[cfg(test)]
mod tests {
    use super::{LupaEngine, SearchOptions};
    use crate::config::LupaConfig;

    #[test]
    fn end_to_end_index_and_search() {
        let root = std::env::temp_dir().join(format!(
            "lupa_engine_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch should be available")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("should create temporary test root");
        std::fs::write(root.join("hello.txt"), "hello from lupa index")
            .expect("should write fixture file for indexing");

        let cfg = LupaConfig {
            excludes: vec![".lupa".to_string()],
            ..LupaConfig::default()
        };
        let engine = LupaEngine::new(root.clone(), cfg).expect("should create engine");
        let stats = engine
            .build_incremental()
            .expect("should build index incrementally");
        assert!(stats.scanned >= 1);

        let result = engine
            .search(
                "hello",
                &SearchOptions {
                    highlight: true,
                    ..Default::default()
                },
            )
            .expect("should execute search");
        assert!(result.total_hits >= 1);

        let _ = std::fs::remove_dir_all(root);
    }
}
