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

## Next

- [ ] **P0: Root cause still unknown; mitigation + tracing shipped in source (rebuild and reinstall to use it). Reproduce and fix the real document hang at block 56.** Current evidence: job `txt-38624-1790741713343` reaches `coordinator_acquired` for block 56, then the HTTP request returns only after `120009 ms` with an error and no saved block. Capture the request payload and matching llama-server task/log state before changing the worker.
- [x] (done, stream + 45 s idle timeout, tests in `runtimes/lm_studio.rs`) Add a runtime-boundary regression test for a model HTTP response that stalls or never completes; verify that the job transitions to a visible terminal/recoverable state and cannot remain apparently active indefinitely.
- [x] (done: native same-language rejection + picker guard) Prevent or clearly warn when a LingvoLoc-generated `.translated.<lang>.<ext>` output is selected as a new source, especially when analysis resolves to the same language (`en → en`).

- [ ] Manually verify on real machines: Download/Update llama.cpp on AMD/Intel and no-GPU setups (NVIDIA was confirmed); Start with Windows toggle; Add to PATH; installer on a machine without WebView2.
- [ ] Optionally remove the `Start with Windows` registry value from an NSIS uninstall hook and refresh its path when the executable moves.
- [ ] Optionally allow resizing the extension overlay from its left and top edges.

- [ ] Consider a Windows job object so an abnormal app crash cannot orphan `llama-server` (normal exit and tray Quit already stop it).

- [x] Map source language for user-selected StarDict dictionaries from `bookname`/file name (`rus-eng`, `Russian-English`, ...); target language is not mapped yet.
- [ ] Improve StarDict record parsing and sense separation so common Russian-English dictionaries yield stable definitions, translations, forms, examples, synonyms, and antonyms where the source data provides them.
- [ ] Add regression tests (truncated index and language mapping done) for malformed indexes, dictionary type/markup variants, language metadata, and multi-sense records before changing the dictionary UI contract.

- [x] Drag-and-drop of files onto the window and the multi-file queue were verified by the user on real books (2026-09-30).
- [x] Progress polling uses `get_document_progress` (counters and state only, no blocks) instead of `get_document_job`.

## Models

- [ ] Verify the Hunyuan-MT and Qwen adapters end to end in the desktop UI (only benchmark-script and unit-test coverage so far).
- [ ] Optionally translate long text paragraph by paragraph (split on blank lines in `services/translation.rs`, join the results) so weaker models cannot skip trailing paragraphs.
- [ ] Optionally show tokens/s in the extension popup (`apps/extension/src/popup.ts`); `completion_tokens` is already in the `TranslationResult` JSON returned by the loopback API.

## Before Public Release

- [ ] Complete formal trademark and domain review for the `LingvoLoc` name.
