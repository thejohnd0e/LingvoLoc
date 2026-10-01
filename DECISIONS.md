# Decisions

## Phase 6 DOCX

- **Decision:** Keep DOCX analysis bounded and in-memory. Require only `[Content_Types].xml`, `_rels/.rels`, and `word/document.xml`; preserve unsupported package parts for export and report diagnostics rather than silently dropping them.
- **Reason:** Real DOCX files may omit numbering, styles, or settings, while ZIP traversal, duplicate names, encryption, and decompression limits must be rejected before content is trusted.
- **Decision:** Use direct `quick-xml = "0.42"` with `NsReader`; use `SimpleFileOptions`/`start_file` for rewritten XML and `raw_copy_file` for untouched ZIP entries.
- **Reason:** These are the documented APIs for the pinned dependencies and preserve relationships/media without extracting user files.
- **Decision:** Persist diagnostics transactionally and finish DOCX exports as `CompletedWithWarnings` when diagnostics exist; retain TXT export behavior unchanged.
- **Reason:** Unsupported stories must be visible to users without making a partially preserved document look fully clean.
- **Decision:** Word and LibreOffice validation is a release gate, not an automated substitute. The current host lacks both viewers.

## Phase 7 EPUB

- **Decision:** Process only the EPUB spine XHTML in spine order, translating ordinary paragraphs, headings, list items, and table cells; raw-copy all other ZIP entries and preserve unsupported XHTML unchanged with diagnostics.
- **Reason:** This bounds reconstruction while retaining resources, CSS, anchors, links, identifiers, and package metadata without silently presenting unsupported content as translated.
- **Decision:** Use `epub-v1` for the parser/configuration version and the existing recoverable document-job/export pipeline, with `CompletedWithWarnings` when diagnostics are present.
- **Reason:** EPUB jobs must resume and export under the same source/runtime safeguards as TXT and DOCX, while unsupported constructs remain visible to users.
- **Decision:** Leave EPUB navigation labels and OPF bibliographic metadata unchanged in the first EPUB increment.
- **Reason:** The implemented scope rewrites supported spine XHTML only; changing navigation or metadata without dedicated block mappings would risk altering unrelated package content. This is a documented follow-up limitation.
- **Decision:** Treat an EPUB validator or reader smoke check as a release gate separate from automated tests. The current host has no `epubcheck`, Calibre/`ebook-convert`, Pandoc, or repository EPUBCheck JAR.
- **Reason:** Package/XML tests prove deterministic reconstruction and preservation, but they cannot prove that an independent EPUB consumer accepts and renders the output.

## Phase 7 FB2

- **Decision:** Add FB2 as a separate plain-XML format on the existing document-job pipeline, using parser/configuration version `fb2-v1`; do not accept archived `.fb2.zip` inputs in the first increment.
- **Reason:** This enables common `.fb2` books without coupling EPUB ZIP-package assumptions to FB2 or introducing archive extraction.
- **Decision:** Translate section-title paragraphs, body paragraphs, epigraph paragraphs, and note-body paragraphs in XML order. Leave book metadata unchanged and preserve namespaces, links, identifiers, images, and binary resources.
- **Reason:** Stable text blocks support the existing recoverable worker while leaving structural and non-text content intact.
- **Decision:** Do not resolve custom or external XML entities. Preserve the affected paragraph unchanged and report a diagnostic; report unsupported text-bearing elements the same way.
- **Reason:** This avoids fetching external DTDs or inventing entity text while allowing supported paragraphs in the rest of a book to be translated.
- **Decision:** Export translated FB2 through the existing temporary-file and no-source-replacement safeguards; use `CompletedWithWarnings` when parser diagnostics exist.
- **Reason:** FB2 must retain document-job recovery and safe export semantics already used by TXT, DOCX, and EPUB.

## Phase 8 PDF

