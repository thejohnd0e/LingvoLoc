# Status

## Current Release

- Version `2.1.1` is the current release across the root, desktop, extension, Tauri, Cargo, and extension packaging metadata.
- `v2.1.1` is published on GitHub Releases (installer and extension ZIP attached). The last recorded release gate passed: `npm run check`, `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`, and `npm run desktop:build`.
- Release artifacts are `apps/desktop/src-tauri/target/release/bundle/nsis/LingvoLoc_2.1.1_x64-setup.exe` and `apps/extension/LingvoLoc-extension-2.1.1.zip`.

## Document Translation Stalls (Closed By The User, Root Cause Never Isolated)

- Symptom: a document job stayed `translating` while one block request never finished. Seen at blocks 54-64 of several 80-block jobs (`txt-38624-1790741713343` at block 56: `coordinator_acquired`, then `request_end ... elapsed_ms=120009 result=error`) and at block 119 of a 561-block EPUB (`txt-55600-1790747942527`: `coordinator_acquired` but no `http_send`, i.e. it stopped before any HTTP request left the app).
- Not reproducible outside the app: 240 sequential requests from a Python client and 300 from the app's Rust client against a fresh `llama-server` (same flags) never stalled, and the same block text translates fine in other jobs. The trigger therefore depends on in-app or host state (the user runs the app inside an RDP session). The llama-server log had no task for the stalled block.
- Mitigations shipped in commit `7a81976` (NOT a proven root-cause fix): streaming requests with a 45 s idle timeout and one restart+retry in Standalone (`runtimes/lm_studio.rs`, `runtimes/llama_server.rs`); no per-completion blocking reqwest client any more; runtime step events and a `request_slow` watchdog (every 20 s, with `last_event=`) written to `%TEMP%\lingvoloc-document-worker.log` (`trace.rs`).
- Status: closed on 2026-09-30. After the last rebuild the user translated several real books (including the 561-block EPUB and multi-file queues) with no stalls and considers the problem solved. The exact cause was never isolated; the mitigations below are the fix in practice.
- Only if it ever recurs, reopen it: read the `job=runtime` and `request_slow ... last_event=` lines around the stalled block in the worker log. `last_event=http_thread_spawn` or `translate_begin` means the stall is before the HTTP request (thread/runtime/proxy/OS level); `http_send` without `http_headers` means the server accepted but never answered (check `complete_error ... health=`).
- By design a job with every block translated stays `translating` until Export; this is not a hang (the UI now says "translated, ready to export" and hides Pause/Cancel).

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
- Files mode (header switch Text | Files, Ctrl+1/Ctrl+2): pick or drop several TXT/DOCX/EPUB/FB2 files, see recent jobs, translate one or all ready jobs in a row (each exported next to its source), pause/resume/cancel, and read a clear completion line ("Translated N / N blocks in <time>").
- Extension API tests and desktop TypeScript/Rust tests are part of `npm run check`.

## In Progress

- File translation Phase 0 is complete: the baseline gate passed and self-authored TXT, DOCX, EPUB, and FB2 fixtures were added under `docs/fixtures/`. Phase 1 is complete as a feasibility investigation: the ignored spike runtime uses the official PDFium Windows x64 DLL; extraction, text bounds, rendering, and controlled replacement overflow are recorded in `docs/FILE_TRANSLATION_PHASE1_REPORT.md`. Phase 2 is complete: the isolated document-job store now persists jobs and blocks, enforces transitions, hashes sources, and recovers interrupted work. Phase 3 is complete: all interactive translation entry points share a one-request inference coordinator, and document blocks have a yielding background seam. Phase 4 is complete: oversized blocks are split conservatively and the worker persists each successful segment independently. Phase 5 is complete: TXT jobs can be analyzed, translated, paused/resumed/cancelled between blocks, recovered after restart, and exported to a separate file. Phase 6 DOCX implementation and automated package/security review are complete. Phase 7 EPUB implementation and automated package/XML review are complete; a separate FB2 v1 path now uses the same recoverable job pipeline.
- TXT supports UTF-8 with or without BOM and UTF-16 with BOM. Text is normalized to LF and non-empty paragraphs become persisted blocks. DOCX supports bounded analysis and package-preserving export for supported paragraphs in `word/document.xml`; unsupported stories and constructs remain unchanged and produce diagnostics. EPUB supports bounded, package-preserving translation of spine XHTML paragraphs, headings, list items, and table cells, with diagnostics for unsupported XHTML content. FB2 supports title paragraphs, section/body paragraphs, epigraphs, and notes in source order; it preserves metadata, namespaces, links, identifiers, image references, and binary resources. Unsupported text, unresolved XML entities, and translations that may cross inline formatting boundaries remain visible through diagnostics. EPUB navigation labels and OPF bibliographic metadata are not translated. Outputs are written through a same-directory temporary file and rename; existing outputs and source replacement are rejected.
- Dictionary lookup is considered good enough by the user (2026-09-30): user StarDict folders show cleaned articles per dictionary with transcription, examples and audio. There is no planned dictionary work; see the optional cosmetic notes in Known Issues.

