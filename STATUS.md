# Status

## Works

- Repository coordination and documentation files are initialized.
- The Phase 0-1 implementation plan is defined.
- The local LM Studio endpoint and available TranslateGemma models have been verified.
- Phase 0 workspace scaffold, frontend checks, and Rust test harness are implemented.
- Phase 1 LM Studio runtime, TranslateGemma adapter, normalized commands, and initial UI flow are implemented.
- `npm run desktop:build` successfully produces the native executable.
- NSIS x64 installer bundling succeeds with `LingvoLoc_0.1.0_x64-setup.exe`.
- Native startup smoke test succeeds with window title `LingvoLoc`.
- Manual smoke test passed for English, Russian, and German translation flows.
- The initial Model Manager now shows model owner and quantization metadata and keeps adapter selection explicit.
- Model Manager supports manual model-list refresh; richer fields remain explicitly unknown when LM Studio omits them.
- Local language detection for English, Russian, German, Spanish, French, Italian, Portuguese, Polish, and Ukrainian is implemented without model inference.
- Translation history is persisted locally in SQLite and the UI shows paged entries.
- History supports text search and favorite toggling.
- History can be exported to a quoted CSV file in the system Downloads directory.
- History actions now show visible feedback and include frontend fallbacks for search/download.
- Search matches are highlighted in yellow in both source and translation text.
- The latest native installer includes history highlighting, visible action feedback, and frontend fallbacks.
- History results are paged in groups of 20 with a Load more action.
- Windows UI Automation sees the native window but does not expose WebView2 controls for automated clicks.
- The native executable starts successfully and remains running under a basic process smoke test.
- The latest release build includes the native system tray icon and its Show/Hide/Quit menu.
- Lexical records now expose sourced examples and provider labels, and seed/FreeDict facts are aggregated for a matching lemma.
- Closing the main window now hides it to the system tray instead of exiting; `Quit` is the explicit exit action.
- The main window has a normal taskbar button while visible; after close-to-tray, the window and its taskbar button are hidden while the tray icon remains.
- Desktop and extension surfaces now have a focused visual polish pass with stronger hierarchy, responsive narrow-window layouts, and visible keyboard focus states.
- The desktop UI shows the semantic app version and an automatically incremented production build number; the latest installer contains build 17.
- Dictionary lookup results are transient: a new lookup clears the prior cards, stale asynchronous responses are ignored, and a Clear action is available. Dictionary/model word requests do not write translation history.
- Both the main window and clipboard popup now expose persisted `-`/`+` text-size controls from 50% through 100%.
- The installed build 6 was tested end to end: Russian clipboard text translated to English through the global shortcut and the translated result was written back to the clipboard.
- Launching LingvoLoc from a pinned taskbar shortcut while it is hidden now focuses the existing process instead of creating a second copy.
- Global shortcuts are implemented: `Ctrl+Shift+L` shows the app, and `Ctrl+Shift+T` translates the current clipboard text.
- Clipboard translation now uses a compact always-on-top popup window with source preview, translated result, and copy feedback.
- Clipboard popup handoff now uses native clipboard capture plus a pending IPC request; it no longer depends on a startup event reaching a hidden WebView.
- Lexical MVP now provides local lemma lookup with part of speech, definitions, forms, translations, synonyms, antonyms, related words, and a dictionary UI.
- Partial WordNet fallback cards were removed from the default dictionary UX; unknown words now report no local match instead of showing incomplete facts.
- Dictionary lookup now searches both English and Russian seed entries regardless of the translation source selector.
- Double-clicking a word in either translation text area now fills Dictionary lookup; source selection also highlights the first matching target translation.
- Source-side highlighting now selects a dictionary translation that actually occurs in the current model result instead of assuming the first translation is present.
- Source-side highlighting now uses a separate LLM word translation request, so it does not depend on a local dictionary entry and does not add a history row.
- Translation-side word selection now reverse-translates the selected word and selects the matching source word in the left textarea, including common inflection-prefix matches.
- The Windows release executable no longer creates a console window; the previous blank window was the console subsystem, not a second LingvoLoc UI window.
- Successful translations are automatically copied to the system clipboard when WebView2 clipboard access is available.
- Automatic source detection is now the default; the primary language pair controls the detected target language.
- MIT license is included.
- Automatic language selection uses two independent pair selectors rather than fixed presets.
- Model selection is aligned with the other controls and refresh uses a compact icon button.
- On startup, the native window now fits the rendered content up to the available screen height; scrolling and manual resizing remain enabled.
- Native startup sizing permissions are explicitly configured in `src-tauri/capabilities/default.json`.
- A native system tray icon now provides `Show LingvoLoc`, `Hide LingvoLoc`, and `Quit` actions.

## In Progress

