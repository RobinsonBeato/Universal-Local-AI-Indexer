use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LupaConfig {
    pub excludes: Vec<String>,
    // Extensiones que se leerán como texto para full-text en contenido.
    pub include_extensions: Vec<String>,
    pub max_file_size_bytes: u64,
    pub hash_small_file_threshold: u64,
    pub threads: usize,
}

impl Default for LupaConfig {
    fn default() -> Self {
        Self {
            excludes: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "target".to_string(),
                ".lupa".to_string(),
                "AppData".to_string(),
                "Program Files".to_string(),
                "Windows".to_string(),
                "System32".to_string(),
            ],
            include_extensions: vec![
                "txt", "md", "log", "rs", "toml", "json", "yaml", "yml", "js", "ts", "tsx", "jsx",
                "py", "java", "go", "cs", "cpp", "h", "hpp", "html", "css", "sh", "ps1", "sql",
                "xml", "ini",
            ]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
            max_file_size_bytes: 2 * 1024 * 1024,
            hash_small_file_threshold: 64 * 1024,
            threads: 0,
        }
    }
}

impl LupaConfig {
    pub fn load(project_root: &Path) -> Result<Self> {
        let cfg_path = project_root.join("config.toml");
        if !cfg_path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(&cfg_path)
            .with_context(|| format!("no se pudo leer {}", cfg_path.display()))?;
        let cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("config.toml inválido en {}", cfg_path.display()))?;
        Ok(cfg)
    }

    pub fn should_exclude(&self, path: &Path) -> bool {
        let lower = path.to_string_lossy().to_lowercase();
        self.excludes
            .iter()
            .any(|needle| lower.contains(&needle.to_lowercase()))
    }

    pub fn is_text_extension(&self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };

        let ext = ext.to_lowercase();
        self.include_extensions
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&ext))
    }

    pub fn effective_threads(&self) -> usize {
        if self.threads == 0 {
            std::thread::available_parallelism().map_or(4, |n| n.get())
        } else {
            self.threads
        }
    }

    pub fn data_dir(project_root: &Path) -> PathBuf {
        project_root.join(".lupa")
    }
}

#[cfg(test)]
mod tests {
    use super::LupaConfig;
    use std::path::Path;

    #[test]
    fn default_excludes_match_required_paths() {
        let cfg = LupaConfig::default();
        for p in [
            "node_modules",
            ".git",
            "target",
            ".lupa",
            "AppData",
            "Program Files",
            "Windows",
            "System32",
        ] {
            assert!(cfg.should_exclude(Path::new(p)));
        }
    }
}
