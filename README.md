# LingvoLoc

## Overview

LingvoLoc is a Windows-first desktop application for private, local translation. The first implementation will translate through a selectable TranslateGemma model served by LM Studio while keeping model and runtime concerns behind explicit abstractions.

## Setup

Install Node.js 22+, Rust, WebView2, and the Windows C++ build tools. Then run:

```powershell
npm install
```

LM Studio should expose its OpenAI-compatible API at `http://127.0.0.1:1234/v1`.

## Usage

Run the web UI with `npm run dev` or the native desktop shell with `npm run desktop:dev`.
The app persists endpoint, model, adapter, language selections, and two independently selected pair languages locally. Automatic source detection is enabled by default; successful translations are copied to the clipboard when WebView2 allows clipboard access.

## Build

Use `npm run build` for the frontend production build. Each production build increments `BUILD_NUMBER`, which is shown in the desktop UI. Use `npm run desktop:build` to compile the Tauri executable and create the x64 NSIS installer at `apps/desktop/src-tauri/target/release/bundle/nsis/`.

## Testing

Run `npm test` for Vitest and `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` for Rust tests. Run the full local checks with `npm run check`.

To build the compact lexical index from downloaded source files, run `npm run lexical:index -- --stardict-dir <directory> --stardict-dir <directory> --output <index.json>`. The converter accepts FreeDict StarDict directories, Kaikki JSONL (`--kaikki-language <code>`), and a tab-separated morphology file with `lemma<TAB>form` rows (`--morphology-language <code>`). It keeps language and lemma as separate identity fields, so additional legally compatible language pairs can be added without collisions. It preserves definitions, examples, forms, translations, and provider labels; retain the upstream license and attribution notices alongside the generated index.

User StarDict dictionaries can be placed under the directory shown in the Dictionary lookup `User StarDict dictionaries` panel. Extract each archive into its own subdirectory, preserving `.ifo`, `.idx` or `.idx.gz`, and `.dict` or `.dict.dz`; then press `Refresh dictionaries` and enable the dictionaries with checkboxes. Nested `res.zip` files are optional resources and are ignored. The lookup scans only enabled dictionaries together with the bundled indexes.

The native executable and NSIS installer build successfully on this machine with the branded LingvoLoc icon. Startup smoke passes; an interactive window smoke test remains pending.

The project is licensed under MIT; see `LICENSE`.

## Project Documentation

- `AGENTS.md`: shared instructions for coding agents.
- `STATUS.md`: current state, issues, and next step.
- `DECISIONS.md`: important technical decisions and rationale.
- `TODO.md`: pending work.
- `docs/IMPLEMENTATION_PLAN.md`: agreed Phase 0-1 implementation plan.
- `docs/LOCAL_API.md`: authenticated loopback API contract for the future browser extension.
- `apps/extension/`: Chromium MV3 extension source; build with `npm run extension:build`.
