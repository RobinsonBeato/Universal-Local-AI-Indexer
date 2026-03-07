# Windows Installer and First Release

## Scope

This guide covers how to produce the first public Windows installer for LUPA Desktop (`lupa-desktop-tauri`), test it locally, and validate basic release readiness.

## Prerequisites

- Windows 10/11
- Rust stable toolchain (`rustup default stable`)
- MSVC build tools (Visual Studio Build Tools / C++ workload)
- WebView2 Runtime available on target machines

## Build Installer

From repo root:

```powershell
cargo tauri build -p lupa-desktop-tauri
```

Expected output:

- NSIS installer (`.exe`): `target_local/release/bundle/nsis/`
- MSI installer (`.msi`): `target_local/release/bundle/msi/`

## Installer Configuration

Current config is defined in:

- `crates/desktop-tauri/tauri.conf.json`

Key points:

- Bundle is enabled (`"bundle.active": true`)
- Targets: NSIS + MSI
- App icon: `crates/desktop-tauri/icons/main-icon.ico`

## Local Validation Checklist

1. Install from NSIS `.exe`.
2. Launch app from Start Menu/Desktop shortcut.
3. Verify first-run onboarding:
   - language selection works
   - MIT terms must be accepted
   - optional local AI setup button responds
3. Verify icon appears correctly in:
   - installer
   - Start Menu
   - taskbar
4. Run smoke flow:
   - select index path
   - run `Build Index`
   - run a search
   - open file / open folder actions
6. Uninstall and confirm clean uninstall behavior.

## Publishing Notes

- Keep `LICENSE` (MIT) in repo root.
- Tag release with version aligned to `crates/desktop-tauri/tauri.conf.json`.
- Attach both installer artifacts to GitHub Release if you want users to choose (`.exe` or `.msi`).
