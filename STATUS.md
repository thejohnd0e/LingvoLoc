# Status

## Current Release

- Version `2.1.1` is the current release across the root, desktop, extension, Tauri, Cargo, and extension packaging metadata.
- `v2.1.1` is published on GitHub Releases (installer and extension ZIP attached). The last recorded release gate passed: `npm run check`, `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`, and `npm run desktop:build`.
- Release artifacts are `apps/desktop/src-tauri/target/release/bundle/nsis/LingvoLoc_2.1.1_x64-setup.exe` and `apps/extension/LingvoLoc-extension-2.1.1.zip`.

## Current Incident: Document Translation Still Hangs

- The previous runtime/control fix did not solve the user's real-world EPUB hang. Do not describe document translation as fixed.
- Reproduction remains `D:\MyProjects\LoTran\example_books\Bykov_Putin-i-muzhik.265267.fb2.epub`, translated with `translategemma-12b-it.Q5_K_M.gguf` through Standalone llama.cpp.
- The user's latest screenshot shows the active job at `56 / 80 blocks translated`, `Status: translating`, with the UI still displaying `Translating...` and no completion or error.
- Fresh worker evidence identifies the actual boundary: job `txt-38624-1790741713343` saved block 55, entered block 56, logged `coordinator_acquired`, then returned `request_end ... elapsed_ms=120009 result=error`. This proves the worker is not waiting for the inference coordinator; it is blocked inside the model HTTP request until the 120-second client timeout.
- The paired `llama-server` log has normal completed requests before this point but no completion record for the block-56 request. The server-side behavior for that request is still unknown.
- The first screenshot is a separate workflow issue: the displayed source is already named `Bykov_Putin-i-muzhik.265267.fb2.translated.en.epub`, with `en → en`, `0 / 80`, and repeated EPUB formatting-boundary diagnostics. This is an output file being analyzed/restarted as a new job, not evidence that the original source language direction is correct.
- `Connection: close`, interrupt/restart handling, two-phase shutdown, clear-job persistence, and polling race protection are already implemented and covered by tests, but none prevents a normal active HTTP request from consuming the full timeout.
- 2026-09-30 investigation: a direct Python client (240 sequential requests) and the app's Rust client (300 requests) against a fresh `llama-server` with the same flags did NOT reproduce the stall, and block 56's text translates fine in other jobs (`txt-25916` finished 80/80). Earlier jobs stalled at 54-64 blocks, so the trigger depends on in-app state, not on text or the server alone. The llama-server log has no task for block 56, i.e. the server never started processing it.
- Mitigation implemented (not a proven root-cause fix): model requests now stream (`stream:true`) with a 45 s idle timeout (`runtimes/lm_studio.rs`), so a stall is detected in 45 s instead of 120 s and Standalone restarts the server and retries once. Runtime events (`http_send/headers/first_chunk/end/stall`, `server_start/stop`, `complete_error` with pid/health probe, interrupt reasons) are appended to `%TEMP%\lingvoloc-document-worker.log` as `job=runtime`. On the next hang read those lines around the stalled block to see whether the server was alive and answered `/health`.
- 2026-09-30 second stall (job `txt-55600-1790747942527`, block 119, 786 chars): the log shows `coordinator_acquired` but NO `http_send`, so the block hung before any HTTP request left the app. Added step events (`translate_begin`, `standalone_begin/paths_ok/server_ok`, `http_thread_spawn`), a `request_slow` watchdog line every 20 s with the last runtime event, and stopped creating an unused blocking reqwest client per completion. The user later reported no more stalls; the cause is still unproven. On the next hang read the `request_slow ... last_event=` lines.
- UI: the header has a Text | Files mode switch (Ctrl+1/Ctrl+2, remembered, progress badge on the Files tab; both views stay mounted). The Files view has a recent-jobs list (`list_document_jobs`, counts computed in SQL), multi-file selection, drag-and-drop of files onto the window, and a queue that translates all ready jobs in order and exports each next to its source. Interactive text requests already run before the next document block (coordinator priority); an active block is never preempted.
- Guards added: same source/target language is rejected natively; the Documents picker refuses `*.translated.<lang>.<ext>`.
- A document with all blocks translated stays `translating` until Export (by design); it is not a hang.
- Old note, superseded: exact next step: reproduce from a clean job while capturing the request payload, llama-server task id, process state, and server log tail around block 56; then determine why this specific request does not complete and add a regression test at the HTTP/runtime boundary before changing the worker again.

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