- **Decision:** Translate text PDFs in place with `pdfium-render` (PDFium loaded at runtime from `pdfium.dll`, bundled in the installer via `scripts/fetch-pdfium.ps1`), block ids `pdf#pNNNN-KKK`, no geometry stored in the job database: export re-runs the same deterministic analysis on the hash-checked source.
- **Reason:** Blocks stay plain text like the other formats, so the existing worker, recovery and queue are reused; geometry never goes stale because the source must be unchanged.
- **Decision:** Remove the original text objects and add new embedded-font text objects (never white boxes), copying the original fill colour; keep everything else on the page.
- **Reason:** Copy/search must not mix original and translated text, and backgrounds, panels and images must survive.
- **Decision:** Code, page numbers and short all-caps labels are not translated. Code is recognised by a dark panel with coloured text or by code tokens, because real books often set code in the body font.
- **Reason:** Translating code corrupts it; the heuristic is deliberately conservative and unrecognised code is translated rather than dropped.
- **Decision:** PDFium can be bound once per process, so one instance lives behind a mutex (`pdf/engine.rs`).
- **Decision:** Region detection must not construct a flow with zero paragraphs when a page's candidate columns contain only non-body regions; emit the existing ambiguity diagnostic and skip the empty fallback instead of panicking.
- **Reason:** Full real books can contain panel/table-heavy pages that trigger column geometry before body classification. A native panic leaves the Files UI stuck in `Analyzing`, while an empty reading flow is safely represented by the page's existing non-translatable regions.
- **Decision:** PDF placement is slot-based (`pdf/slots.rs`): each translated paragraph keeps its source position and grows only into free space; chained paragraphs in one column are re-flowed together, never across obstacles.
- **Reason:** A page-wide flow stacked paragraphs over tables, panels and TOC rows (user screenshots 2026-10-01); anchoring to the source and respecting obstacles removes the overlaps without changing block ids.
- **Decision:** A reading-friendly EPUB output for PDFs is optional and comes after layout preservation (user request 2026-09-30).

## Standalone Runtime

- **Decision:** Add a `standalone` runtime mode next to LM Studio that spawns the user's `llama-server.exe` (llama.cpp) with a `.gguf` chosen from a user-selected models folder, and talks to it over the same OpenAI-compatible API on a free `127.0.0.1` port.
- **Reason:** It reuses the existing `ModelRuntime`/adapter split and needs no C++ toolchain, while letting users pick the CPU/CUDA/Vulkan build that suits their hardware.
- **Decision:** Do not bundle `llama-server.exe` in the installer; the settings screen has a **Download llama.cpp** button that fetches the newest GitHub release into the app data folder (fixed folder `llama.cpp/`, release recorded in `version.txt`), choosing CUDA 12 + cudart for NVIDIA (video card names from `Win32_VideoController` plus the `nvcuda.dll` driver), Vulkan for other GPUs (`vulkan-1.dll`), and CPU otherwise. The release is picked from `releases?per_page=20`, not `releases/latest`, because `latest` points at a binary-less tooling tag (`v0.5.0`) and the real `bNNNNN` builds are prereleases. Choosing an existing folder, Find in PATH, and Check remain under **Advanced**.
- **Reason:** Binaries differ per GPU and are large, so the installer stays small and end users never unpack archives by hand; the engine and its license still come straight from upstream.
- **Decision:** The install folder has no version in its name; an update downloads into a sibling `.new` folder, stops the running server, and swaps the folders. An up-to-date install (same release and variant in `version.txt`) is not downloaded again.
- **Reason:** The saved executable path and any user PATH entry stay valid across updates; a running `llama-server` locks its executable on Windows.
- **Decision:** When `llama-server --list-devices` reports more than one device, pass `--device` with the strongest one (CUDA/ROCm, then discrete Vulkan, then integrated by name markers). The settings screen shows the device in use.
- **Reason:** Integrated GPUs report shared system memory, so memory size alone would prefer them over a discrete card. The name-marker list for integrated GPUs is heuristic and not exhaustive.
- **Decision:** "Add to PATH" edits the user `Environment\Path` value in the registry through PowerShell (`ExpandString`, then a `WM_SETTINGCHANGE` broadcast), never the machine PATH.
- **Reason:** No administrator rights are needed and existing `%VAR%` entries are preserved.
- **Decision:** `standalone` is the default runtime mode (Rust `RuntimeMode::default`, frontend defaults). Users with saved settings keep their stored mode.
- **Reason:** New users should not need LM Studio or any other prerequisite.
- **Decision:** The NSIS installer downloads the WebView2 runtime when missing (`embedBootstrapper`) and the C runtime is linked statically (`.cargo/config.toml`, `+crt-static`).
- **Reason:** End users run only the installer; it must not depend on Node.js, Rust, C++ build tools, or the VC++ Redistributable.
- **Decision:** Standalone model ids are `.gguf` paths relative to the models folder (`mmproj` files and non-first split shards are skipped); one server process is kept and restarted when the model or executable changes, and it is killed on app exit.
- **Reason:** Ids stay stable and cannot escape the selected folder; a single process bounds memory use.

