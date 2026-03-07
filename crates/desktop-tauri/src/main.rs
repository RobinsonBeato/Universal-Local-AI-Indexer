use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use lupa_core::{
    provider_from_config, DoctorReport, IndexStats, LupaConfig, LupaEngine, QaMode, QaRequest,
    SearchOptions, SearchResult,
};
use serde::{Deserialize, Serialize};
use tauri::ClipboardManager;
use tauri::Manager;

#[derive(Debug, Clone, Deserialize)]
struct SearchRequest {
    root: String,
    query: String,
    limit: Option<usize>,
    path_prefix: Option<String>,
    regex: Option<String>,
    highlight: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct BuildRequest {
    root: String,
    metadata_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct DoctorRequest {
    root: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SnippetBatchRequest {
    root: String,
    query: String,
    paths: Vec<String>,
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

#[derive(Debug, Serialize)]
struct SnippetItem {
    path: String,
    snippet: String,
}

#[derive(Debug, Serialize)]
struct SnippetBatchResponse {
    items: Vec<SnippetItem>,
}

#[derive(Debug, Serialize)]
struct BootstrapResponse {
    project_root: String,
}

#[derive(Default)]
struct ModelServerState {
    child: Mutex<Option<Child>>,
}

#[derive(Default)]
struct CpuState {
    sample: Mutex<CpuSampleState>,
}

#[derive(Default, Clone, Copy)]
struct CpuSampleState {
    prev_idle: u64,
    prev_kernel: u64,
    prev_user: u64,
    initialized: bool,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime {
    dw_low_date_time: u32,
    dw_high_date_time: u32,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemTimes(
        lp_idle_time: *mut FileTime,
        lp_kernel_time: *mut FileTime,
        lp_user_time: *mut FileTime,
    ) -> i32;
}

#[cfg(target_os = "windows")]
fn file_time_to_u64(ft: FileTime) -> u64 {
    ((ft.dw_high_date_time as u64) << 32) | (ft.dw_low_date_time as u64)
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

fn expand_env_tokens(input: &str) -> String {
    let mut out = input.to_string();
    if out.contains("%LOCALAPPDATA%") {
        if let Ok(v) = std::env::var("LOCALAPPDATA") {
            out = out.replace("%LOCALAPPDATA%", &v);
        }
    }
    out
}

fn normalize_qa_paths(cfg: &mut LupaConfig) {
    if cfg.qa.model_path.trim().is_empty() {
        cfg.qa.model_path = "%LOCALAPPDATA%\\Lupa\\models\\qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string();
    }
    if cfg.qa.llama_server_path.trim().is_empty()
        || cfg.qa.llama_server_path.trim().eq_ignore_ascii_case("third_party/llama/llama-server.exe")
    {
        cfg.qa.llama_server_path = "%LOCALAPPDATA%\\Lupa\\runtime\\llama-server.exe".to_string();
    }
    if cfg.qa.endpoint.trim().is_empty() {
        cfg.qa.endpoint = "http://127.0.0.1:8088".to_string();
    }
}

#[cfg(target_os = "windows")]
fn shell_path(p: &PathBuf) -> String {
    let abs = if p.is_absolute() {
        p.clone()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    };
    let mut s = abs.display().to_string().replace('/', "\\");
    if let Some(rest) = s.strip_prefix("\\\\?\\") {
        s = rest.to_string();
    }
    s
}

#[cfg(target_os = "windows")]
fn ps_single_quote(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn spawn_powershell_hidden(script: &str) -> Result<(), String> {
    Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn server_alive(endpoint: &str) -> bool {
    let health = format!("{}/health", endpoint.trim_end_matches('/'));
    ureq::get(&health)
        .timeout(Duration::from_millis(600))
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    let e = endpoint.trim_end_matches('/');
    let pos = e.rfind(':')?;
    e[pos + 1..].parse::<u16>().ok()
}

fn ensure_model_server_running(
    state: &tauri::State<ModelServerState>,
    root: &str,
) -> Result<(), String> {
    let root_path = if root.trim().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(root)
    };
    let mut cfg = LupaConfig::load(&root_path).map_err(|e| e.to_string())?;
    normalize_qa_paths(&mut cfg);

    if !cfg.qa.auto_start_server {
        return Ok(());
    }
    if server_alive(&cfg.qa.endpoint) {
        return Ok(());
    }

    let model_path = PathBuf::from(expand_env_tokens(&cfg.qa.model_path));
    let server_path = PathBuf::from(expand_env_tokens(&cfg.qa.llama_server_path));
    if !server_path.exists() {
        return Err(format!(
            "llama-server executable not found: {}",
            server_path.display()
        ));
    }
    if !model_path.exists() {
        return Err(format!("model file not found: {}", model_path.display()));
    }

    let port = endpoint_port(&cfg.qa.endpoint).unwrap_or(8088).to_string();
    let host = "127.0.0.1";

    let mut guard = state.child.lock().map_err(|_| "model server lock poisoned".to_string())?;
    if guard.is_some() {
        return Ok(());
    }

    let mut cmd = Command::new(&server_path);
    cmd.arg("-m")
        .arg(&model_path)
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

    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to start llama-server: {e}"))?;
    *guard = Some(child);
    Ok(())
}

fn stop_model_server(state: &tauri::State<ModelServerState>) {
    if let Ok(mut guard) = state.child.lock() {
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
        }
        *guard = None;
    }
}

#[tauri::command]
fn bootstrap() -> Result<BootstrapResponse, String> {
    let project_root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .display()
        .to_string();
    Ok(BootstrapResponse { project_root })
}

#[tauri::command]
fn cpu_usage(cpu_state: tauri::State<CpuState>) -> Result<f32, String> {
    #[cfg(target_os = "windows")]
    {
        let mut idle = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
        if ok == 0 {
            return Err("GetSystemTimes failed".to_string());
        }

        let idle_now = file_time_to_u64(idle);
        let kernel_now = file_time_to_u64(kernel);
        let user_now = file_time_to_u64(user);
        let mut guard = cpu_state
            .sample
            .lock()
            .map_err(|_| "cpu sample lock poisoned".to_string())?;
        if !guard.initialized {
            guard.prev_idle = idle_now;
            guard.prev_kernel = kernel_now;
            guard.prev_user = user_now;
            guard.initialized = true;
            return Ok(0.0);
        }

        let idle_delta = idle_now.saturating_sub(guard.prev_idle);
        let kernel_delta = kernel_now.saturating_sub(guard.prev_kernel);
        let user_delta = user_now.saturating_sub(guard.prev_user);
        guard.prev_idle = idle_now;
        guard.prev_kernel = kernel_now;
        guard.prev_user = user_now;

        let total = kernel_delta.saturating_add(user_delta);
        if total == 0 {
            return Ok(0.0);
        }
        let busy = total.saturating_sub(idle_delta) as f64;
        let pct = ((busy / total as f64) * 100.0).clamp(0.0, 100.0) as f32;
        Ok(pct)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = cpu_state;
        Ok(0.0)
    }
}

#[tauri::command]
async fn search(req: SearchRequest) -> Result<SearchResult, String> {
    if req.query.trim().is_empty() {
        return Ok(SearchResult {
            query: String::new(),
            total_hits: 0,
            took_ms: 0,
            hits: Vec::new(),
        });
    }

    tauri::async_runtime::spawn_blocking(move || {
        let engine = engine_for(&req.root)?;
        let opts = SearchOptions {
            limit: req.limit.unwrap_or(20),
            path_prefix: req.path_prefix.filter(|s| !s.trim().is_empty()),
            regex: req.regex.filter(|s| !s.trim().is_empty()),
            highlight: req.highlight.unwrap_or(true),
        };
        engine.search(&req.query, &opts).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("search task join error: {e}"))?
}

#[tauri::command]
async fn build_index(req: BuildRequest) -> Result<IndexStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| format!("build task join error: {e}"))?
}

#[tauri::command]
async fn doctor(req: DoctorRequest) -> Result<DoctorReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let engine = engine_for(&req.root)?;
        engine.doctor().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("doctor task join error: {e}"))?
}

#[tauri::command]
async fn fetch_snippets(req: SnippetBatchRequest) -> Result<SnippetBatchResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let engine = engine_for(&req.root)?;
        let pairs = engine
            .snippets_for_paths(&req.paths, &req.query)
            .map_err(|e| e.to_string())?;
        Ok::<SnippetBatchResponse, String>(SnippetBatchResponse {
            items: pairs
                .into_iter()
                .map(|(path, snippet)| SnippetItem { path, snippet })
                .collect(),
        })
    })
    .await
    .map_err(|e| format!("snippet task join error: {e}"))?
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
        let path_arg = shell_path(&p);
        let launched = Command::new("OpenWith.exe").arg(&path_arg).spawn();
        if launched.is_ok() {
            return Ok(());
        }