- File translation Phase 0 is complete: the baseline gate passed and self-authored TXT, DOCX, EPUB, and FB2 fixtures were added under `docs/fixtures/`. Phase 1 is complete as a feasibility investigation: the ignored spike runtime uses the official PDFium Windows x64 DLL; extraction, text bounds, rendering, and controlled replacement overflow are recorded in `docs/FILE_TRANSLATION_PHASE1_REPORT.md`. Phase 2 is complete: the isolated document-job store now persists jobs and blocks, enforces transitions, hashes sources, and recovers interrupted work. Phase 3 is complete: all interactive translation entry points share a one-request inference coordinator, and document blocks have a yielding background seam. Phase 4 is complete: oversized blocks are split conservatively and the worker persists each successful segment independently. Phase 5 is complete: TXT jobs can be analyzed, translated, paused/resumed/cancelled between blocks, recovered after restart, and exported to a separate file. Phase 6 DOCX implementation and automated package/security review are complete. Phase 7 EPUB implementation and automated package/XML review are complete; a separate FB2 v1 path now uses the same recoverable job pipeline.
- TXT supports UTF-8 with or without BOM and UTF-16 with BOM. Text is normalized to LF and non-empty paragraphs become persisted blocks. DOCX supports bounded analysis and package-preserving export for supported paragraphs in `word/document.xml`; unsupported stories and constructs remain unchanged and produce diagnostics. EPUB supports bounded, package-preserving translation of spine XHTML paragraphs, headings, list items, and table cells, with diagnostics for unsupported XHTML content. FB2 supports title paragraphs, section/body paragraphs, epigraphs, and notes in source order; it preserves metadata, namespaces, links, identifiers, image references, and binary resources. Unsupported text, unresolved XML entities, and translations that may cross inline formatting boundaries remain visible through diagnostics. EPUB navigation labels and OPF bibliographic metadata are not translated. Outputs are written through a same-directory temporary file and rename; existing outputs and source replacement are rejected.
- The next product area is dictionary quality. The current generic StarDict parser renders cleaned record content but assigns `language: "und"` and `part_of_speech: "User dictionary"`; it now maps the source language from `bookname`/file name (falls back to `und`), but does not yet reliably split rich `rus-eng` records into structured senses and fields.
- Incident investigation is now the highest-priority work: reproduce and fix the normal document request timeout at block 56. The release EXE builds and unit tests pass, but that is not sufficient evidence for this runtime failure.

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
- Document translation can still remain in `translating` while a block request waits for the HTTP timeout. The latest confirmed case is job `txt-38624-1790741713343`, block 56, 120009 ms from `coordinator_acquired` to request error. The UI may therefore appear frozen even though the worker thread is still alive.
- The current diagnostic list can be misleading when an already translated output file is selected as a fresh input: the UI can show a `.translated.en.epub` filename and `en → en` with zero translated blocks. This needs either an explicit guard/warning or clearer source/output handling.
- A formal trademark and domain review for `LingvoLoc` remains required before a public release.
- Locally rebuilt installers do not update an installed copy automatically; install the new NSIS artifact manually.

## Next Recommended Step

