# Decisions

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

- **Decision:** Use Tauri's built-in tray icon with explicit `Show LingvoLoc`, `Hide LingvoLoc`, and `Quit` menu actions.
- **Reason:** This adds a native Windows background entry point without introducing another plugin or changing the main translation flow.
- **Decision:** Intercept the main window's close request and hide the window; only the tray `Quit` action exits the process.
- **Reason:** Closing the window should behave as minimize-to-tray so the local translation service remains immediately available.
- **Decision:** Keep the normal taskbar button while the main window is visible; use the tray icon as the background-app entry point after hiding it.
- **Reason:** The visible application should remain pinnable and quickly accessible from the taskbar, while the hidden application should leave only its tray icon.
- **Decision:** Build the Windows release executable with the GUI subsystem rather than the console subsystem.
- **Reason:** A desktop Tauri application must not leave a blank console window or taskbar entry alongside its native window and tray icon.
- **Decision:** Use Tauri's single-instance plugin to route launches from a pinned shortcut to the existing hidden process.
- **Reason:** Hiding on close keeps the application available in the tray, so a pinned shortcut must reactivate that process instead of launching another copy.

## Global Shortcuts

- **Decision:** Register `Ctrl+Shift+L` to show LingvoLoc and `Ctrl+Shift+T` to translate the current clipboard text.
- **Reason:** These shortcuts provide a fast Windows workflow while keeping the translation UI and clipboard permissions in the existing frontend.

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

## Local Extension API

- **Decision:** Start the extension integration with a loopback HTTP API bound to `127.0.0.1:47831`, authenticated by a per-process bearer token and restricted CORS origins.
- **Reason:** The browser extension must remain a thin client while arbitrary websites must not be able to invoke privileged local translation actions.

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

## WordNet Dictionary Fallback

- **Decision:** Do not show partial WordNet or model-generated records in the default dictionary lookup.
- **Reason:** Synonyms without a sourced definition, translation, and sense context are misleading when rendered as a normal dictionary card; missing coverage is safer to report explicitly.

## User Dictionary Folder

- **Decision:** Search only user-provided StarDict dictionaries from a folder selected through the native folder picker; do not create or fall back to an app-data dictionary folder.
- **Reason:** Dictionary contents and licensing are user-controlled, while the application should not silently expose bundled or preloaded dictionary records.
