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

## Recommended stable run

```powershell
./scripts/bench.ps1 `
  -Root . `
  -Release `
  -Warmup `
  -Runs 5 `
  -Queries @("error","TODO","config","database","index","search","rust","panic","fn","impl") `
  -OutJson ./.lupa/bench/latest.json
```

## Regression gate example (CI/local)

```powershell
./scripts/bench.ps1 -Root . -Release -Warmup -Runs 3 -MaxP95Ms 50
```

If `overall.p95_ms` is above `-MaxP95Ms`, the script exits with code `2`.

## What it measures

- Total `index build` time.
- Per-query latency for `search --json`.
- Per-run stats: `min`, `max`, `avg`, `stddev`, `p50`, `p95`, `p99`.
- Overall stats across all runs.

## Notes

- Use a warm index (`-Warmup`) to approximate the typical `< 50ms` target.
- For large repositories (100k files), tune `include_extensions` and `max_file_size_bytes` for consistency.
- Use `-Release` for realistic user-facing latency numbers.

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
- `query_count`: `10`
- `overall.p95_ms`: `25`
- `latencies_ms`: `[15, 14, 15.01, 13, 25, 15.1, 14, 14.01, 14.01, 14]`
