# TODO

## File Translation Plan

- [x] Complete Phase 1 of [the file translation plan](docs/FILE_TRANSLATION_PLAN.md): extraction, rendering, controlled replacement, and overflow behavior are recorded in `docs/FILE_TRANSLATION_PHASE1_REPORT.md`. PDFium notice review remains a release prerequisite; preserve all existing workflows and follow the per-phase validation gates.
- [x] Complete Phase 2: implement the independent document-job model, persistence, state transitions, and recovery tests before production format integration.
- [x] Complete Phase 3: serialize interactive and document inference, prioritize interactive requests, quiesce runtime lifecycle changes, and cover recovery/error/shutdown behavior. The document worker itself remains Phase 4 work.
- [x] Complete Phase 4: add conservative bounded segmentation and a recoverable persisted-block worker. Format parsers, export, and the user-facing file workflow remain Phase 5+ work.
- [x] Complete Phase 5: add TXT UTF-8/UTF-16 parsing, persisted file jobs, safe separate export, restart recovery, and the first Documents UI with pause/resume/cancel controls.
- [x] Implement Phase 6 DOCX package/XML processing, bounded analysis, package-preserving export, format-aware commands/UI, and diagnostics. Preserve untouched ZIP parts and relationships.
- [x] Implement Phase 7 EPUB package/XML processing, recoverable jobs, package-preserving export, format-aware commands/UI, and diagnostics for supported spine XHTML. Keep navigation metadata and PDF separately scoped.
- [x] Add separate FB2 v1 analysis, translation jobs, safe XML-preserving export, and Documents UI support for plain `.fb2` files. Metadata remains untranslated; unsupported text is diagnosed and preserved.
- [ ] Validate a deterministic translated EPUB with EPUBCheck or a real EPUB reader. No suitable validator/reader is installed on the current host.
- [ ] Validate a deterministic translated FB2 with an independent reader or structural validator; no FB2 validation tool is installed on the current host.
- [ ] Manually validate translated DOCX output in Microsoft Word and LibreOffice. This remains blocked on viewers not being installed on the current host.

## PDF

- [x] Phase 8 first increment: analysis, in-place layout-preserving export, job commands, Files panel support, unit and PDFium integration tests.
- [ ] (In progress, user tests by hand) Review real-model PDF output page by page and fix layout problems they report; see `STATUS.md` PDF Handoff.
- [ ] Even paragraph spacing (reflow vertically inside a page) and a second line for tight single-line list items.
- [ ] Paragraphs split across a page break are two blocks; consider joining them.
- [ ] Multi-column reading order, tables and figure captions; bundled OFL Cyrillic font instead of the Windows fonts.
- [ ] Verify the installer ships `pdfium/pdfium.dll` (run `scripts/fetch-pdfium.ps1`, then `npm run desktop:build`) bundle `resources/pdfium/licenses/` (the glob only copies files) and review PDFium third-party notices.
- [ ] (Optional, requested) Phase 9: export a PDF translation as a reading-friendly EPUB (and DOCX) from the same blocks.

## Next

- [x] Document translation stalls: closed by the user on 2026-09-30 after real-book testing (mitigations: streaming with idle timeout, tracing; cause never isolated).
- [x] (done, stream + 45 s idle timeout, tests in `runtimes/lm_studio.rs`) Add a runtime-boundary regression test for a model HTTP response that stalls or never completes; verify that the job transitions to a visible terminal/recoverable state and cannot remain apparently active indefinitely.
- [x] (done: native same-language rejection + picker guard) Prevent or clearly warn when a LingvoLoc-generated `.translated.<lang>.<ext>` output is selected as a new source, especially when analysis resolves to the same language (`en → en`).

- [ ] Manually verify on real machines: Download/Update llama.cpp on AMD/Intel and no-GPU setups (NVIDIA was confirmed); Start with Windows toggle; Add to PATH; installer on a machine without WebView2.
- [ ] Optionally remove the `Start with Windows` registry value from an NSIS uninstall hook and refresh its path when the executable moves.
- [ ] Optionally allow resizing the extension overlay from its left and top edges.

- [ ] Consider a Windows job object so an abnormal app crash cannot orphan `llama-server` (normal exit and tray Quit already stop it).

- [x] Map source language for user-selected StarDict dictionaries from `bookname`/file name (`rus-eng`, `Russian-English`, ...); target language is not mapped yet.
- [ ] (Optional, low priority; the user is satisfied with current lookup) Structured StarDict senses and cosmetic cleanup of article markup.
- [ ] (Optional) Add regression tests for malformed indexes, markup variants and multi-sense records if the dictionary code is touched again.

- [x] Drag-and-drop of files onto the window and the multi-file queue were verified by the user on real books (2026-09-30).
- [x] Progress polling uses `get_document_progress` (counters and state only, no blocks) instead of `get_document_job`.

- [ ] Next release: bump the version (root, desktop, extension `package.json`, `manifest.json`, `tauri.conf.json`, `Cargo.toml`, lock files), run `npm run check`, `npm run desktop:build`, `npm run extension:package`, then tag and `gh release create` (see the `v2.2.5` release for the notes format). Include the unreleased items listed in `STATUS.md`.

## Models

- [ ] Verify the Hunyuan-MT and Qwen adapters end to end in the desktop UI (only benchmark-script and unit-test coverage so far).
- [ ] Optionally translate long text paragraph by paragraph (split on blank lines in `services/translation.rs`, join the results) so weaker models cannot skip trailing paragraphs.
- [ ] Optionally show tokens/s in the extension popup (`apps/extension/src/popup.ts`); `completion_tokens` is already in the `TranslationResult` JSON returned by the loopback API.

## Before Public Release

- [ ] Complete formal trademark and domain review for the `LingvoLoc` name.
