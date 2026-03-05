use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::time::SystemTime;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::config::{LupaConfig, QaMode};
use crate::extractors::{extract_docx_text, extract_pdf_text};

#[derive(Debug, Clone)]
pub struct QaRequest {
    pub document_path: String,
    pub question: String,
}

#[derive(Debug, Clone)]
pub struct QaCitation {
    pub path: String,
    pub excerpt: String,
}

#[derive(Debug, Clone)]
pub struct QaAnswer {
    pub answer: String,
    pub citations: Vec<QaCitation>,
}

pub trait QaProvider: Send + Sync {
    fn mode(&self) -> QaMode;
    fn answer(&self, request: &QaRequest) -> Result<QaAnswer>;
}

pub fn provider_from_config(project_root: PathBuf, config: LupaConfig) -> Box<dyn QaProvider> {
    match config.qa.mode {
        QaMode::Extractive => Box::new(ExtractiveProvider::new(project_root, config)),
        QaMode::LocalModel => Box::new(LocalModelProvider::new(project_root, config)),
    }
}

pub struct ExtractiveProvider {
    project_root: PathBuf,
    config: LupaConfig,
}

impl ExtractiveProvider {
    pub fn new(project_root: PathBuf, config: LupaConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }
}

impl QaProvider for ExtractiveProvider {
    fn mode(&self) -> QaMode {
        QaMode::Extractive
    }

    fn answer(&self, request: &QaRequest) -> Result<QaAnswer> {
        let path = resolve_doc_path(&self.project_root, &request.document_path);
        if !path.exists() {
            return Err(anyhow!("document not found: {}", path.display()));
        }

        let meta = std::fs::metadata(&path)?;
        let size = meta.len();
        if !self.config.allows_content_extract(&path, size) {
            return Ok(QaAnswer {
                answer: "Document is too large for extractive mode limits.".to_string(),
                citations: vec![],
            });
        }

        let ext = extension_of(&path);
        let question_l = request.question.to_ascii_lowercase();
        if question_l.contains("created")
            || question_l.contains("creado")
            || question_l.contains("creation date")
        {
            let created = meta
                .created()
                .ok()
                .map(format_system_time)
                .unwrap_or_else(|| "-".to_string());
            return Ok(QaAnswer {
                answer: format!("Created: {created}"),
                citations: vec![QaCitation {
                    path: path.display().to_string(),
                    excerpt: "metadata.created".to_string(),
                }],
            });
        }
        if question_l.contains("modified")
            || question_l.contains("modificado")
            || question_l.contains("last modified")
        {
            let modified = meta
                .modified()
                .ok()
                .map(format_system_time)
                .unwrap_or_else(|| "-".to_string());
            return Ok(QaAnswer {
                answer: format!("Modified: {modified}"),
                citations: vec![QaCitation {
                    path: path.display().to_string(),
                    excerpt: "metadata.modified".to_string(),
                }],
            });
        }
        if question_l.contains("size") || question_l.contains("tamano") || question_l.contains("peso")
        {
            return Ok(QaAnswer {
                answer: format!("Size: {}", human_size(size)),
                citations: vec![QaCitation {
                    path: path.display().to_string(),
                    excerpt: "metadata.size".to_string(),
                }],
            });
        }

        let content = if self.config.is_text_extension(&path) {
            read_text_limited(&path, self.config.max_file_size_bytes as usize)?
        } else if ext == "pdf" {
            extract_pdf_text(&path).unwrap_or_default()
        } else if ext == "docx" {
            extract_docx_text(&path).unwrap_or_default()
        } else {
            String::new()
        };

        if content.trim().is_empty() {
            return Ok(QaAnswer {
                answer: "No extractable text found for this document in extractive mode."
                    .to_string(),
                citations: vec![],
            });
        }

        let excerpt = pick_best_excerpt(&content, &request.question);
        Ok(QaAnswer {
            answer: excerpt.clone(),
            citations: vec![QaCitation {
                path: path.display().to_string(),
                excerpt,
            }],
        })
    }
}

pub struct LocalModelProvider {
    project_root: PathBuf,
    config: LupaConfig,
}

impl LocalModelProvider {
    pub fn new(project_root: PathBuf, config: LupaConfig) -> Self {
        Self {
            project_root,
            config,
        }
    }
}

impl QaProvider for LocalModelProvider {
    fn mode(&self) -> QaMode {
        QaMode::LocalModel
    }

