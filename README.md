# LingvoLoc

## Overview

LingvoLoc is a Windows-first desktop application for private, local translation with a selectable TranslateGemma model. Everything runs on your computer.

![LingvoLoc main window](docs/screenshot.png)

## Install

Download the latest `LingvoLoc_x.y.z_x64-setup.exe` from the [Releases](../../releases) page and run it. The installer checks for the components LingvoLoc needs and installs anything that is missing.

## First run

LingvoLoc works out of the box in **Standalone** mode and needs no other software:

1. Open **Settings** (gear icon, top right) and press **Download llama.cpp**. LingvoLoc picks the build that fits your computer (CUDA for NVIDIA, Vulkan for other GPUs, CPU otherwise), downloads it, and checks it.
2. Choose the **Models folder** containing your `.gguf` files.
3. Pick a model in the main window and translate.

The **GPU support** row shows your video card(s) and which llama.cpp build will be downloaded, and, once llama.cpp is installed, which device it actually uses. With several GPUs (for example a discrete card and an integrated one) LingvoLoc pins the strongest one. llama.cpp is installed into one fixed folder, so pressing **Update llama.cpp** later replaces it in place, reports when the latest release is already installed, and never changes the saved path.

The first translation is slower while the model loads: LingvoLoc starts the model server on the first translation, restarts it when you switch models, and stops it on exit. **Add to PATH** puts the llama.cpp folder into your user PATH (no administrator rights needed) so you can also run it from a terminal. **Advanced** lets you point to an existing llama.cpp folder or find one already in PATH.

Prefer LM Studio? Switch **Mode** to `LM Studio` and expose its OpenAI-compatible API at `http://127.0.0.1:1234/v1`.

## Settings

The **Settings** window (gear icon) has three sections:

- **Model runtime**: Standalone or LM Studio mode, models folder, GPU support, and llama.cpp.
- **Dictionaries**: choose the folder with your StarDict dictionaries and enable the ones to search. Articles from different dictionaries are shown as separate cards, each labeled with its dictionary name.
- **Browser extension**: **Copy token** copies the pairing token for the browser extension.

## Tray icon

Left-click the tray icon to show or hide the main window. Right-click opens the menu:

- **Open LingvoLoc** (`Ctrl+Shift+L`)
- **Translate clipboard** (`Ctrl+Shift+T`): translates the clipboard text and shows it in a small popup window.
- **Settings…**: opens the main window with the settings.
- **Start with Windows**: starts LingvoLoc hidden in the tray when you sign in.
- **Quit**

## Browser extension

The Chromium extension (`LingvoLoc-extension-x.y.z.zip` on the Releases page) translates selected text through the desktop app. Load it from `chrome://extensions` with Developer mode enabled, press **Copy token** in LingvoLoc Settings, and paste the token into the extension.

Select text and use the toolbar button or the right-click menu **Translate selection with LingvoLoc**. Both open the same in-page window, which you can drag by its header and resize by its right edge, bottom edge, or corner; the text and translation fields grow with it and the size is remembered. The **⧉** button opens the current content in a separate browser window that stays open and keeps its content when you switch tabs. On pages where extensions cannot inject content (such as `chrome://` pages) the separate window opens instead. See `docs/BROWSER_EXTENSION.md` for details.

## Usage

The status pill in the header shows the runtime and the selected model. The app persists endpoint, model, adapter, language selections, two independently selected pair languages, and text-size settings locally. Automatic source detection is enabled by default; successful translations are copied to the clipboard when WebView2 allows clipboard access. The clipboard popup has its own text-size setting, can be resized, and fits its height to its content.

## Development

The sections below are for contributors building LingvoLoc from source.

### Requirements

Node.js 22+, Rust, WebView2, and the Windows C++ build tools. Then run:

```powershell
npm install
```

Run the web UI with `npm run dev` or the native desktop shell with `npm run desktop:dev`.

### Standalone runtime internals

In Standalone mode LingvoLoc downloads the newest llama.cpp Windows x64 release from GitHub into its app data folder (`llama.cpp/`, a fixed folder whose release is recorded in `version.txt`), chooses the archive from the detected GPU, starts `llama-server` on a free local port when the first translation runs, restarts it when you switch models, and stops it on exit.

### Build

Use `npm run build` for the frontend production build. The application version is managed in the package, Tauri, and extension manifests and is shown in the desktop UI. Use `npm run desktop:build` to compile the Tauri executable and create the x64 NSIS installer at `apps/desktop/src-tauri/target/release/bundle/nsis/`. The installer lets the user choose between a per-user installation in Local AppData and an all-users installation in Program Files, links the C runtime statically, and downloads the WebView2 runtime when it is not installed.

### Testing

Run `npm test` for Vitest and `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` for Rust tests. Run the full local checks with `npm run check`.

To build the compact lexical index from downloaded source files, run `npm run lexical:index -- --stardict-dir <directory> --stardict-dir <directory> --output <index.json>`. The converter accepts FreeDict StarDict directories, Kaikki JSONL (`--kaikki-language <code>`), and a tab-separated morphology file with `lemma<TAB>form` rows (`--morphology-language <code>`). It keeps language and lemma as separate identity fields, so additional legally compatible language pairs can be added without collisions. It preserves definitions, examples, forms, translations, and provider labels; retain the upstream license and attribution notices alongside the generated index.

Use `Choose folder` in the **Dictionaries** section of **Settings** (gear icon, top right) to select the directory containing your StarDict dictionary folders. Extract each archive into its own subdirectory, preserving `.ifo`, `.idx` or `.idx.gz`, and `.dict` or `.dict.dz`; keep referenced media files such as `.wav`, `.mp3`, `.ogg`, `.flac`, `.m4a`, `.aac`, `.jpg`, `.jpeg`, `.png`, `.gif`, or `.webp` beside the dictionary data, or keep them in the dictionary's `res.zip`. Then press `Refresh dictionaries` and enable the dictionaries with checkboxes. The lookup scans only enabled dictionaries selected by the user; parsed dictionaries are cached in memory and reloaded when their files change. The `Additional` section with the dictionary lookup opens automatically when a lookup starts. Audio controls and images are loaded on demand when a dictionary entry references available media. LingvoLoc no longer creates or uses an app-data dictionary directory.

The native executable and NSIS installer build successfully on this machine with the branded LingvoLoc icon. The Chromium extension is packaged with `npm run extension:package` and uses the local loopback API after pairing. Its source and target languages are independent from desktop settings; the context-menu popup is in-page, movable, resizable, and supports copying the result.

The project is licensed under MIT; see `LICENSE`.

## Project Documentation

- `AGENTS.md`: shared instructions for coding agents.
- `STATUS.md`: current state, issues, and next step.
- `DECISIONS.md`: important technical decisions and rationale.
- `TODO.md`: pending work.
- `docs/IMPLEMENTATION_PLAN.md`: agreed Phase 0-1 implementation plan.
- `docs/LOCAL_API.md`: authenticated loopback API contract for the future browser extension.
- `apps/extension/`: Chromium MV3 extension source; build with `npm run extension:build`.