- The functional desktop, loopback API, and Chromium extension slices are complete. Lexical dictionary quality remains deferred to a focused data/UX pass.
- The latest release build is 17. Text-size controls, one preloaded EN/RU dictionary, transient Dictionary lookup behavior, user StarDict loading, visible Dictionary sources refresh/checkboxes, and preserved/sanitized StarDict HTML definitions are complete.

## Known Issues

- Tauri Windows prerequisites are installed; compiler tools are available through Visual Studio but are not on the general PATH.
- The branded LingvoLoc icon is now used for the executable and installer.
- LM Studio returns valid UTF-8 Cyrillic output; the adapter still rejects actual U+FFFD replacement characters as a defensive malformed-response check.
- Interactive control-level native smoke test is complete for the initial three languages.
- Detection currently supports nine languages: English, Russian, German, Spanish, French, Italian, Portuguese, Polish, and Ukrainian.
- History currently supports paging, search/favorites/export, schema versioning, and a full clear action; retention is capped at 1,000 non-favorite rows.
- The installed application must be updated manually from the latest NSIS artifact after local rebuilds.
- Tray, close-to-tray, pinned-shortcut reactivation, clipboard popup, and extension workflows have been functionally smoke-tested; future polish and broader regression coverage remain.
- Clipboard access depends on the WebView2 permission/context; the translation still succeeds and reports a notice if copying is unavailable.
- Product-name review selected `LingvoLoc`; final trademark and domain review remain required before public release.
- The lexical dataset combines a project-authored English/Russian seed lexicon with the bundled FreeDict index; Russian coverage still needs a larger imported dataset.
- A reproducible `npm run lexical:index` converter now combines Kaikki Russian JSONL with OpenCorpora-derived morphology TSV into the compact JSON shape used by the lexical service; the generated upstream dataset is not bundled yet.
- The lexical service now bundles a generated FreeDict+WikDict index with 99,359 English/Russian entries and preserves the upstream CC BY-SA 3.0 notices.
- The lexical converter preserves Kaikki/Wiktionary examples and provider provenance; Rust lookup aggregates seed and FreeDict fields without fabricating missing facts.
- Lexical aggregation now keeps different parts of speech separate while combining matching provider facts.
- The reproducible lexical indexer now supports arbitrary FreeDict language pairs and explicit Kaikki/morphology source languages without cross-language lemma collisions.
- The bundled lexical source is now intentionally limited to the preloaded EN/RU index plus user-selected StarDict folders; no additional Spanish bundled provider is used.
- The bundled 99,359-entry EN/RU index now exposes explicit `FreeDict ENG-RUS` and `FreeDict RUS-ENG` provider labels for Russian-first lookup and aggregation.
- User StarDict directories are now scanned from app data, including `.dict.dz` compression; the Dictionary lookup UI has `Refresh dictionaries` and persisted per-dictionary checkboxes.
- Installed build 8 starts successfully with the bundled FreeDict index; lexical Rust tests pass for both English and Russian entries.
- Model translations are marked `MODEL TRANSLATION` so they remain distinguishable from local dictionary facts.
- Lexical lookup still needs a broader legally compatible dataset and sense-aware aggregation; missing records are intentionally not filled with model-generated dictionary facts.
- Dictionary facts remain optional: word-to-result highlighting uses the active translation model when a local lexical entry is unavailable.
- An authenticated loopback API foundation is implemented for the future Chromium extension: status, models, language pair, and translation endpoints on `127.0.0.1:47831`.
- A minimal Chromium MV3 extension now supports token pairing, selected-text capture, context-menu selection, and popup translation through the local API.
- Extension build and typecheck pass; Chrome pairing, popup translation, and context-menu translation are functionally verified.
- Extension API tests pass and a distributable ZIP is produced at `apps/extension/LingvoLoc-extension-0.1.0.zip`.
- Extension pairing now has an explicit `Pair extension` confirmation and validates the token against the desktop status endpoint before storing it.
- Chrome MCP confirmed the local API is reachable and rejects unauthenticated requests with HTTP 401; direct extension toolbar interaction remains outside the MCP page API.
- Desktop startup now restores the last persisted model, validates it against LM Studio, and synchronizes the selected model into native state before extension requests.

## Next Step

- The next work item is to improve parsing and sense quality of the bundled `rus-eng` records and add optional language metadata mapping for user dictionaries.
- Dictionary lookup results are intentionally transient and separate from Recent translations. Existing historical rows from earlier builds can be removed with Clear history.
- RuWordNet was reviewed but is not being bundled: its public acquisition path requires a maintainer request and describes non-commercial distribution terms.
- Latest installer: `apps/desktop/src-tauri/target/release/bundle/nsis/LingvoLoc_0.1.0_x64-setup.exe` (build 17).
- Verification baseline: `npm run check`, `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`, and `npm run desktop:build` pass.