## Responsiveness

- **Decision:** Mark every command that does I/O, HTTP, or dictionary parsing as `#[tauri::command(async)]` so it runs off the main (UI) thread, and show a spinner for each wait (translation, model refresh, dictionary lookup/scan, runtime check).
- **Reason:** Synchronous Tauri commands run on the main thread; a slow lookup or a model that is still loading froze the window and Windows labelled it "Not Responding".
- **Decision:** Cache parsed StarDict folders in memory, keyed by folder path and a fingerprint of the files' names, sizes, and modification times (at most 16 folders).
- **Reason:** Every lookup used to decompress and re-parse every enabled dictionary. The cache trades memory for speed and reloads automatically when a dictionary file changes.

## Settings Windows

- **Decision:** The header has a single Settings icon. The Settings window holds three sections separated by rules: Model runtime, Dictionaries, and Browser extension (copy the pairing token). Choosing an existing llama.cpp folder locates `llama-server.exe` itself and sits under Advanced.
- **Reason:** Keeps the main screen focused on translation and stops users from selecting the wrong executable (for example the new `llama.exe` launcher).
- **Decision:** Articles from different user dictionaries are never merged (`merge_entries` also compares providers for `User dictionary` entries) and each card shows its dictionary name.
- **Reason:** Merging joined definitions from unrelated dictionaries and lost the dictionary name.

## Agent Documentation Structure

- **Decision:** Use `AGENTS.md` as the authoritative shared instruction file. Keep `CLAUDE.md` as a pointer to it.
- **Reason:** All coding agents receive the same guidance without duplicated instructions drifting apart.

## Product Name

- **Decision:** Rename the product to LingvoLoc.
- **Reason:** It communicates language plus local processing, has no exact match in the preliminary web search, and avoids the direct `LingoLoc` conflict. A final trademark and domain review is still required before public release.

## Initial Scope

- **Decision:** Implement only Phase 0 and Phase 1 from `docs/IMPLEMENTATION_PLAN.md` first.
- **Reason:** The smallest useful milestone is a verified desktop-to-LM-Studio translation flow; later features should not delay it.

## Technology Stack

- **Decision:** Use Tauri 2, React, strict TypeScript, Vite, Rust, and npm workspaces.
- **Reason:** This matches the Windows-first native integration needs while keeping the UI and native/runtime responsibilities separate.

## Translation Architecture

- **Decision:** Keep model behavior behind `TranslationModelAdapter` and runtime behavior behind `ModelRuntime`, initially implemented in Rust.
- **Reason:** The UI must remain independent of TranslateGemma and LM Studio so additional models and llama.cpp can be added later.

## TranslateGemma Phase 1 Payload

- **Decision:** Send a plain LM Studio-compatible user message with explicit source and target language names and codes in the prompt.
- **Reason:** The installed LM Studio GGUF accepts the structured payload but ignores its language metadata; the plain prompt is the working direction-control format for this runtime.

## Repository Visibility

- **Decision:** Keep the GitHub repository private and use the MIT license.
- **Reason:** MIT permits reuse and distribution with minimal conditions while the project remains private during development.

## History Retention

- **Decision:** Retain up to 1,000 non-favorite history rows and preserve favorites when pruning; version the SQLite schema with `PRAGMA user_version`.
- **Reason:** The UI can continue paging in small groups while local history grows beyond the original 100-row query cap without allowing unbounded database growth or deleting user-marked favorites.

## Automatic Translation Pair

- **Decision:** Default source language to automatic detection and use the opposite language from two independently selected pair languages as the target; languages outside the pair fall back to the first selected language.
- **Reason:** Two independent selectors are more flexible than fixed presets while retaining a fast two-language workflow.

## Startup Window Sizing

- **Decision:** Fit the native window to rendered startup content up to the available screen height, while keeping scrolling and manual resizing enabled.
- **Reason:** The initial desktop view should avoid unnecessary empty space without preventing access to longer history content or user-controlled window sizing.

## System Tray

