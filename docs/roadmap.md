# Roadmap

## Near Term (v0.1.x)

- Stabilize Windows installer UX (`.exe` + `.msi`).
- Improve first-run onboarding flow and copy.
- Add more benchmark baselines and publish p95 snapshots per release.
- Harden edge cases for file actions (`open`, `open with`, `open folder`).

## Mid Term (v0.2.x)

- Better query UX:
  - stronger suggestions
  - intent-aware filters
  - richer snippet relevance
- Thumbnail cache optimization and preview loading polish.
- Improve local AI setup experience and status reporting.

## Long Term (v0.3+)

- Cross-platform support hardening (macOS/Linux) in the same repo.
- Packaging automation and signed artifacts.
- Expanded observability for local benchmark diagnostics (still offline-first).

## Non-Goals

- No cloud dependency by default.
- No telemetry by default.
- No external file upload for core search.
