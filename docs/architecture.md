# Architecture

## Overview

LUPA is organized in four crates:

1. `crates/core` (`lupa-core`): indexing, metadata storage, search, snippets, QA providers.
2. `crates/cli` (`lupa`): CLI workflows (`index`, `search`, `doctor`).
3. `crates/gui` (`lupa-gui`): legacy egui desktop frontend.
4. `crates/desktop-tauri` (`lupa-desktop-tauri`): current desktop app (Tauri + WebPanel).

The single source of search/index logic is `lupa-core`.

## Mandatory engine decisions

- Full-text engine: Tantivy (disk index).
- Metadata store: SQLite.
- Parallel workload: Rayon.
- Incremental strategy: `mtime + size` + optional `xxhash` for small files.
- Offline-first and privacy-first by default.

## Storage Model

### Tantivy index

- Location: `.lupa/index/`
- Main fields:
  - `path` (`STRING | STORED`)
  - `name` (`TEXT | STORED`)
  - `content` (`TEXT`)
  - `mtime` (`FAST | STORED`)

### SQLite metadata

- Location: `.lupa/metadata.db`
- Main table: `files`
  - `path` (PK)
  - `mtime`
  - `size`
  - `hash`
  - `indexed_at`

## Indexing Pipeline

1. Walk filesystem with configured includes/excludes.
2. Compare candidate file state against SQLite metadata.
3. Mark changed/new/deleted files.
4. Extract text content:
   - plain text/source directly
   - optional structured extraction for `pdf/docx` (size-limited)
5. Commit updates to Tantivy and SQLite.
6. Remove deleted entries from both stores.

Modes:

- `index build`: fast metadata pass for quick readiness.
- `index backfill`: deep content pass to improve text/snippet coverage.
- `index watch`: event-driven incremental updates with dirty-path batching.

## Search Pipeline

1. Parse query with Tantivy parser on `name`, `path`, `content`.
2. Retrieve top candidates.
3. Apply filters (`limit`, `path_prefix`, `regex`).
4. Re-rank by relevance + practical heuristics (filename/path recency effects).
5. Build snippets/highlights when requested.
6. Return stable result payload.

## Desktop Architecture (Tauri + WebPanel)

### UI runtime

- Frontend: `crates/gui/webpanel/` (`index.html`, `app.js`, `styles.css`, assets/icons).
- Host: Tauri window from `crates/desktop-tauri`.
- IPC: frontend calls Rust commands via `window.__TAURI__.invoke(...)`.

### Tauri command boundary

Rust commands exposed by `lupa-desktop-tauri` include:

- `search`
- `build_index`
- `doctor`
- `pick_folder`
- file actions (`open_path`, `open_with`, `open_folder`, `copy_path`, `open_at_match`)

This keeps visual changes independent from indexing/search internals.

### UI behavior highlights

- Collection filters and advanced filters on left panel.
- Incremental "load more" for result rows.
- Thumbnail/image preview on demand.
- Right panel supports preview mode and document chat mode.
- Keyboard support for result navigation and open action.

## QA / Doc Chat Architecture

Provider abstraction in core:

- `QaProvider` trait
- `ExtractiveProvider`
- `LocalModelProvider`

Runtime config (`config.toml`):

- `qa.mode = "extractive"`: no model/runtime required.
- `qa.mode = "local_model"`: local `llama-server` + GGUF.

### Extractive mode

- Deterministic local answers.
- Uses snippets + metadata only.

### Local model mode

- Fully local inference.
- Optional auto-start of local model server.
- Context is bound to selected document.

## Concurrency and Performance

- Rayon parallelizes crawl, extraction, and preprocessing tasks.
- Tantivy writer access is serialized to keep index consistency.
- Dirty-path batching avoids expensive full rebuilds.
- UI keeps interactions smooth with incremental rendering strategies.

## Privacy Defaults

Default excluded paths:

- `node_modules`
- `.git`
- `target`
- `.lupa`
- `AppData`
- `Program Files`
- `Windows`
- `System32`

## Stable JSON Contract

`search --json` returns:

- `query`
- `total_hits`
- `took_ms`
- `hits[]` (`path`, `score`, optional `snippet`, metadata fields when available)