- **Decision:** Use Tauri's built-in tray icon. Its menu is `Open LingvoLoc`, `Translate clipboard`, `Settings…`, a `Start with Windows` checkbox, and `Quit`; the separate Show/Hide items were removed because left-click already toggles the window.
- **Reason:** This adds a native Windows background entry point without introducing another plugin or changing the main translation flow.
- **Decision:** Intercept the main window's close request and hide the window; only the tray `Quit` action exits the process.
- **Reason:** Closing the window should behave as minimize-to-tray so the local translation service remains immediately available.
- **Decision:** Keep the normal taskbar button while the main window is visible; use the tray icon as the background-app entry point after hiding it.
- **Reason:** The visible application should remain pinnable and quickly accessible from the taskbar, while the hidden application should leave only its tray icon.
- **Decision:** Build the Windows release executable with the GUI subsystem rather than the console subsystem.
- **Reason:** A desktop Tauri application must not leave a blank console window or taskbar entry alongside its native window and tray icon.
- **Decision:** Left-clicking the tray icon toggles the main window; the menu opens only on right-click.
- **Reason:** Matches common Windows tray behavior and keeps quick show/hide one click away.
- **Decision:** Use Tauri's single-instance plugin to route launches from a pinned shortcut to the existing hidden process.
- **Reason:** Hiding on close keeps the application available in the tray, so a pinned shortcut must reactivate that process instead of launching another copy.

## Global Shortcuts

- **Decision:** Register `Ctrl+Shift+L` to show LingvoLoc and `Ctrl+Shift+T` to translate the current clipboard text.
- **Reason:** These shortcuts provide a fast Windows workflow while keeping the translation UI and clipboard permissions in the existing frontend.
- **Decision:** Expose the same clipboard translation as a `Translate clipboard` tray menu item that shares the shortcut's native code path.
- **Reason:** Users without the hotkey in mind can trigger the popup from the tray, with identical behavior.

## Clipboard Popup

- **Decision:** Use a separate always-on-top Tauri window for clipboard translation instead of replacing the main workspace.
- **Reason:** Clipboard translation should be fast and non-disruptive, while the full editor remains available through the taskbar or tray.
- **Decision:** Capture clipboard text natively on the global hotkey and expose it as a one-shot IPC request for the popup.
- **Reason:** A hidden WebView can miss an event emitted before its listener is ready; native pending state makes the first popup request deterministic.

## Lexical MVP Dataset

- **Decision:** Start Lexical MVP with a small project-authored English seed lexicon and document its provenance.
- **Reason:** The lexical UI and lookup contract can be implemented and verified without importing an external dataset before its redistribution license is confirmed.
- **Decision:** Use WordNet-backed fallback data for broad English synonym coverage and keep Princeton's license notice in the repository.
- **Reason:** WordNet provides a legally documented, offline English lexical base without requiring a multi-gigabyte Wiktionary-derived download.
- **Decision:** Use the existing TranslateGemma translation adapter for WordNet fallback translations and mark them `MODEL TRANSLATION`.
- **Reason:** TranslateGemma is reliable for translation but not guaranteed to follow arbitrary JSON-generation prompts; the lexical UI must not depend on unsupported structured output.
- **Decision:** Use double-click word selection as the bridge between translation text and dictionary lookup; source selection highlights its first known target translation.
- **Reason:** This keeps lookup explicit and fast without attempting unreliable automatic word alignment for full sentences.
- **Decision:** Use a history-free model translation request to align a selected source word with the current target result when the local lexicon has no entry.
- **Reason:** Highlighting should describe the active translation, not be blocked by the coverage of the optional local dictionary.
- **Decision:** Reverse-align a selected target word by translating it back and selecting the matching source token in the editable source textarea.
- **Reason:** The source field is editable, so native selection is preferable to a separate visual overlay and preserves normal text editing behavior.

## Broader Russian Lexical Dataset

