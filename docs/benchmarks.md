# Benchmarks

## Goals

LUPA tracks these baseline metrics:

1. Index `N` files without failures.
2. Execute 10 queries and report `p95` latency.
3. Keep warm-index search typically under `50ms` p95 on SSD.

For Doc Chat (optional), also track:

4. first-answer latency for `extractive` and `local_model`.

## Benchmark Script

PowerShell helper:

```powershell
./scripts/bench.ps1 -Root . -Queries @(
  "error","TODO","config","database","index","search","rust","panic","fn","impl"
)
```

Recommended stable run:

```powershell
./scripts/bench.ps1 `
  -Root . `
  -Release `
  -Warmup `
  -Runs 5 `
  -Queries @("error","TODO","config","database","index","search","rust","panic","fn","impl") `
  -OutJson ./.lupa/bench/latest.json
```

## Regression Gate

```powershell
./scripts/bench.ps1 -Root . -Release -Warmup -Runs 3 -MaxP95Ms 50
```

If `overall.p95_ms` exceeds `-MaxP95Ms`, script exits with code `2`.

## What Is Measured

- `index build` duration.
- Per-query latency for `search --json`.
- Stats per run: `min`, `max`, `avg`, `stddev`, `p50`, `p95`, `p99`.
- Aggregate stats across runs.

## Desktop UX Perf Checks (manual)

When validating `lupa-desktop-tauri`, also check:

- Result list scroll smoothness under large hit sets.
- Selection latency (click/keyboard) without scroll jump.
- Right panel preview load behavior (image preview on demand).
- Search-to-render perceived delay with snippets enabled.

## QA Latency Checks (Doc Chat)

Use a fixed document and prompts in both modes:

- `extractive`: target sub-second warm response.
- `local_model` (small GGUF): expect higher latency; track p50/p95.

Example prompts:

- `Summarize this document in 3 bullets.`
- `How many times does the word "casa" appear?`
- `When was this file modified?`

Track:

- startup-to-first-answer time
- warm-answer time
- answer consistency
- repetition rate

## Notes

- Always benchmark with a warm index for realistic daily usage.
- Use `-Release` for production-grade numbers.
- For large datasets (100k+ files), tune:
  - `include_extensions`
  - `max_file_size_bytes`
  - `max_structured_file_size_bytes`
- For local model mode, keep `max_tokens` moderate (`120-256`) to reduce latency spikes.

## Local Reference Run (2026-03-05)

Environment: Windows, repository root as benchmark dataset.

Results:

- `index_build_ms`: `338.78`
- `query_count`: `10`
- `overall.p95_ms`: `25`
- `latencies_ms`: `[15, 14, 15.01, 13, 25, 15.1, 14, 14.01, 14.01, 14]`
