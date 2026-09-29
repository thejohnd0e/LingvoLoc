# Decisions

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

## User Dictionary Folder

- **Decision:** Search only user-provided StarDict dictionaries from a folder selected through the native folder picker; do not create or fall back to an app-data dictionary folder.
- **Reason:** Dictionary contents and licensing are user-controlled, while the application should not silently expose bundled or preloaded dictionary records.

## Phase 7 EPUB

- **Decision:** Process EPUB spine XHTML in spine order, translating ordinary paragraphs, headings, list items, and table cells; raw-copy all other ZIP entries and preserve unsupported XHTML with diagnostics.
- **Reason:** This bounds reconstruction while retaining resources, CSS, anchors, links, identifiers, and package metadata.
- **Decision:** Use `epub-v1` with the existing recoverable document-job/export pipeline and finish exports with warnings when diagnostics exist.
- **Reason:** EPUB jobs need the same source/runtime safeguards as TXT and DOCX while unsupported constructs remain visible.
- **Decision:** Leave navigation labels and OPF bibliographic metadata unchanged in the first EPUB increment.
- **Reason:** The implemented rewrite scope has no dedicated mappings for those fields.
- **Decision:** Treat an EPUB validator or reader smoke check as a release gate separate from automated tests.
- **Reason:** Independent EPUB consumers require a separate validator or reader check.
