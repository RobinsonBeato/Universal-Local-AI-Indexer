# WebPanel Frontend

This folder contains the desktop UI frontend used by `lupa-desktop-tauri`.

## Current role

- Main desktop UI layer (HTML/CSS/JS + Web Components).
- Talks to Rust through Tauri IPC (`window.__TAURI__.invoke`).
- Keeps search/index logic in Rust core while enabling fast UI iteration.

## Files

- `index.html`: app shell entrypoint.
- `app.js`: component rendering + event wiring + Tauri command calls.
- `styles.css`: single-source styles for the full UI.
- `assets/icons/`: SVG icon set for sidebar/actions.
- `state.json`: optional bridge-mode sample state.

## Run in Desktop Mode (recommended)

From workspace root:

```powershell
cargo run -p lupa-desktop-tauri
```

This is the production path for the current UI.

## Optional Bridge Mode (frontend-only visual iteration)

You can still run the panel in a browser for visual work:

```powershell
powershell -ExecutionPolicy Bypass -File .\crates\gui\webpanel\serve.ps1
```

Then open:

- `http://127.0.0.1:4173/index.html`

In bridge mode, UI can read from `state.json` as mock state (no real desktop actions).

## Notes

- Keep icons and static assets under `assets/` to avoid polluting JS.
- Avoid moving business logic into frontend code; keep it in `lupa-core`.
