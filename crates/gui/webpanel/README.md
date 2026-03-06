# Web Panel (Desktop UI Bridge)

This folder is the first step to move desktop UI styling to web technology while keeping Rust core logic.

## What it does

- `lupa-gui` exports live UI state to `state.json`.
- `index.html` renders that state using Web Components.
- This lets you iterate visual design 1:1 with web styling, without touching search/index performance code.

## Run flow

1. Start GUI:

```powershell
cargo run -p lupa-gui
```

2. Start the included local static server:

```powershell
powershell -ExecutionPolicy Bypass -File .\crates\gui\webpanel\serve.ps1
```

3. Open `http://127.0.0.1:4173/index.html`.

The page refreshes from `state.json` every ~350ms.

## Next step

Embed this web panel in a desktop shell (Tauri/Wry) and wire actions back to Rust via IPC commands.
