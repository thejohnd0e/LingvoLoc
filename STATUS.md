# Status

## Current Release

- Version `2.1.1` is the current release across the root, desktop, extension, Tauri, Cargo, and extension packaging metadata.
- `v2.1.1` is published on GitHub Releases (installer and extension ZIP attached). The last recorded release gate passed: `npm run check`, `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`, and `npm run desktop:build`.
- Release artifacts are `apps/desktop/src-tauri/target/release/bundle/nsis/LingvoLoc_2.1.1_x64-setup.exe` and `apps/extension/LingvoLoc-extension-2.1.1.zip`.

## Completed

- The Windows desktop app translates through the selected local model (Standalone llama.cpp or LM Studio), supports automatic detection for 12 languages, persists settings and SQLite history, copies successful results, and provides tray, single-instance, global-shortcut, and clipboard-popup workflows.
- The authenticated API on `127.0.0.1:47831` supports the thin Chromium MV3 extension. Pairing, independent language directions, toolbar translation, and a movable/resizable in-page context-menu popup have been implemented and smoke-tested.
- Dictionary lookup uses only user-selected StarDict folders. Dictionary selection is persisted; `.dict.dz`, common audio/image references, and `res.zip` media are supported. Bundled and app-data dictionary fallbacks are intentionally not used.
- Translation word selection supports model-assisted forward and reverse highlighting without adding word requests to translation history.
- The desktop and extension have responsive layouts, visible focus states, persisted text-size controls, and current LingvoLoc branding.
- 2.1.1 work: the model adapter follows the model file name (TranslateGemma, Hunyuan-MT, generic chat for Qwen etc.) and llama-server gets matching chat-template flags; the status line shows response time and tokens/s; README documents the RTX 3060 model benchmark.
- 2.0.5 work: Standalone is the default mode; Settings has Download/Update llama.cpp (GPU-aware, fixed install folder, up-to-date message), a GPU support row with the device in use, Add to PATH, and separate Model runtime / Dictionaries / Browser extension sections; the tray menu was reworked (see below); clipboard popup translation was fixed; user-dictionary articles are separate named cards; the extension overlay is fluid and opens from the toolbar button, with a detach-to-window button; the installer installs WebView2 when missing and links the C runtime statically.
- Agent handoff guidance in `AGENTS.md` now records the repository shape, authoritative commands, runtime constraints, and version-update requirements.

## Currently Works

