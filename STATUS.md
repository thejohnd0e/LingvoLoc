# Status

## Current Release

- Version `1.45.2` is the current release across the root, desktop, extension, Tauri, Cargo, and extension packaging metadata.
- The last recorded release gate passed: `npm run check`, `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`, and `npm run desktop:build`.
- Release artifacts are `apps/desktop/src-tauri/target/release/bundle/nsis/LingvoLoc_1.45.2_x64-setup.exe` and `apps/extension/LingvoLoc-extension-1.45.2.zip`.

## Completed

- The Windows desktop app translates through the selected LM Studio TranslateGemma model, supports automatic detection for 12 languages, persists settings and SQLite history, copies successful results, and provides tray, single-instance, global-shortcut, and clipboard-popup workflows.
- The authenticated API on `127.0.0.1:47831` supports the thin Chromium MV3 extension. Pairing, independent language directions, toolbar translation, and a movable/resizable in-page context-menu popup have been implemented and smoke-tested.
- Dictionary lookup uses only user-selected StarDict folders. Dictionary selection is persisted; `.dict.dz`, common audio/image references, and `res.zip` media are supported. Bundled and app-data dictionary fallbacks are intentionally not used.
- Translation word selection supports model-assisted forward and reverse highlighting without adding word requests to translation history.
- The desktop and extension have responsive layouts, visible focus states, persisted text-size controls, and current LingvoLoc branding.
- Agent handoff guidance in `AGENTS.md` now records the repository shape, authoritative commands, runtime constraints, and version-update requirements.

## Currently Works

- `npm run dev`, `npm run desktop:dev`, desktop frontend builds, native Tauri/NSIS builds, and extension build/package commands are established in the root `package.json`.
- LM Studio model discovery uses loaded instances from `/api/v1/models`, with a cache-busted `/v1/models` fallback for older versions. Startup restores and validates the persisted model for desktop and extension requests.
- History supports paging, search highlighting, favorites, CSV export, clear, schema migration, and retention of 1,000 non-favorite rows.
- The desktop executable runs without a console window. Closing hides it to the tray; relaunching a pinned shortcut focuses the existing process.
- Extension API tests and desktop TypeScript/Rust tests are part of `npm run check`.

## In Progress

- No product implementation is currently in progress in the worktree.
- The next product area is dictionary quality. The current generic StarDict parser renders cleaned record content but assigns `language: "und"` and `part_of_speech: "User dictionary"`; it now maps the source language from `bookname`/file name (falls back to `und`), but does not yet reliably split rich `rus-eng` records into structured senses and fields.

## Known Issues And Blockers

- LM Studio must be running at `http://127.0.0.1:1234/v1` with a compatible model loaded; translation and extension requests otherwise fail.
- Native builds require Rust, WebView2, and Windows C++ build tools/Windows SDK. Visual Studio provides the compiler tools on this machine, but they may not be on the general `PATH`.
- Dictionary coverage and quality depend entirely on dictionaries selected by the user. Missing records are intentionally not replaced with model-generated dictionary facts.
- StarDict language metadata is not mapped, and complex dictionary markup/senses are only cleaned and displayed rather than semantically parsed. This is the main unfinished product work.
- Clipboard copying depends on WebView2 clipboard permission/context; translation still succeeds and reports a notice when copying is unavailable.
- The loopback API uses a per-process token, so the extension must be paired again after the desktop process restarts.
- A formal trademark and domain review for `LingvoLoc` remains required before a public release.
- Locally rebuilt installers do not update an installed copy automatically; install the new NSIS artifact manually.

## Next Recommended Step

- Add language metadata mapping and representative `rus-eng` fixture tests around `user_stardict_entries` and `parse_stardict_index_with_audio` in `apps/desktop/src-tauri/src/services/lexical.rs`. Then improve record parsing only as needed to make those fixtures produce stable definitions, translations, examples, and sense separation. Run focused Rust tests first, followed by `npm run check`.

## Inspect First

- `AGENTS.md`: repository commands, constraints, and version coordination.
- `TODO.md`: remaining actionable work only.
- `DECISIONS.md`: architectural constraints, especially **Lexical Provider Aggregation**, **WordNet Dictionary Fallback**, and **User Dictionary Folder**.
- `apps/desktop/src-tauri/src/services/lexical.rs`: current StarDict scanning, parsing, aggregation, media loading, and tests.
- `apps/desktop/src/app/App.tsx`: dictionary folder selection, enabled dictionaries, lookup state, sanitization, and rendering.
- `scripts/build-lexical-index.mjs`: offline lexical conversion path; do not conflate it with runtime user-dictionary lookup.
- `apps/desktop/src-tauri/src/api.rs` and `apps/extension/src/`: loopback API and extension boundary if extension work resumes.