        let escaped = ps_single_quote(&path_arg);
        let script =
            format!("$p='{escaped}'; Start-Process -FilePath $p -Verb OpenAs -ErrorAction SilentlyContinue");
        spawn_powershell_hidden(&script)
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
        let path_arg = shell_path(&p);
        let escaped = ps_single_quote(&path_arg);

        // Use the exact file path already known by the app and ask Explorer to select it.
        // Passing '/select,' and path as separate argument avoids malformed parsing.
        let select_script = format!(
            "$p='{escaped}'; Start-Process explorer.exe -ArgumentList '/select,', $p"
        );
        if spawn_powershell_hidden(&select_script).is_ok() {
            return Ok(());
        }

        // Fallback: open the parent directory for the same file path.
        let parent = p
            .parent()
            .map(|v| v.to_path_buf())
            .unwrap_or_else(|| p.clone());
        let parent_arg = shell_path(&parent);
        let parent_escaped = ps_single_quote(&parent_arg);
        let fallback_script = format!("$p='{parent_escaped}'; Start-Process explorer.exe -ArgumentList $p");
        spawn_powershell_hidden(&fallback_script)
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
fn pick_folder(model_state: tauri::State<ModelServerState>) -> Result<Option<String>, String> {
    let picked = tauri::api::dialog::blocking::FileDialogBuilder::new()
        .pick_folder()
        .map(|p| p.display().to_string());
    if let Some(root) = picked.as_deref() {
        let _ = ensure_model_server_running(&model_state, root);
    }
    Ok(picked)
}

#[tauri::command]
fn ask_document(
    req: AskDocumentRequest,
    model_state: tauri::State<ModelServerState>,
) -> Result<AskDocumentResponse, String> {
    let root_path = if req.root.trim().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(&req.root)
    };