- **Decision:** Use the FreeDict+WikDict `eng-rus` and `rus-eng` StarDict releases as the first bundled bilingual dictionary provider.
- **Reason:** The releases provide roughly 100,000 bilingual headwords in a compact, ready-to-parse format and document their CC BY-SA 3.0 provenance and attribution requirements.
- **Decision:** Keep the generated FreeDict index separate from the project-authored seed and WordNet fallback.
- **Reason:** Provider labels and source notices must remain visible in the implementation so sourced translations are not confused with curated facts or WordNet relations.
- **Decision:** Keep OpenCorpora-derived morphology and Kaikki/Wiktionary raw data as follow-up providers rather than bundling them in the first index.
- **Reason:** They require additional conversion and substantially larger source snapshots, while FreeDict already provides immediate bilingual coverage.
- **Decision:** Do not bundle RuWordNet until its XML distribution terms are confirmed in writing.
- **Reason:** RuWordNet is described as CC BY-SA 4.0 but its published acquisition path is non-commercial and requires requesting the XML files; that is not a sufficient basis for an unrestricted LingvoLoc release.

- **Decision:** `Start with Windows` writes the per-user `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value through `reg.exe` and launches the app with `--tray`, which hides the main window on startup. `Settings…` shows the window and emits an `open-settings` event the frontend listens for.
- **Reason:** No administrator rights or extra crates; the app starts silently in the tray. The registry value stores the executable path, so it must be toggled off and on after moving the install folder.

## Clipboard Popup Settings

- **Decision:** The clipboard popup reads settings from local storage for every request and falls back to the native model id when the stored one is empty; failures show the real error text.
- **Reason:** The popup window is created at app start, before the main window restores or picks a model, so a snapshot taken at mount had an empty model and every request failed.

## Local Extension API

- **Decision:** Start the extension integration with a loopback HTTP API bound to `127.0.0.1:47831`, authenticated by a per-process bearer token and restricted CORS origins.
- **Reason:** The browser extension must remain a thin client while arbitrary websites must not be able to invoke privileged local translation actions.
- **Decision:** The extension has no `default_popup`. The toolbar button and the context menu both inject the same draggable, resizable in-page overlay (an iframe of `popup.html`); pages where injection fails (`chrome://`, store, PDF viewer) get a `chrome.windows.create` popup window instead. The framed page is fluid, has right/bottom/corner resize handles, and the size is stored in `chrome.storage.local`.
- **Reason:** A browser action popup cannot be moved or resized. The overlay's iframe gets `allow="clipboard-write"` but copy uses a selection-based fallback there because the host page's permissions policy blocks the Clipboard API.
- **Decision:** Keeping the translator visible across tabs is done by a detach button that opens `popup.html?window=1` in a separate browser window and hands over text, languages, and result through `chrome.storage.local` (`detachState`).
- **Reason:** A per-tab overlay cannot follow tab switches without the broad "all sites" host permission; a real window needs no new permissions. It cannot be forced always-on-top.

## Lexical Provider Aggregation

- **Decision:** Preserve examples and provider labels in generated lexical records and aggregate matching seed and FreeDict facts into one local entry.
- **Reason:** Dictionary facts from multiple licensed/local providers should be visible without silently replacing one source with another or inventing missing definitions.
- **Decision:** Merge provider facts only when language, lemma, and part of speech match.
- **Reason:** Different parts of speech represent different lexical senses and must remain separate cards even when they share a spelling.
- **Decision:** Key generated lexical records by language and lemma, and require explicit source-language options for Kaikki and morphology inputs.
- **Reason:** The converter can safely grow to additional licensed language datasets without merging homographs across languages or guessing a dataset's source language.
- **Decision:** Do not preload dictionary records; require all dictionaries to be user-selected StarDict folders.
- **Reason:** Users need explicit control over dictionary sources and licensing; bundled providers caused duplicate and unreadable lookup cards.
- **Decision:** Prioritize `rus-eng` parsing and regression coverage over additional language-pair downloads.
- **Reason:** Russian-English is the primary product workflow, and improving the quality of its existing 42,283 records has higher value than expanding the number of partially parsed providers.

## Model Adapter Selection

- **Decision:** Choose the translation adapter and the `llama-server` chat-template flags from the model file/id (`adapters::Family::from_model_id`): `gemma` → TranslateGemma prompt, `hunyuan` → Hunyuan-MT prompt, otherwise a generic system+user translator prompt (Qwen3 gets `/no_think` and `enable_thinking:false`). The stored `adapterId` setting is informational; the result reports the adapter actually used.
- **Reason:** The UI must not depend on one model family, and each family needs a different prompt and template handling; deriving it from the model name avoids a new setting and a mismatch between adapter and model.
- **Decision:** Recommend Gemma 3 12B QAT Q4_0 and TranslateGemma 12B (Q5_K_M as the quality/speed balance) for Russian on an RTX 3060 12 GB; do not recommend abliterated fine-tunes, Hunyuan-MT-7B or Qwen3-14B for Russian. Measurements are in `README.md` ("Choosing a model").
- **Reason:** Benchmarked on 12 texts including a five-paragraph article; abliterated models dropped text, Qwen3 produced invented idioms and stray characters, Hunyuan was wordy and merged paragraphs.