    fn answer(&self, request: &QaRequest) -> Result<QaAnswer> {
        let doc_path = resolve_doc_path(&self.project_root, &request.document_path);
        let meta = std::fs::metadata(&doc_path)
            .map_err(|e| anyhow!("cannot read document metadata {}: {e}", doc_path.display()))?;
        let size = meta.len();
        if !self.config.allows_content_extract(&doc_path, size) {
            return Err(anyhow!(
                "document too large for local AI context limits: {}",
                doc_path.display()
            ));
        }

        // Deterministic fast-path for count-style questions.
        if let Some((needle, count)) =
            count_word_question(&self.config, &request.question, &doc_path, self.config.max_structured_file_size_bytes)?
        {
            let lang = detect_lang(&request.question);
            let answer = if lang == "es" {
                format!("La palabra \"{needle}\" aparece {count} veces.")
            } else {
                format!("The word \"{needle}\" appears {count} times.")
            };
            return Ok(QaAnswer {
                answer,
                citations: vec![QaCitation {
                    path: request.document_path.clone(),
                    excerpt: format!("count(\"{needle}\") = {count}"),
                }],
            });
        }

        let model_path = resolve_doc_path(&self.project_root, &expand_env_tokens(&self.config.qa.model_path));
        if model_path.as_os_str().is_empty() || self.config.qa.model_path.trim().is_empty() {
            return Err(anyhow!(
                "qa.mode=local_model but qa.model_path is empty. Configure it in config.toml."
            ));
        }
        if !model_path.exists() {
            return Err(anyhow!(
                "model file not found: {}",
                model_path.display()
            ));
        }

        let endpoint = self.config.qa.endpoint.trim();
        if endpoint.is_empty() {
            return Err(anyhow!("qa.endpoint is empty"));
        }

        if !server_alive(endpoint) && self.config.qa.auto_start_server {
            let server_path = resolve_doc_path(
                &self.project_root,
                &expand_env_tokens(&self.config.qa.llama_server_path),
            );
            start_server_once(
                &server_path,
                &model_path,
                endpoint,
                self.config.qa.timeout_ms,
            )?;
        }

        wait_for_server(endpoint, self.config.qa.timeout_ms)?;
        let content = load_document_text(&self.config, &doc_path)?;
        if content.trim().is_empty() {
            return Err(anyhow!("no extractable text for local AI in {}", doc_path.display()));
        }
        let prompt = build_doc_prompt(request, &content);
        let completion = request_completion(
            endpoint,
            &prompt,
            self.config.qa.max_tokens,
            self.config.qa.timeout_ms,
        )?;
        let completion = sanitize_answer(&completion);

        Ok(QaAnswer {
            answer: completion.clone(),
            citations: vec![QaCitation {
                path: request.document_path.clone(),
                excerpt: completion,
            }],
        })
    }
}

fn build_doc_prompt(request: &QaRequest, content: &str) -> String {
    let lang = detect_lang(&request.question);
    let context = build_context(content, &request.question, 5600);
    let lang_rule = if lang == "es" {
        "Respond only in Spanish."
    } else {
        "Respond only in English."
    };
    format!(
        "You are an offline document assistant.\nRules:\n- {lang_rule}\n- Answer in at most 3 short bullet points.\n- Do not repeat ideas.\n- Use ONLY the provided context.\n- If answer is not in context, reply exactly: Not found in document context.\nDocument path: {}\nContext:\n{}\nQuestion: {}\nAnswer:",
        request.document_path, context, request.question
    )
}

fn request_completion(endpoint: &str, prompt: &str, max_tokens: usize, timeout_ms: u64) -> Result<String> {
    let url = format!("{}/completion", endpoint.trim_end_matches('/'));
    let body = json!({
        "prompt": prompt,
        "n_predict": (max_tokens.min(120)) as i64,
        "temperature": 0.05,
        "top_p": 0.85,
        "top_k": 30,
        "repeat_penalty": 1.22,
        "presence_penalty": 0.2,
        "stop": ["\n\nUser:", "\n\nQuestion:", "\nContext:", "\nRules:", "Document path:"]
    });
    let value: Value = ureq::post(&url)
        .timeout(Duration::from_millis(timeout_ms))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| anyhow!("llama-server request failed: {e}"))?
        .into_json::<Value>()
        .map_err(|e| anyhow!("invalid response json: {e}"))?;

    let text = value
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("completion").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(anyhow!("empty response from local model"));
    }
    Ok(text)
}

