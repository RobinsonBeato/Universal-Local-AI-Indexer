# Architecture

## Overview

`lupa` uses a three-layer architecture:

1. `crates/core` (`lupa-core`): indexing, storage, and query.
2. `crates/cli` (`lupa`): command-line interface.
3. `crates/gui` (`lupa-gui`): desktop interface on top of the same core.

## Storage model

### Tantivy index (disk)

- Location: `.lupa/index/`
- Fields:
  - `path` (`STRING | STORED`)
  - `name` (`TEXT | STORED`)
  - `content` (`TEXT | STORED`)

### SQLite metadata

- Location: `.lupa/metadata.db`
- `files` table:
  - `path` (PK)
  - `mtime`
  - `size`
  - `hash` (optional)
  - `indexed_at`

## Incremental strategy

1. Traverse the filesystem with `walkdir` applying excludes (without restricting file types).
2. Compare against previous metadata (`mtime + size`).
3. If a file is small, compute `xxhash` to avoid reindexing from noisy timestamp changes.
4. Reindex only new/changed files.
5. Remove deleted paths from Tantivy and SQLite.

Note: all files are indexed by name/path. Content full-text is applied only to configured text extensions.
Additionally, real text extraction from `docx` and `pdf` is enabled by default.

## Concurrency

- Parallel document preprocessing (read + hash) with `rayon`.
- Single Tantivy writer for index writes (consistent commit model).

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

## CLI commands

- `lupa index build`: manual incremental indexing.
- `lupa index watch`: periodic incremental loop.
- `lupa search "<query>"`: full-text search with optional JSON output.
- `lupa doctor`: health checks for paths/permissions/index/db.

## JSON output stability

`search --json` returns a stable structure:

- `query`
- `total_hits`
- `took_ms`
- `hits[]` with `path`, `score`, `snippet`