    let mut cfg = LupaConfig::load(&root_path).map_err(|e| e.to_string())?;
    normalize_qa_paths(&mut cfg);
    let mode = match req.mode.as_deref() {
        Some("local_model") => QaMode::LocalModel,
        _ => QaMode::Extractive,
    };
    cfg.qa.mode = mode;
    if cfg.qa.mode == QaMode::LocalModel {
        let _ = ensure_model_server_running(&model_state, &req.root);
    }

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

#[tauri::command]
fn install_local_ai() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let script = cwd.join("scripts").join("ai").join("setup-local-ai.ps1");
        if !script.is_file() {
            return Err(format!(
                "Local AI setup script not found: {}",
                script.display()
            ));
        }
        let status = Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Local AI install failed with status: {status}"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("install_local_ai is currently implemented for Windows only".to_string())
    }
}

fn main() {
    tauri::Builder::default()
        .manage(ModelServerState::default())
        .manage(CpuState::default())
        .setup(|app| {
            let app_handle = app.handle();
            std::thread::spawn(move || {
                let root = std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .display()
                    .to_string();
                let state = app_handle.state::<ModelServerState>();
                let _ = ensure_model_server_running(&state, &root);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            cpu_usage,
            search,
            build_index,
            doctor,
            fetch_snippets,
            open_path,
            open_with,
            open_folder,
            copy_path,
            open_at_match,
            pick_folder,
            ask_document,
            install_local_ai
        ])
        .on_window_event(|event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event.event() {
                let state = event.window().state::<ModelServerState>();
                stop_model_server(&state);
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run lupa desktop");
}
