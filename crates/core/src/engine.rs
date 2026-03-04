use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use rayon::prelude::*;
use regex::Regex;
use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, ReloadPolicy, TantivyDocument, Term};
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use crate::config::LupaConfig;
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
    content: Field,
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
        let mut writer = index.writer(50_000_000)?;

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

        let prepared = candidates
            .par_iter()
            .filter_map(|snapshot| self.prepare_doc(snapshot).transpose())
            .collect::<Result<Vec<_>>>()?;

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
                fields.content => prepared_doc.content
            );
            writer.add_document(doc)?;

            if prepared_doc.is_new {
                indexed_new += 1;
            } else {
                indexed_updated += 1;
            }
            upserts.push(prepared_doc.record);
        }

        let scanned_set = self.collect_scanned_paths_set();
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
            errors: 0,
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
        let parser = QueryParser::for_index(&index, vec![fields.content]);
        let q = parser
            .parse_query(query)
            .with_context(|| format!("query inválida: {query}"))?;

        let oversample = opts.limit.saturating_mul(5).max(opts.limit);
        let top_docs = searcher.search(&q, &TopDocs::with_limit(oversample))?;

        let regex = match &opts.regex {
            Some(pattern) => {
                Some(Regex::new(pattern).with_context(|| format!("regex inválida: {pattern}"))?)
            }
            None => None,
        };

        let mut hits = Vec::new();
        for (score, addr) in top_docs {
            let retrieved: TantivyDocument = searcher.doc(addr)?;
            let path = retrieved
                .get_first(fields.path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let content = retrieved
                .get_first(fields.content)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if let Some(prefix) = &opts.path_prefix {
                if !path.starts_with(prefix) {
                    continue;
                }
            }

            if let Some(re) = &regex {
                let hay = format!("{}\n{}", path, content);
                if !re.is_match(&hay) {
                    continue;
                }
            }

            let snippet = if opts.highlight {
                Some(highlight_snippet(&content, query))
            } else {
                None
            };

            hits.push(SearchHit {
                path,
                score,
                snippet,
            });
            if hits.len() >= opts.limit {
                break;
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
            return Ok((index, resolve_fields(&schema)?));
        }

        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("path", STRING | STORED);
        schema_builder.add_text_field("content", TEXT | STORED);
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

    fn collect_scanned_paths_set(&self) -> HashSet<String> {
        self.walk_files()
            .into_iter()
            .map(|p| normalize_path(&p))
            .collect::<HashSet<_>>()
    }

    fn walk_files(&self) -> Vec<PathBuf> {
        WalkDir::new(&self.project_root)
            .into_iter()
            .filter_entry(|entry| !self.config.should_exclude(entry.path()))
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.path().to_path_buf())
            .filter(|path| self.config.is_allowed_extension(path))
            .collect::<Vec<_>>()
    }

    fn prepare_doc(&self, snapshot: &FileSnapshot) -> Result<Option<PreparedDoc>> {
        if snapshot.size > self.config.max_file_size_bytes {
            return Ok(None);
        }

        let bytes = std::fs::read(&snapshot.path)
            .with_context(|| format!("no se pudo leer {}", snapshot.path.display()))?;
        if bytes.contains(&0) {
            return Ok(None);
        }

        let hash = if snapshot.size <= self.config.hash_small_file_threshold {
            Some(format!("{:x}", xxh3_64(&bytes)))
        } else {
            None
        };

        if let Some(prev) = &snapshot.prev {
            if hash.is_some() && prev.hash == hash {
                return Ok(None);
            }
        }

        let content = String::from_utf8_lossy(&bytes).to_string();
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
            content,
            is_new: snapshot.prev.is_none(),
        }))
    }
}

fn resolve_fields(schema: &Schema) -> Result<Fields> {
    let path = schema.get_field("path")?;
    let content = schema.get_field("content")?;
    Ok(Fields { path, content })
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

        let mut cfg = LupaConfig::default();
        cfg.excludes.clear();
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
