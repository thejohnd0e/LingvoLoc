# Agent Instructions

## Repository Shape

- This is a Windows-first LingvoLoc app using npm workspaces, React/Vite/strict TypeScript, Tauri 2, and Rust.
- `apps/desktop/src` is the frontend; `apps/desktop/src-tauri/src` contains native commands, LM Studio runtime, adapters, history, detection, and lexical services.
- `apps/extension` is a separate Chromium MV3 client. It uses the authenticated loopback API at `127.0.0.1:47831`; it does not run a model or own translation data.
- Keep shared agent guidance in this file rather than duplicating it in tool-specific files.

## Commands

- Install with `npm ci` (Node.js 22+). Run `npm run check` for the complete local gate; its enforced order is formatting, desktop lint, desktop typecheck, extension typecheck/test, desktop tests, Rust format, and Rust tests.
- Focused checks: `npm run lint`, `npm run typecheck`, `npm test`, `npm run extension:typecheck`, and `npm run extension:test`.
- Rust checks use `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`; the stricter CI-style check is `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`.
- Use `npm run build` for the desktop frontend, `npm run desktop:build` for the Windows Tauri executable and NSIS installer, and `npm run extension:build` or `npm run extension:package` for the extension.
- Use `npm run lexical:index -- --stardict-dir <dir> --output <index.json>` only when regenerating lexical data; preserve each upstream dataset's license and attribution beside generated output.

## Runtime Constraints

- Desktop translation has two runtime modes (`runtimeMode` setting). `standalone` (default) scans a user-selected folder for `.gguf` models and runs them through a `llama-server.exe` (llama.cpp) that the app downloads itself (`runtimes/llama_download.rs`) or that the user points to, spawning it on a free loopback port (`runtimes/llama_server.rs`). `lmStudio` expects LM Studio's OpenAI-compatible API at `http://127.0.0.1:1234/v1`. The adapter is picked from the model file name (`adapters/mod.rs` `Family`): `gemma` → TranslateGemma, `hunyuan` → Hunyuan-MT, anything else (Qwen etc.) → generic chat; llama-server gets matching template flags. The stored `adapterId` setting is informational.
- Native builds require Rust, WebView2, and Windows C++ build tools/Windows SDK. The compiler tools may be available through Visual Studio without being on the normal `PATH`.
- User dictionaries are selected StarDict folders only; do not restore the removed app-data or bundled dictionary fallback. Dictionary media may be beside the dictionary or in `res.zip`.
- Release version `3.1.2` is duplicated in the root, desktop, extension, Tauri, and Cargo metadata (the extension ZIP name is taken from `apps/extension/package.json` by `scripts/package-extension.ps1`). Update all relevant locations together when changing it.

## Documentation Context

- Before substantial changes, read `STATUS.md`, `DECISIONS.md`, and `TODO.md`; they contain current capabilities, deliberate constraints, and remaining work.
- Treat executable configuration and package scripts as authoritative when they disagree with older prose, especially `package.json`, workspace manifests, Tauri config, and `.github/workflows/windows.yml`.
