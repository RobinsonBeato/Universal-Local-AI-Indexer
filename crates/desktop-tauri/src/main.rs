use std::path::PathBuf;
use std::process::Command;

use lupa_core::{
    provider_from_config, DoctorReport, IndexStats, LupaConfig, LupaEngine, QaMode, QaRequest,
    SearchOptions, SearchResult,
};
use serde::{Deserialize, Serialize};
use tauri::ClipboardManager;

#[derive(Debug, Deserialize)]
struct SearchRequest {
    root: String,
    query: String,
    limit: Option<usize>,
    path_prefix: Option<String>,
    regex: Option<String>,
    highlight: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BuildRequest {
    root: String,
    metadata_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DoctorRequest {
    root: String,
}

#[derive(Debug, Deserialize)]
struct PathRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
struct OpenAtMatchRequest {
    path: String,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AskDocumentRequest {
    root: String,
    document_path: String,
    question: String,
    mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct AskDocumentResponse {
    answer: String,
    citations: Vec<AskDocumentCitation>,
}

#[derive(Debug, Serialize)]
struct AskDocumentCitation {
    path: String,
    excerpt: String,
}

fn engine_for(root: &str) -> Result<LupaEngine, String> {
    let root_path = if root.trim().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(root)
    };
    let cfg = LupaConfig::load(&root_path).map_err(|e| e.to_string())?;
    LupaEngine::new(root_path, cfg).map_err(|e| e.to_string())
}

#[tauri::command]
fn search(req: SearchRequest) -> Result<SearchResult, String> {
    if req.query.trim().is_empty() {
        return Ok(SearchResult {
            query: String::new(),
            total_hits: 0,
            took_ms: 0,
            hits: Vec::new(),
        });
    }

    let engine = engine_for(&req.root)?;
    let opts = SearchOptions {
        limit: req.limit.unwrap_or(20),
        path_prefix: req.path_prefix.filter(|s| !s.trim().is_empty()),
        regex: req.regex.filter(|s| !s.trim().is_empty()),
        highlight: req.highlight.unwrap_or(true),
    };
    engine.search(&req.query, &opts).map_err(|e| e.to_string())
}

#[tauri::command]
fn build_index(req: BuildRequest) -> Result<IndexStats, String> {
    let engine = engine_for(&req.root)?;
    if req.metadata_only.unwrap_or(false) {
        engine
            .build_metadata_only_with_progress(|_| {})
            .map_err(|e| e.to_string())
    } else {
        engine
            .build_incremental_with_progress(|_| {})
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn doctor(req: DoctorRequest) -> Result<DoctorReport, String> {
    let engine = engine_for(&req.root)?;
    engine.doctor().map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(req: PathRequest) -> Result<(), String> {
    let p = PathBuf::from(req.path);
    if !p.exists() {
        return Err("Path does not exist".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(p.as_os_str())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Open failed with status: {status}"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("open_path is currently implemented for Windows only".to_string())
    }
}

#[tauri::command]
fn open_with(req: PathRequest) -> Result<(), String> {
    let p = PathBuf::from(req.path);
    if !p.exists() {
        return Err("Path does not exist".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("rundll32.exe")
            .arg("shell32.dll,OpenAs_RunDLL")
            .arg(p.as_os_str())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Open with failed with status: {status}"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("open_with is currently implemented for Windows only".to_string())
    }
}

#[tauri::command]
fn open_folder(req: PathRequest) -> Result<(), String> {
    let p = PathBuf::from(req.path);
    if !p.exists() {
        return Err("Path does not exist".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("explorer.exe")
            .arg("/select,")
            .arg(p.as_os_str())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Open folder failed with status: {status}"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("open_folder is currently implemented for Windows only".to_string())
    }
}

#[tauri::command]
fn copy_path(app: tauri::AppHandle, req: PathRequest) -> Result<(), String> {
    app.clipboard_manager()
        .write_text(req.path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_at_match(req: OpenAtMatchRequest) -> Result<(), String> {
    let p = PathBuf::from(req.path);
    if !p.exists() {
        return Err("Path does not exist".to_string());
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    #[cfg(target_os = "windows")]
    {
        let query = req.query.unwrap_or_default();
        if matches!(ext.as_str(), "txt" | "md" | "log" | "csv" | "json" | "xml" | "rs" | "js" | "ts" | "py") {
            // Best effort: open in Notepad and keep query available in status/UI.
            let status = Command::new("notepad.exe")
                .arg(p.as_os_str())
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("Open at match failed with status: {status}"));
            }
            if !query.trim().is_empty() {
                return Ok(());
            }
            return Ok(());
        }

        let status = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(p.as_os_str())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Open at match failed with status: {status}"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("open_at_match is currently implemented for Windows only".to_string())
    }
}

#[tauri::command]
fn pick_folder() -> Result<Option<String>, String> {
    let picked = tauri::api::dialog::blocking::FileDialogBuilder::new()
        .pick_folder()
        .map(|p| p.display().to_string());
    Ok(picked)
}

#[tauri::command]
fn ask_document(req: AskDocumentRequest) -> Result<AskDocumentResponse, String> {
    let root_path = if req.root.trim().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(&req.root)
    };

    let mut cfg = LupaConfig::load(&root_path).map_err(|e| e.to_string())?;
    let mode = match req.mode.as_deref() {
        Some("local_model") => QaMode::LocalModel,
        _ => QaMode::Extractive,
    };
    cfg.qa.mode = mode;

    let provider = provider_from_config(root_path, cfg);
    let answer = provider
        .answer(&QaRequest {
            document_path: req.document_path,
            question: req.question,
        })
        .map_err(|e| e.to_string())?;

    Ok(AskDocumentResponse {
        answer: answer.answer,
        citations: answer
            .citations
            .into_iter()
            .map(|c| AskDocumentCitation {
                path: c.path,
                excerpt: c.excerpt,
            })
            .collect(),
    })
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            search,
            build_index,
            doctor,
            open_path,
            open_with,
            open_folder,
            copy_path,
            open_at_match,
            pick_folder,
            ask_document
        ])
        .run(tauri::generate_context!())
        .expect("failed to run lupa desktop");
}
