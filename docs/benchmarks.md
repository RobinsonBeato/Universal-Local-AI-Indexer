# Basic benchmarks

This project prioritizes two metrics:

1. Index `N` files without failures.
2. Run 10 queries and report `p95`.

## Suggested script (PowerShell)

```powershell
./scripts/bench.ps1 -Root . -Queries @(
  "error", "TODO", "config", "database", "index", "search", "rust", "panic", "fn", "impl"
)
```

## What it measures

- Total `index build` time.
- Per-query latency for `search --json`.
- `p95` over the 10 searches.

## Notes

- Run benchmarks with a warm index (second run) to approximate the typical `< 50ms` target.
- On large repositories (100k files), tune `include_extensions` and `max_file_size_bytes` for consistency.

## Local reference run (2026-03-05)

Environment: Windows, benchmark on this repository (`-Root .`).

Command:

```powershell
./scripts/bench.ps1 -Root . -Queries @(
  "error","TODO","config","database","index","search","rust","panic","fn","impl"
)
```

Results:

- `index_build_ms`: `338.78`
- `queries`: `10`
- `p95_search_ms`: `25`
- `latencies_ms`: `[15, 14, 15.01, 13, 25, 15.1, 14, 14.01, 14.01, 14]`
