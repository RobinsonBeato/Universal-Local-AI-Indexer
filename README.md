# Universal Local AI Indexer (`lupa`)

Fast local indexing and search for Windows (portable architecture), offline-first and privacy-first.

`lupa` supports two document Q&A modes:

- `Extractive` (no model, fastest startup, no extra downloads)
- `Local AI` (optional GGUF model, still fully offline)

No cloud calls, no telemetry by default, no external services required.

## Core principles

- Offline-first
- Zero cloud cost
- Fast incremental indexing
- Stable JSON output for automation
- Privacy by default (sensible excludes)

## What gets indexed

- File name + full path for all file types.
- Text content for configured text extensions.
- Text extraction for `pdf` and `docx` (size-limited by config).

## Quickstart

### 1) Build and run

```bash
cargo build --release -p lupa
cargo run -p lupa-gui
```

### 2) Initial index

```bash
cargo run -p lupa -- index build
```

JSON:

```bash
cargo run -p lupa -- index build --json
```

### 3) Search

```bash
cargo run -p lupa -- search "connection error" --limit 20 --highlight --stats
```

JSON:

```bash
cargo run -p lupa -- search "query" --json
```

### 4) Keep index fresh

```bash
cargo run -p lupa -- index watch --interval-secs 2
```

### 5) Validate environment

```bash
cargo run -p lupa -- doctor
```

## CLI commands

- `lupa index build`
- `lupa index backfill`
- `lupa index watch`
- `lupa search "<query>" [--json] [--limit N] [--path-prefix ...] [--regex ...] [--highlight] [--stats]`
- `lupa doctor`

## GUI features

`lupa-gui` includes:

- root selector and index controls (`Build`, `Monitor`, `Doctor`)
- natural query parsing + suggestions
- categories and advanced filters
- virtualized result list for smooth scrolling
- preview panel with metadata, snippets, image preview, and quick actions
- `DOC CHAT` panel for selected document Q&A

## Doc Chat modes

### Extractive mode

- No model download.
- Fast and deterministic.
- Answers from local snippets and file metadata.

### Local AI mode

- Uses local `llama-server` + GGUF model.
- No internet during inference.
- Uses selected document context and returns concise answers.
- Honors question language (`es` / `en`) in current implementation.

Switch mode directly inside `DOC CHAT`.

## Local AI setup (Windows, one-time)

Use the smallest default profile (Qwen 0.5B Q4):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ai\setup-local-ai.ps1
```

Installed outside the repository:

- `%LOCALAPPDATA%\Lupa\runtime\` (runtime binaries and required DLLs)
- `%LOCALAPPDATA%\Lupa\models\qwen2.5-0.5b-instruct-q4_k_m.gguf`

Optional manual launch:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ai\run-local-ai-server.ps1
```

## `config.toml`

`lupa` loads `config.toml` from the app root.

```toml
excludes = ["node_modules", ".git", "target", ".lupa", "AppData", "Program Files", "Windows", "System32"]
include_extensions = ["txt", "md", "log", "rs", "toml", "json", "js", "ts", "py", "sql"]
max_file_size_bytes = 2097152
max_structured_file_size_bytes = 10485760
hash_small_file_threshold = 65536
threads = 0

[qa]
mode = "extractive" # "extractive" | "local_model"
model_path = "%LOCALAPPDATA%\\Lupa\\models\\qwen2.5-0.5b-instruct-q4_k_m.gguf"
endpoint = "http://127.0.0.1:8088"
llama_server_path = "%LOCALAPPDATA%\\Lupa\\runtime\\llama-server.exe"
auto_start_server = true
max_tokens = 256
timeout_ms = 12000
```

Notes:

- `threads = 0` uses available CPU cores.
- `max_structured_file_size_bytes` affects `pdf/docx` extraction budget.
- `qa.mode = "extractive"` keeps startup light and avoids model usage.

## Troubleshooting Local AI

- `qa.mode=local_model but qa.model_path is empty`
  - Ensure `[qa]` exists in `config.toml`.
- `local model server did not become ready`
  - Check `%LOCALAPPDATA%\Lupa\runtime\` has full runtime, not only one `.exe`.
  - Ensure VC++ runtime is installed (`Microsoft.VCRedist.2015+.x64`).
  - Increase `qa.timeout_ms` to `20000` for slower machines.
- Repetitive answers
  - Keep `max_tokens` moderate (e.g. `120-256`).
  - Use explicit questions tied to selected document.

## Privacy and repository safety

- Model/runtime files are installed in `%LOCALAPPDATA%`, not in this repo.
- Do not commit local artifacts under `models/`, `runtime/`, or temporary AI folders.

## Additional docs

- Architecture: [docs/architecture.md](docs/architecture.md)
- Benchmarks: [docs/benchmarks.md](docs/benchmarks.md)

## Quality gates

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