fn sanitize_answer(raw: &str) -> String {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut out = Vec::new();
    for line in raw.lines() {
        let clean = line.trim();
        if clean.is_empty() {
            continue;
        }
        let key = clean.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(clean.to_string());
        }
        if out.len() >= 5 {
            break;
        }
    }
    if out.is_empty() {
        raw.trim().chars().take(300).collect()
    } else {
        out.join("\n")
    }
}

fn build_context(content: &str, question: &str, max_chars: usize) -> String {
    let normalized = content.replace('\r', " ").replace('\n', " ");
    let mut sentences = normalized
        .split(['.', '!', '?', ';'])
        .map(str::trim)
        .filter(|s| s.len() >= 25)
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if sentences.is_empty() {
        return normalized.chars().take(max_chars).collect();
    }

    let keywords = extract_keywords(question);
    if keywords.is_empty() {
        let mut out = String::new();
        for s in sentences.iter().take(18) {
            if out.len() + s.len() + 2 > max_chars {
                break;
            }
            if !out.is_empty() {
                out.push_str(". ");
            }
            out.push_str(s);
        }
        return out;
    }

    let mut scored = sentences
        .drain(..)
        .map(|s| {
            let lower = s.to_ascii_lowercase();
            let score = keywords.iter().filter(|k| lower.contains(*k)).count();
            (score, s)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let mut out = String::new();
    for (score, s) in scored.into_iter().take(28) {
        if score == 0 && out.len() > max_chars / 2 {
            break;
        }
        if out.len() + s.len() + 2 > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push_str(". ");
        }
        out.push_str(&s);
    }
    if out.is_empty() {
        normalized.chars().take(max_chars).collect()
    } else {
        out
    }
}

fn server_alive(endpoint: &str) -> bool {
    let health = format!("{}/health", endpoint.trim_end_matches('/'));
    ureq::get(&health)
        .timeout(Duration::from_millis(600))
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
}

fn wait_for_server(endpoint: &str, timeout_ms: u64) -> Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < timeout_ms as u128 {
        if server_alive(endpoint) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(anyhow!(
        "local model server did not become ready at {} within {}ms",
        endpoint,
        timeout_ms
    ))
}

fn start_server_once(server_path: &Path, model_path: &Path, endpoint: &str, timeout_ms: u64) -> Result<()> {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }

    if !server_path.exists() {
        STARTED.store(false, Ordering::SeqCst);
        return Err(anyhow!(
            "llama-server executable not found: {}",
            server_path.display()
        ));
    }

    let port = endpoint_port(endpoint).unwrap_or(8088).to_string();
    let host = "127.0.0.1";
    let mut cmd = Command::new(server_path);
    cmd.arg("-m")
        .arg(model_path)
        .arg("-c")
        .arg("2048")
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(&port);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn()
        .map_err(|e| anyhow!("failed to start llama-server: {e}"))?;

    wait_for_server(endpoint, timeout_ms)?;
    Ok(())
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    let e = endpoint.trim_end_matches('/');
    let pos = e.rfind(':')?;
    e[pos + 1..].parse::<u16>().ok()
}

fn expand_env_tokens(input: &str) -> String {
    let mut out = input.to_string();
    if out.contains("%LOCALAPPDATA%") {
        if let Ok(v) = std::env::var("LOCALAPPDATA") {
            out = out.replace("%LOCALAPPDATA%", &v);
        }
    }
    if out.starts_with("~/") {
        if let Ok(home) = std::env::var("USERPROFILE") {
            out = out.replacen("~", &home, 1);
        }
    }
    out
}

fn resolve_doc_path(project_root: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        project_root.join(p)
    }
}

fn load_document_text(config: &LupaConfig, path: &Path) -> Result<String> {
    let ext = extension_of(path);
    if config.is_text_extension(path) {
        return read_text_limited(path, config.max_file_size_bytes as usize);
    }
    if ext == "pdf" {
        return Ok(extract_pdf_text(path).unwrap_or_default());
    }
    if ext == "docx" {
        return Ok(extract_docx_text(path).unwrap_or_default());
    }
    Ok(String::new())
}