## WordNet Dictionary Fallback

- **Decision:** Do not show partial WordNet or model-generated records in the default dictionary lookup.
- **Reason:** Synonyms without a sourced definition, translation, and sense context are misleading when rendered as a normal dictionary card; missing coverage is safer to report explicitly.

## Document Jobs

- **Decision:** Store document jobs and translated blocks in a separate `document-jobs.sqlite`; persist SHA-256 source hashes, parser/configuration versions, and a runtime snapshot. Save each block with an idempotent transaction.
- **Reason:** File translation must not add block rows to ordinary translation history, must detect changed sources reliably, and must resume without duplicating committed work.
- **Decision:** Mark active jobs `interrupted` during recovery and require an explicit resume with matching runtime and configuration snapshots.
- **Reason:** Startup must not consume GPU resources automatically or silently mix results produced by different models/configurations.

## Inference Coordination

- **Decision:** Route desktop, API, clipboard/word, and future document-block requests through one in-process coordinator. Permit one blocking model request at a time, give waiting interactive requests priority, and require document callers to acquire the slot separately for each block.
- **Reason:** The current runtimes use blocking HTTP and Standalone owns one shared server; yielding between blocks keeps interactive translation responsive without forcibly cancelling an active request.
- **Decision:** Runtime lifecycle changes and application exit quiesce the coordinator before replacing or stopping the shared llama-server; lifecycle operations do not permanently disable normal inference unless the application is shutting down.
- **Reason:** Updating llama.cpp or changing settings must not stop a server underneath an active request or race a queued document block.

## Document Hang Incident

- **Decision:** Treat the block-56 document hang as an unresolved runtime/HTTP problem, not as a frontend polling or coordinator problem, until a fresh reproduction proves otherwise.
- **Evidence:** The worker log shows `coordinator_acquired` for block 56 and then a request error exactly 120009 ms later; preceding blocks complete normally. The current `Connection: close` header and server interrupt/restart behavior did not prevent this case.
- **Consequence:** The next fix must instrument and bound the model request at the runtime boundary, correlate it with the llama-server task/log, and add a regression test before changing UI state handling. Green unit/build gates alone do not close this incident.

## Streaming Requests With Idle Timeout

- **Decision:** Chat completions use `stream: true`, read on a dedicated thread with a small tokio runtime; each wait (headers, every chunk) has a 45 s idle deadline and the whole request a 900 s cap. Servers that ignore `stream` and return JSON are still accepted.
- **Reason:** The blocking reqwest client has no per-read timeout, so a server that accepts a request and goes silent held a document block for the full 120 s. Idle detection bounds a stall to 45 s and lets Standalone restart and retry.
- **Decision:** Runtime/HTTP/server-lifecycle events are logged to the same file as the document worker.
- **Reason:** The block-56 stall could not be reproduced outside the app; the next occurrence must show whether the server was alive and answering `/health`.

## Files Mode, Job List And Queue

- **Decision:** One window with a Text | Files switch (persisted, Ctrl+1/2); both views stay mounted and only `hidden` toggles, and the Files tab shows a progress badge.
- **Reason:** Model, runtime and language pair are shared, and a running job must keep polling while the user translates text.
- **Decision:** The Files view lists recent jobs from a lightweight SQL summary and polls progress with `get_document_progress`; `get_document_job` (all blocks) is used only when opening a job or after an action.
- **Reason:** Polling every block of a 4000-block book every 750 ms was wasteful.
- **Decision:** The queue runs ready jobs one at a time in creation order and exports each next to its source without a dialog; pause, cancel, failure or an existing output stops it. Files are analyzed while nothing else runs (drops during a run are refused).
- **Reason:** One coordinator slot and one SQLite writer per job; never overwriting outputs is an existing safety rule.
- **Decision:** Same source and target language is rejected natively; a `.translated.<lang>` file name only produces a note, never a refusal; the Documents panel sends `auto` as the target when the source is `auto`.
- **Reason:** A file name says nothing about content, and a stale stored target caused `en -> en` jobs.