- First reproduce the current incident from a clean job and inspect the exact block-56 request at `apps/desktop/src-tauri/src/runtimes/lm_studio.rs`, `apps/desktop/src-tauri/src/runtimes/llama_server.rs`, and `apps/desktop/src-tauri/src/services/translation.rs`. Capture server logs and process command lines at the timeout.
- Add a focused runtime test or local fake server that reproduces a response which never completes, then implement bounded cancellation/recovery that cannot leave the job looking indefinitely active.
- After the runtime fix, add a frontend/native guard against selecting LingvoLoc's own translated output as a new source, or at minimum show a warning for `.translated.<lang>.<ext>` and same-language `en → en` jobs.
- Only after the incident is fixed return to EPUB/FB2 reader validation and dictionary-quality work.

## Session Handoff

- Phase 7 EPUB implementation and automated package/security review are complete; EPUB validator/reader validation remains blocked because `epubcheck`, Calibre/`ebook-convert`, Pandoc, and a repository EPUBCheck JAR are unavailable on this host.
- Phase 6 manual viewer validation remains blocked because Word and LibreOffice are not installed on this host.
- TXT, DOCX, EPUB, and FB2 entry points are registered in `apps/desktop/src-tauri/src/lib.rs`; FB2 uses `analyze_fb2`, `start_fb2_job`, `resume_fb2_job`, and `export_fb2_job`.
- TXT parsing and export are in `apps/desktop/src-tauri/src/documents/txt.rs`; job orchestration is in `documents/commands.rs`; segmentation and persisted translation are in `documents/segmentation.rs` and `documents/worker.rs`.
- The frontend workflow is `apps/desktop/src/app/DocumentsPanel.tsx`, mounted from `App.tsx`; command types/wrappers are in `apps/desktop/src/lib/commands.ts` and styles are in `apps/desktop/src/styles.css`.
- Jobs live in app data `document-jobs.sqlite`. Startup calls `recover_interrupted`; the active job id is kept in local storage under `lingvoloc.document-job-id`.
- TXT output normalizes CRLF/CR to LF and joins non-empty paragraphs with a blank LF line. Existing output files and source replacement are rejected.
- The native worker pauses or cancels only between blocking model requests; the active HTTP request is not forcibly interrupted.
- FB2 implementation verification: eight parser/export tests plus three command tests cover source-order headings and paragraphs, epigraph and note text, metadata preservation, note/image links, embedded binary resources, unsupported tables/containers/poetry, unresolved entities, inline-format diagnostics, diagnostic bounds, and segmented export. `npm run check` passes (137 Rust, 27 desktop, and 3 extension tests); `cargo clippy --all-targets -- -D warnings` and `npm run build` pass. Independent FB2 reader/validator verification remains open.
- Not verified: opening translated EPUB/FB2 in real readers or running independent validators because no suitable local tools are installed; real model translation, Windows pause/resume/cancel, clean-machine installer behavior, and Word/LibreOffice DOCX validation also remain unverified.
- Previous release-gate checks are green (`npm run check`, Clippy, and `tauri build --no-bundle`), but the user's real-world document translation hang remains unresolved. Do not use those green gates as a runtime-fix claim.
- Latest decisive log lines are in `C:\Users\la\AppData\Local\Temp\lingvoloc-document-worker.log`: job `txt-38624-1790741713343` block 56 has `coordinator_acquired` followed 120009 ms later by `request_end ... result=error`, with no `block_saved`. `C:\Users\la\AppData\Local\Temp\lingvoloc-llama-server.log` contains no corresponding completed server task after the prior successful requests.
- The active HTTP implementation is synchronous blocking `ureq`-style `.post(...).send()`/`.json()` in `apps/desktop/src-tauri/src/runtimes/lm_studio.rs`; Standalone reuses it from `apps/desktop/src-tauri/src/runtimes/llama_server.rs`. The worker loop and coordinator are in `apps/desktop/src-tauri/src/documents/worker.rs` and `apps/desktop/src-tauri/src/services/inference_coordinator.rs`.
- The exact next action for the new agent is to reproduce block 56 with fresh logs and inspect the request/server task correlation. Do not start by changing polling or `Clear job`; those paths are not the confirmed cause of this incident.

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
