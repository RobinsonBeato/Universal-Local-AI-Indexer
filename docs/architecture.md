# Architecture

## Overview

`lupa` is organized in three crates:

1. `crates/core` (`lupa-core`): indexing, storage, search, and QA providers.
2. `crates/cli` (`lupa`): command-line workflows.
3. `crates/gui` (`lupa-gui`): desktop interface using the same core.

## Storage model

### Tantivy index (disk)

- Path: `.lupa/index/`
- Schema fields:
  - `path` (`STRING | STORED`)
  - `name` (`TEXT | STORED`)
  - `content` (`TEXT`)
  - `mtime` (`FAST | STORED`)

### SQLite metadata

- Path: `.lupa/metadata.db`
- Main table `files`:
  - `path` (PK)
  - `mtime`
  - `size`
  - `hash`
  - `indexed_at`

## Indexing pipeline

1. Crawl filesystem with `walkdir` and configured excludes.
2. Compare file state (`mtime + size`) against SQLite metadata.
3. Optionally hash small files (`xxhash`) to avoid false-positive updates.
4. Extract content (text + optional structured extraction for `pdf/docx`).
5. Update Tantivy docs and SQLite metadata.
6. Remove deleted entries from both stores.

Modes:

- `index build`: fast metadata pass (quick readiness).
- `index backfill`: deeper content extraction pass.
- `index watch`: event-driven incremental updates with dirty-path batching.

## Search path

1. Parse user query with Tantivy parser (`name`, `path`, `content`).
2. Retrieve top docs.
3. Apply optional filters (`path_prefix`, `regex`, `limit`).
4. Rerank with heuristics (name/path match + recency).
5. Optionally compute snippets/highlights.

## QA architecture (Doc Chat)

Core exposes a provider interface:

- `QaProvider` trait
- `ExtractiveProvider`
- `LocalModelProvider`

Selection is runtime-configurable with `config.toml`:

- `qa.mode = "extractive"`: no model, deterministic and lightweight.
- `qa.mode = "local_model"`: local `llama-server` + GGUF model.

### Extractive provider

- Uses local file metadata and extracted snippets.
- Handles count-style questions deterministically.
- No external process.

### Local model provider

- Uses local runtime endpoint (`qa.endpoint`, default `127.0.0.1:8088`).
- Can auto-start `llama-server` (`qa.auto_start_server = true`).
- Uses document-aware prompt context.
- Applies anti-repetition settings and answer sanitization.
- Keeps inference fully offline.

## GUI integration notes

- QA requests are async and non-blocking for UI responsiveness.
- Chat panel is bound to selected document.
- Switching selected file closes current doc chat context.
- Chat mode toggle in UI (`Extractive` / `Local AI`) maps to `qa.mode`.

## Concurrency and performance

- `rayon` for parallel preprocessing/extraction.
- Single Tantivy writer for consistency and lock safety.
- Incremental update batches minimize full rebuilds.
- UI virtualization keeps large result sets responsive.

## Privacy defaults

Default excludes:

- `node_modules`
- `.git`
- `target`
- `.lupa`
- `AppData`
- `Program Files`
- `Windows`
- `System32`

## JSON output stability

`search --json` contract:

- `query`
- `total_hits`
- `took_ms`
- `hits[]` with `path`, `score`, `snippet`