fn count_word_question(
    config: &LupaConfig,
    question: &str,
    path: &Path,
    max_size: u64,
) -> Result<Option<(String, usize)>> {
    let q = question.to_ascii_lowercase();
    let looks_like_count = q.contains("cuantas veces")
        || q.contains("cuántas veces")
        || q.contains("how many times")
        || q.contains("count");
    if !looks_like_count {
        return Ok(None);
    }
    let word = extract_quoted_word(question);
    let Some(needle) = word else {
        return Ok(None);
    };
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_size {
        return Ok(None);
    }
    let content = load_document_text(config, path)?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let count = count_occurrences(&content, &needle);
    Ok(Some((needle, count)))
}

fn extract_quoted_word(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    for quote in ['"', '\'', '“', '”'] {
        let mut start = None;
        for (i, c) in chars.iter().enumerate() {
            if *c == quote {
                if let Some(st) = start {
                    let val: String = chars[st + 1..i].iter().collect();
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                    start = None;
                } else {
                    start = Some(i);
                }
            }
        }
    }
    None
}

fn count_occurrences(content: &str, needle: &str) -> usize {
    let hay = content.to_ascii_lowercase();
    let nd = needle.to_ascii_lowercase();
    if nd.is_empty() {
        return 0;
    }
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(idx) = hay[from..].find(&nd) {
        n += 1;
        from += idx + nd.len();
        if from >= hay.len() {
            break;
        }
    }
    n
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

fn read_text_limited(path: &Path, max_bytes: usize) -> Result<String> {
    use std::io::Read;
    let f = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    let mut limited = f.take(max_bytes as u64);
    limited.read_to_end(&mut buf)?;
    if buf.contains(&0) {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn pick_best_excerpt(content: &str, question: &str) -> String {
    let keywords = extract_keywords(question);
    let normalized = content.replace('\n', " ");
    let sentences = normalized
        .split(['.', '!', '?', ';'])
        .map(str::trim)
        .filter(|s| s.len() >= 20)
        .take(20)
        .collect::<Vec<_>>();

    if sentences.is_empty() {
        return normalized.chars().take(220).collect();
    }
    if keywords.is_empty() {
        return sentences[0].to_string();
    }

    let mut best = sentences[0];
    let mut best_score = 0usize;
    for s in &sentences {
        let lower = s.to_ascii_lowercase();
        let score = keywords.iter().filter(|k| lower.contains(*k)).count();
        if score > best_score {
            best_score = score;
            best = s;
        }
    }
    best.to_string()
}

fn extract_keywords(question: &str) -> Vec<String> {
    let stop = [
        "the", "and", "for", "with", "from", "that", "this", "what", "where", "when", "como",
        "para", "con", "del", "las", "los", "que", "una", "uno", "sobre", "donde", "cual",
    ];
    question
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| w.trim().to_ascii_lowercase())
        .filter(|w| w.len() >= 3 && !stop.contains(&w.as_str()))
        .take(10)
        .collect()
}

fn detect_lang(question: &str) -> &'static str {
    let q = question.to_ascii_lowercase();
    let es_hits = [
        " que ", " como ", " cuando ", " dónde", " donde ", " resumen ", "explica", "cuantas",
        "cuántas", "palabra", "archivo", "documento", "por que", "porque",
    ]
    .iter()
    .filter(|k| q.contains(**k))
    .count();
    let en_hits = [
        " what ", " how ", " when ", " where ", "summary", "explain", "word", "document", "file",
        "why ", "count",
    ]
    .iter()
    .filter(|k| q.contains(**k))
    .count();
    if es_hits >= en_hits { "es" } else { "en" }
}

fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn format_system_time(t: SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sec_day = secs % 86_400;
    let h = sec_day / 3_600;
    let m = (sec_day % 3_600) / 60;
    let s = sec_day % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

#[cfg(test)]
mod tests {
    use super::{ExtractiveProvider, QaProvider, QaRequest};
    use crate::config::LupaConfig;

    #[test]
    fn extractive_provider_answers_from_text_file() {
        let root = std::env::temp_dir().join(format!(
            "lupa_qa_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch should be available")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("should create temp root");
        let doc = root.join("note.txt");
        std::fs::write(
            &doc,
            "Lupa is a local-first search tool. It indexes files very fast on Windows.",
        )
        .expect("should write fixture document");

        let provider = ExtractiveProvider::new(root.clone(), LupaConfig::default());
        let request = QaRequest {
            document_path: doc.display().to_string(),
            question: "What does this tool do?".to_string(),
        };
        let ans = provider.answer(&request).expect("should answer");
        assert!(!ans.answer.trim().is_empty());
        assert_eq!(ans.citations.len(), 1);

        let _ = std::fs::remove_dir_all(root);
    }
}