- The Text | Files UI (mode switch, recent-jobs list, multi-file selection, drag-and-drop, sequential queue with automatic export next to the source, lightweight `get_document_progress` polling) is implemented and was confirmed by the user on real books. Its automated coverage is unit-level only; the drag-and-drop event wiring has no automated test.

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
- Dictionary articles are cleaned and shown as the dictionary wrote them, not parsed into structured senses (`language` is `und` unless the dictionary name reveals it). Cosmetic leftovers seen in the UI: a stray markup fragment such as `<l >`, the headword repeated in the first line, and audio players showing `0:00 / 0:00` before loading. Fix only if the user asks.
- Clipboard copying depends on WebView2 clipboard permission/context; translation still succeeds and reports a notice when copying is unavailable.
- The loopback API uses a per-process token, so the extension must be paired again after the desktop process restarts.
- The queue exports each result to `<name>.translated.<lang>.<ext>` next to its source; if that file already exists the export fails and the queue stops at that job (existing outputs are never overwritten).
- FB2/EPUB/DOCX "may cross inline formatting boundaries" is one summary line in the UI (a lower bound when the native list of 100 diagnostics is truncated); the job still finishes as `completed_with_warnings`.
- Interactive text requests wait for the current document block to finish (no mid-block preemption); very large blocks can delay them by several seconds.
- A formal trademark and domain review for `LingvoLoc` remains required before a public release.
- Locally rebuilt installers do not update an installed copy automatically; install the new NSIS artifact manually.

## Next Recommended Step

- The stall incident is closed (see its section for what to read if it ever returns).
- Then return to the open validation items: EPUB/FB2/DOCX in real readers (blocked on missing tools), Windows pause/resume/cancel with a real model, and clean-machine installer checks.
- Dictionary quality is not a planned area; pick the next item from `TODO.md` or ask the user.
- Optional small follow-ups: overwrite/unique-name handling for queue exports, show tokens/s in the extension, Windows job object for `llama-server`.

## Session Handoff

- Phase 7 EPUB implementation and automated package/security review are complete; EPUB validator/reader validation remains blocked because `epubcheck`, Calibre/`ebook-convert`, Pandoc, and a repository EPUBCheck JAR are unavailable on this host.
- Phase 6 manual viewer validation remains blocked because Word and LibreOffice are not installed on this host.
- TXT, DOCX, EPUB, and FB2 entry points are registered in `apps/desktop/src-tauri/src/lib.rs`; FB2 uses `analyze_fb2`, `start_fb2_job`, `resume_fb2_job`, and `export_fb2_job`.
- TXT parsing and export are in `apps/desktop/src-tauri/src/documents/txt.rs`; job orchestration is in `documents/commands.rs`; segmentation and persisted translation are in `documents/segmentation.rs` and `documents/worker.rs`.
- The frontend workflow is `apps/desktop/src/app/DocumentsPanel.tsx`, mounted from `App.tsx`; command types/wrappers are in `apps/desktop/src/lib/commands.ts` and styles are in `apps/desktop/src/styles.css`.
- Jobs live in app data `document-jobs.sqlite`. Startup calls `recover_interrupted`; the active job id is kept in local storage under `lingvoloc.document-job-id`.
- TXT output normalizes CRLF/CR to LF and joins non-empty paragraphs with a blank LF line. Existing output files and source replacement are rejected.
- FB2 implementation verification: eight parser/export tests plus three command tests cover source-order headings and paragraphs, epigraph and note text, metadata preservation, note/image links, embedded binary resources, unsupported tables/containers/poetry, unresolved entities, inline-format diagnostics, diagnostic bounds, and segmented export. `npm run check` passes (137 Rust, 27 desktop, and 3 extension tests); `cargo clippy --all-targets -- -D warnings` and `npm run build` pass. Independent FB2 reader/validator verification remains open.
- Not verified: opening translated EPUB/FB2 in real readers or running independent validators because no suitable local tools are installed; real model translation, Windows pause/resume/cancel, clean-machine installer behavior, and Word/LibreOffice DOCX validation also remain unverified.

- The native worker pauses or cancels only between block requests; Pause/Cancel/Clear also call `llama_server::interrupt(reason)`, which kills the server so an in-flight request fails fast and is not retried.
- Model HTTP: `LmStudioRuntime::complete` streams SSE on a dedicated thread with a tokio current-thread runtime (`tokio` features `rt`,`time`,`net`); a server that ignores `stream` and returns JSON is still accepted. `services::translation::translate` and `StandaloneRuntime::complete` write step events via `trace::runtime_event`.
- Frontend: `App.tsx` owns the mode switch (`lingvoloc.mode` in localStorage) and keeps both views mounted (`hidden`). `DocumentsPanel.tsx` owns file picking, drag-and-drop (`@tauri-apps/api/webview` `onDragDropEvent`), the recent-jobs list, the queue (`runQueue`) and progress polling; `documentProgress.ts` has ETA/duration/diagnostic grouping helpers. Native list/progress commands: `list_document_jobs`, `get_document_progress` (`documents/commands.rs`, `store.rs` `summaries`/`summary`).
- Verification state at handoff: `npm run check` (41 desktop tests, 150 Rust tests, 3 extension tests), clippy `-D warnings` and `npm run desktop:build` were green on the final code; the built installer is `apps/desktop/src-tauri/target/release/bundle/nsis/LingvoLoc_2.1.1_x64-setup.exe`. Rebuilding fails with "Access is denied" while `lingvoloc.exe` is running: quit the app first.
- Untracked and intentionally not committed: `.omo/` (agent tooling state) and `example_books/` (user's test books, copyrighted).

## Inspect First

- `apps/desktop/src-tauri/src/runtimes/lm_studio.rs`, `runtimes/llama_server.rs`, `trace.rs`, `documents/worker.rs`: the request path and stall diagnostics.
- `apps/desktop/src/app/DocumentsPanel.tsx`, `App.tsx`: Files mode, list, queue, drag-and-drop.
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
