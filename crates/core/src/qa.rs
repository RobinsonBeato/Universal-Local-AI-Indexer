use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, Result};

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
    _project_root: PathBuf,
    config: LupaConfig,
}

impl LocalModelProvider {
    pub fn new(project_root: PathBuf, config: LupaConfig) -> Self {
        Self {
            _project_root: project_root,
            config,
        }
    }
}

impl QaProvider for LocalModelProvider {
    fn mode(&self) -> QaMode {
        QaMode::LocalModel
    }

    fn answer(&self, _request: &QaRequest) -> Result<QaAnswer> {
        if self.config.qa.model_path.trim().is_empty() {
            return Err(anyhow!(
                "qa.mode=local_model but qa.model_path is empty. Configure it in config.toml."
            ));
        }
        Err(anyhow!(
            "Local model provider is a stub for now. Keep qa.mode=extractive until worker integration."
        ))
    }
}

fn resolve_doc_path(project_root: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        p
    } else {
        project_root.join(p)
    }
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