## Export Overwrite And READMEs

- **Decision:** An output path picked in the save dialog may replace an existing file (the dialog already asked for confirmation); automatic outputs (queue, default names) never overwrite. Writes still go through a same-directory temporary file and rename, and the source file can never be the target.
- **Reason:** The old check refused confirmed overwrites and the failure was easy to miss in the status line, so users believed the old file had been replaced.
- **Decision:** For a PDF, "Start translation" re-analyzes the file first when the page-range field differs from the job's range, then waits for a second press.
- **Reason:** The range is only read when a file is analyzed; a changed field silently translated the whole book.
- **Decision:** `README.md` (English) and `README.ru.md` (Russian) are maintained side by side: user guide first, technical description at the end.
- **Reason:** End users need a plain description at the top; contributors need the architecture in one predictable place.

## Repository Name, Release Version And Dictionary Colours

- **Decision:** The GitHub repository is named `LingvoLoc` like the product; `LingoLoc` was the name we deliberately avoided. Internal identifiers (`com.lingoloc.desktop`, `lingoloc.sqlite`, `lingoloc.settings`) keep the old spelling.
- **Reason:** A rename of the repository only needs a redirect, but changing the bundle identifier or storage names would orphan users' settings, history, and jobs.
- **Decision:** The extension ZIP name comes from `apps/extension/package.json` in `scripts/package-extension.ps1`; the README uses only static badges.
- **Reason:** A hard-coded name overwrote the published 2.1.1 archive once; dynamic badges do not render for a private repository and a version badge would go stale.
- **Decision:** Dictionary colours are applied as classes derived from the `<c c="name">` colour name (letters only) with a fixed dark-theme palette, never as inline styles.
- **Reason:** Dictionaries are designed for light backgrounds and their markup is untrusted input to `dangerouslySetInnerHTML`.

## Document Segmentation

- **Decision:** When a tokenizer is unavailable, bound document input with a conservative character budget after reserving space for the adapter prompt and model output. Split paragraphs at sentence boundaries, then whitespace, and finally Unicode scalar boundaries for a single oversized token.
- **Reason:** Character counts are not token counts, but a conservative reserve prevents unbounded context requests without adding a model-specific tokenizer dependency to the document subsystem.
- **Decision:** Subdivided blocks use stable `parent-id::part-NNNN` identities and ordered derived ordinals. Code, formula, and image blocks are not split automatically when oversized.
- **Reason:** Reconstruction needs stable mapping, while splitting protected content can corrupt identifiers or structure; unsupported oversized content must become an explicit job diagnostic.
- **Decision:** The Phase 4 worker persists each translated segment immediately and skips already translated rows. A runtime/model mismatch pauses the job; other block failures fail the job while retaining prior results.
- **Reason:** Process interruption and individual model failures must not discard committed work or silently mix translations from incompatible runtime snapshots.

## TXT Documents

- **Decision:** Phase 5 accepts UTF-8 with or without BOM and UTF-16 little/big endian only when a BOM is present. Ambiguous legacy encodings are rejected rather than guessed.
- **Reason:** Silent replacement of undecodable text would corrupt source content and make a later export impossible to audit.
- **Decision:** Normalize CRLF/CR to LF and represent non-empty double-newline-separated paragraphs as independent blocks. Export joins translated blocks with LF blank lines and never replaces the source.
- **Reason:** This gives stable, bounded reconstruction for TXT without pretending to preserve unknown whitespace semantics; the original remains available for comparison.
- **Decision:** The first Documents UI stores the active job id locally, while the native SQLite job remains authoritative. Startup marks interrupted active jobs recoverable before the UI queries them.
- **Reason:** UI notifications can be missed or the app can close during a blocking request; reopening must recover from persisted state rather than rely on frontend memory.

## User Dictionary Folder

- **Decision:** Search only user-provided StarDict dictionaries from a folder selected through the native folder picker; do not create or fall back to an app-data dictionary folder.
- **Reason:** Dictionary contents and licensing are user-controlled, while the application should not silently expose bundled or preloaded dictionary records.
