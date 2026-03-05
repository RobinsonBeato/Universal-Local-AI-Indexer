# Universal Local AI Indexer (`lupa`)

Local file indexer and search engine for Windows (also portable to Linux/macOS), offline-first, with no AI and no external services.

## Goals

- Offline-first: no external APIs, no file uploads, no telemetry by default.
- $0 cost: no models, embeddings, vector DB, or cloud.
- Ultra-fast: Tantivy full-text + SQLite metadata + incremental indexing.
- Privacy by default: sensitive excludes are preconfigured.

## What it indexes

- All file types by `name` and `path` (includes Word files, images, binaries, etc.).
- Full-text content for:
  - configured text extensions (`txt`, `md`, `log`, source code, etc.)
  - `docx` (real internal text)
  - `pdf` (real internal text)

## Quickstart

### 1) Build

```bash
cargo build --release -p lupa
```

GUI (desktop):

```bash
cargo run -p lupa-gui
```

### 2) First-time indexing

```bash
cargo run -p lupa -- index build
```

JSON output:

```bash
cargo run -p lupa -- index build --json
```

### 3) Search

```bash
cargo run -p lupa -- search "connection error" --limit 20 --highlight --stats
```

JSON output for scripts:

```bash
cargo run -p lupa -- search "query" --json
```

### 4) Watch mode (incremental)

```bash
cargo run -p lupa -- index watch --interval-secs 2
```

`index watch` uses filesystem events + a `dirty paths` queue to reindex only changed files.

### 5) Local diagnostics

```bash
cargo run -p lupa -- doctor
```

## CLI commands

- `lupa index build`
- `lupa index watch`
- `lupa search "<query>" [--json] [--limit N] [--path-prefix ...] [--regex ...] [--highlight] [--stats]`
- `lupa doctor`

## Graphical Interface

The `lupa-gui` app provides:

- root selector
- `Index Build`
- `Watch Start/Stop`
- `Doctor`
- search with `limit`, `path_prefix`, `regex`, `highlight`
- results panel + activity panel
- side preview for the selected result:
  - contextual snippet around the query (text/docx/pdf)
  - enlarged image preview when available
  - metadata + quick actions (`Open`, `Folder`)

### Doc Chat modes

Right panel `DOC CHAT` supports two modes:

- `Extractive`: no model, answers from local snippets/metadata.
- `Local AI`: uses local `llama-server` + GGUF model (offline).

You can switch modes directly in the chat panel.

## Local AI one-time setup (Windows)

Install runtime + tiny model once:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ai\setup-local-ai.ps1
```

This installs files in:

- `%LOCALAPPDATA%\Lupa\runtime\llama-server.exe`
- `%LOCALAPPDATA%\Lupa\models\qwen2.5-0.5b-instruct-q4_k_m.gguf`

Then in GUI, open `DOC CHAT` and select `Local AI`.

Optional: run server manually for debugging:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\ai\run-local-ai-server.ps1
```

## Configuration (optional `config.toml`)

If present in the project root, it is loaded automatically.

```toml
excludes = ["node_modules", ".git", "target", ".lupa", "AppData", "Program Files", "Windows", "System32"]
include_extensions = ["txt", "md", "log", "rs", "toml", "json", "js", "ts", "py", "sql"]
max_file_size_bytes = 2097152
max_structured_file_size_bytes = 10485760
hash_small_file_threshold = 65536
threads = 0
```

- `threads = 0`: uses available CPU cores.
- `hash_small_file_threshold`: for small files, computes `xxhash` to avoid reindexing when content did not change.
- `max_structured_file_size_bytes`: limit for `pdf/docx` text extraction (increase it if snippets are missing in large files).

## Default excludes (privacy)

- `node_modules`
- `.git`
- `target`
- `.lupa`
- `AppData`
- `Program Files`
- `Windows`
- `System32`

## Architecture

Summary: [docs/architecture.md](docs/architecture.md)

## Basic benchmarks

Guide and script: [docs/benchmarks.md](docs/benchmarks.md)

## DoD verification

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