- Standalone mode (default; Settings window): press Download llama.cpp (GPU-appropriate build fetched automatically, with progress), choose a models folder, pick a `.gguf`, and translate without LM Studio. An existing llama.cpp folder can still be chosen under Advanced. The download flow has unit tests for asset selection and was confirmed on an NVIDIA machine (CUDA build, fixed folder). `llama-server` template flags depend on the model family (`adapters::Family`, derived from the model file name): `gemma` → `--no-jinja --chat-template gemma` (TranslateGemma's embedded Jinja template rejects plain-text messages and some conversions fall back to ChatML); `hunyuan` → `--jinja`; anything else (Qwen etc.) → `--jinja --chat-template-kwargs {"enable_thinking":false}`. Verified manually on Windows with llama.cpp build b11243 (CUDA 13.4, RTX 3060) using TranslateGemma 4B Q8_0 and 12B Q4_K_S.
- Blocking native commands run off the UI thread and parsed StarDict folders are cached, so lookups no longer freeze the window; waits show a spinner. Dictionary, runtime, and extension-token settings live in the Settings window.

- `npm run dev`, `npm run desktop:dev`, desktop frontend builds, native Tauri/NSIS builds, and extension build/package commands are established in the root `package.json`.
- LM Studio model discovery uses loaded instances from `/api/v1/models`, with a cache-busted `/v1/models` fallback for older versions. Startup restores and validates the persisted model for desktop and extension requests.
- History supports paging, search highlighting, favorites, CSV export, clear, schema migration, and retention of 1,000 non-favorite rows.
- Extension: the toolbar button and the context menu open the same movable, resizable in-page overlay (right/bottom/corner handles, size remembered); the detach button (⧉) moves the content to a separate browser window that survives tab switches; restricted pages fall back to that window. Clipboard copy in the overlay no longer logs permission-policy errors.
- Status pill: shows `Standalone · ready · <model file>` before the first translation and `Standalone · running <model>` after it. `llama-server` is started lazily on the first translation (not at app start); a "load model at startup" option was considered and declined.
- The desktop executable runs without a console window. Closing hides it to the tray; relaunching a pinned shortcut focuses the existing process.
- Tray menu: Open LingvoLoc (Ctrl+Shift+L), Translate clipboard (Ctrl+Shift+T), Settings… (opens the settings window), Start with Windows (checkbox; per-user `HKCU\...\Run` value launching the app with `--tray` so the main window stays hidden), Quit. Left-click toggles the main window. The autostart toggle has not been exercised end to end.
- Extension API tests and desktop TypeScript/Rust tests are part of `npm run check`.

## In Progress

- No implementation is in progress; the worktree is clean at release `v2.1.1` (plus the handoff documentation commit).
- The next product area is dictionary quality. The current generic StarDict parser renders cleaned record content but assigns `language: "und"` and `part_of_speech: "User dictionary"`; it now maps the source language from `bookname`/file name (falls back to `und`), but does not yet reliably split rich `rus-eng` records into structured senses and fields.

## Known Issues And Blockers

- The Hunyuan-MT and generic chat (Qwen) adapters were verified only through a standalone benchmark script that reproduced the adapter prompts and `llama-server` flags, plus unit tests; they were not exercised through the desktop UI. The Gemma family (Gemma 3 QAT, TranslateGemma 4B/12B) was used in the UI and works.
- Abliterated Gemma fine-tunes drop trailing paragraphs of long text (reproduced with `gemma-3-12b-it-qat-abliterated` q4_k_m/q6_k on a five-paragraph article). Regular Gemma 3 QAT and TranslateGemma 12B translated it fully. There is no per-paragraph chunking, so a weak model can still omit text; the UI does not warn about it.
- Tokens/s is completion tokens divided by the whole request time, so the first request after a model switch is slower (it includes model loading). The browser extension popup still shows latency only.
- The adapter is chosen from the model file name; a renamed file (for example without `hunyuan`/`gemma` in its name) gets the generic chat adapter. The `adapterId` setting is informational and no longer validated against the request.
- Not yet verified on real hardware or a clean machine: the Download llama.cpp flow on AMD/Intel/no-GPU machines (on the maintainer's NVIDIA machine it produced the fixed `llama.cpp/` folder with `version.txt` = `b11247 CUDA`), the Start with Windows toggle, the Add to PATH button in a fresh profile, the installer on a machine without WebView2, and the `+crt-static` build on other machines. Layout changes (header, dictionary cards, extension overlay) were checked by the user on one machine only.
- The integrated-GPU name list in `runtimes/llama_server.rs` (`is_integrated`) is heuristic; an unusual integrated GPU could be preferred over a discrete Vulkan card.
- The `Start with Windows` registry value stores the executable path; after moving the install folder the checkbox must be toggled off and on. The installer has no uninstall hook that removes the value.
- The extension archive for a version is tracked in git; release ZIPs for released versions must not be repackaged (`npm run extension:package` overwrites `LingvoLoc-extension-<version>.zip`).

- In LM Studio mode, LM Studio must be running at `http://127.0.0.1:1234/v1` with a compatible model loaded; in Standalone mode llama.cpp must be downloaded (or an existing folder chosen) and a models folder selected. Translation and extension requests otherwise fail.
- Native builds require Rust, WebView2, and Windows C++ build tools/Windows SDK. Visual Studio provides the compiler tools on this machine, but they may not be on the general `PATH`.
- Dictionary coverage and quality depend entirely on dictionaries selected by the user. Missing records are intentionally not replaced with model-generated dictionary facts.
- StarDict language metadata is not mapped, and complex dictionary markup/senses are only cleaned and displayed rather than semantically parsed. This is the main unfinished product work.
- Clipboard copying depends on WebView2 clipboard permission/context; translation still succeeds and reports a notice when copying is unavailable.
- The loopback API uses a per-process token, so the extension must be paired again after the desktop process restarts.
- A formal trademark and domain review for `LingvoLoc` remains required before a public release.
- Locally rebuilt installers do not update an installed copy automatically; install the new NSIS artifact manually.

## Next Recommended Step

- First do the manual verifications listed in `TODO.md` if hardware is available. Then, for product work: add language metadata mapping and representative `rus-eng` fixture tests around `user_stardict_entries` and `parse_stardict_index_with_audio` in `apps/desktop/src-tauri/src/services/lexical.rs`. Then improve record parsing only as needed to make those fixtures produce stable definitions, translations, examples, and sense separation. Run focused Rust tests first, followed by `npm run check`.

## Inspect First

- `apps/desktop/src-tauri/src/runtimes/llama_download.rs` and `llama_server.rs`: llama.cpp download/selection, device pinning, PATH editing, server lifecycle.
- `apps/desktop/src-tauri/src/lib.rs`: tray menu, autostart wiring, native commands.
- `apps/extension/src/background.ts`, `popup.ts`, `popup.css`: overlay injection, resize handles, detach window, fluid layout.

- `AGENTS.md`: repository commands, constraints, and version coordination.
- `TODO.md`: remaining actionable work only.
- `DECISIONS.md`: architectural constraints, especially **Lexical Provider Aggregation**, **WordNet Dictionary Fallback**, and **User Dictionary Folder**.
- `apps/desktop/src-tauri/src/services/lexical.rs`: current StarDict scanning, parsing, aggregation, media loading, and tests.
- `apps/desktop/src/app/App.tsx`: dictionary folder selection, enabled dictionaries, lookup state, sanitization, and rendering.
- `scripts/build-lexical-index.mjs`: offline lexical conversion path; do not conflate it with runtime user-dictionary lookup.
- `apps/desktop/src-tauri/src/api.rs` and `apps/extension/src/`: loopback API and extension boundary if extension work resumes.
