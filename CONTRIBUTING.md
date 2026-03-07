# Contributing to LUPA

Thanks for your interest in contributing.

## Ground Rules

- Keep the project offline-first and privacy-first.
- Do not add telemetry by default.
- Prefer incremental and performance-safe changes.
- Keep PRs small and focused.

## Development Setup

1. Install Rust stable toolchain.
2. Clone repo.
3. Run checks:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

## Branch and PR Flow

1. Create a branch from `main`.
2. Use clear commit messages.
3. Open a Pull Request with:
   - problem statement
   - solution summary
   - test evidence
   - screenshots for UI changes

## Performance Expectations

- Warm-index search target: `< 50ms p95` on SSD.
- Avoid regressions in indexing throughput and UI responsiveness.
- If your change can affect performance, include benchmark notes.

## Coding Notes

- Core search/index logic lives in `crates/core`.
- Desktop app host lives in `crates/desktop-tauri`.
- Web panel UI lives in `crates/gui/webpanel`.
- Keep platform-specific code behind `cfg(...)` gates.

## Security and Privacy

- Never upload local user files to external services by default.
- Avoid introducing default network dependencies.
- Respect excluded paths and user-local boundaries.

## Reporting Bugs

Use the bug report template and include:
- steps to reproduce
- expected vs actual behavior
- OS/version
- logs or screenshots if available
