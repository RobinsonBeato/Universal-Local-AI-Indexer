# Basic benchmarks

This project prioritizes two metrics:

1. Index `N` files without failures.
2. Run 10 queries and report `p95`.

For Doc Chat, track a third metric:

3. First-answer latency (`extractive` and `local_model`) on a fixed document set.

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

## Suggested QA latency checks

Use a stable document and fixed prompts in both modes:

- `extractive`: target sub-second on warm cache.
- `local_model` (0.5B): expect higher latency; track median and p95.

Example prompts:

- `Summarize this document in 3 bullets.`
- `How many times does the word "casa" appear?`
- `When was this file modified?`

Record:

- startup-to-first-answer time
- warm-answer time
- answer length consistency
- repetition rate

## Notes

- Use a warm index (`-Warmup`) to approximate the typical `< 50ms` target.
- For large repositories (100k files), tune `include_extensions` and `max_file_size_bytes` for consistency.
- Use `-Release` for realistic user-facing latency numbers.
- For `local_model`, keep `max_tokens` moderate (120-256) to reduce repetition and latency spikes.

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
