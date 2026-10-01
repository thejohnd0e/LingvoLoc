# File Translation Implementation Plan

## Status and purpose

- Created: 2026-09-29.
- Status: Phase 0 through Phase 6 DOCX and Phase 7 EPUB/FB2 implementations are complete in code; DOCX viewer validation and EPUB/FB2 reader or validator validation remain open. Production PDF implementation has not started.
- Inspected baseline: `8ada3cc` on `master`, following release `v2.1.1` (`8d20985`). Recheck the actual repository state before starting.
- Intended readers: Codex, Claude Code, and other implementation/review agents.
- This document records the agreed direction and phased implementation tasks. It does not authorize automatically executing every phase, publishing a release, or making commits.
- Start with Phase 0 when the user asks to begin. Execute one explicitly assigned phase or bounded subtask at a time.

## User requirements

The user's priority formats are PDF, EPUB, and FB2. Starting with TXT and DOCX is acceptable.

PDF inputs are mainly technical literature containing illustrations. Preserving the main layout is important and desirable. Exporting a PDF translation to EPUB or DOCX for comfortable reading is also useful.

Provide two eventual PDF output modes:

1. **Layout-preserving PDF:** retain the main arrangement of text, illustrations, tables, formulas, and captions as closely as supported.
2. **Reflowable output:** reconstruct reading order and export to DOCX or EPUB with illustrations near their associated text.

The primary constraint is preserving existing functionality. Implement file translation as an isolated subsystem, with narrowly scoped, tested integration into shared inference infrastructure.

## Agent execution rules

1. Read `AGENTS.md`, `STATUS.md`, `DECISIONS.md`, and `TODO.md` before implementation.
2. Inspect Git status and the current code. Preserve unrelated or user-authored changes.
3. Do not combine this work with dictionary improvements or broad refactoring of history, settings, runtime, or the main UI.
4. Preserve existing IPC/HTTP contracts and compatibility of `Settings`, `TranslationRequest`, and `TranslationResult`. Any necessary additive change requires compatibility tests and safe defaults.
5. Keep document-specific settings separate where practical. Do not change existing translation prompts globally to accommodate documents.
6. Keep the extension a thin client. File translation initially belongs to the desktop app; no extension API expansion is required.
7. Do not overwrite published release assets. Version changes, commits, and releases require separate instructions.
8. Run focused tests during development and the integration gates before declaring a phase complete. Fix failures before proceeding.
9. Record implementation status, checks actually run, limitations, and the precise next action in handoff documentation after each implementation phase.
10. Do not promise perfect layout or semantic completeness. Report unsupported content and uncertain results explicitly.

## Existing implementation facts

Revalidate these facts against the current source before changing shared code:

- `apps/desktop/src-tauri/src/services/translation.rs` translates one request through the selected adapter and runtime.
- History insertion occurs in callers in `lib.rs` and `api.rs`, rather than in the translation service. Document blocks can therefore use the service without creating thousands of history rows.
- `runtimes/llama_server.rs` owns one shared `llama-server` process, restarted when its model or executable changes. Its configured context size at the inspected baseline is 8192.
- Runtime HTTP requests use a blocking client. Blocking work must remain off the UI thread.
- Desktop translation, the loopback API, clipboard translation, and word alignment share translation resources. A background document must not race them or silently switch their model.
- Existing dependencies include ZIP, SQLite, Tauri dialogs, and the opener plugin. Assess reuse before adding dependencies.
- Current dictionary behavior must remain user-selected StarDict only.

## Proposed architecture

Names below are illustrative; follow repository conventions when implementing.

```text
apps/desktop/src-tauri/src/
  documents/
    mod.rs
    domain.rs
    commands.rs
    jobs.rs
    store.rs
    segmentation.rs
    validation.rs
    export.rs
    formats/
      txt.rs
      documents/docx/{mod,package,xml}.rs
      epub.rs
      fb2.rs
      pdf/
        mod.rs
        extract.rs
        layout.rs
        render.rs
  services/
    translation.rs
    inference_coordinator.rs

apps/desktop/src/features/documents/
  DocumentsPanel.tsx
  DocumentJobDetails.tsx
  commands.ts
  types.ts
```

Avoid implementing the subsystem inside the existing large `App.tsx` or `lib.rs`. Keep commands thin and format processing in dedicated modules.

### Document representation

Represent translatable content as stable, ordered blocks associated with the original format structure. Retain:

- Document identity, source-content hash, format, parser version, and source metadata.
- Chapters, sections, stable block identifiers, and reading order.
- Block types: heading, paragraph, list item, table cell, caption, footnote, code, formula, image.
- Original content and translations separately.
- Resource references and mapping back to source XML nodes or format-specific structures.
- PDF page, coordinates, rotation/crop context, and reading order where applicable.
- Diagnostics and unsupported-content inventory.

Do not flatten all formats to TXT. Use a shared translation/block model while retaining original containers and format-specific structures for reconstruction.

### Persistent jobs

Use an independent document-job store, provisionally `document-jobs.sqlite`, rather than modifying translation-history tables.

Suggested states:

```text
queued -> analyzing -> ready -> translating -> exporting -> completed
                                  |
                                  +-> pausing -> paused

Additional states: interrupted, completed_with_warnings, failed, cancelled
```

Specify valid transitions, recovery rules, and terminal-state semantics in code. Persist source identity, language direction, runtime/model snapshot, parser/segmentation/translation versions, block results, attempts, errors, and output location.

## Phase 0 — Establish the baseline and fixtures

### Tasks

- Record the starting commit and worktree state.
- Run the existing check, strict Clippy, and frontend build commands listed under Validation below.
- Distinguish pre-existing failures and missing environment prerequisites from regressions.
- Prepare the existing-feature smoke matrix before changing shared code.
- Obtain representative samples: TXT, DOCX with mixed formatting/table/link/image, EPUB/FB2 with navigation and notes, and technical PDFs with illustrations, formulas, tables, and columns.
- Use small self-authored or redistribution-permitted fixtures in the repository. Do not commit the user's books.

### Acceptance

The baseline is recorded, existing failures are understood, and representative comparison fixtures are available. If PDF samples are unavailable, record that dependency rather than claiming PDF feasibility has been established.

## Phase 1 — Early PDF feasibility investigation

Perform this before finalizing the document architecture, even though production PDF support comes later.

### Tasks

- Test Unicode text extraction, coordinates, reading order, columns, hyphenation, repeated headers/footers, captions, tables, formulas, raster images, and vector drawings.
- Test replacing original text and embedding fonts with Cyrillic coverage.
- Compare candidate tools for sample quality, redistribution license, Windows build/packaging, installed size, subprocess requirements, and optional model requirements.
- End users should not need a developer Python environment or an office suite installed. If a helper executable is necessary, define how it is packaged and licensed.
- Do not choose a PDF library solely for text extraction; investigate reconstruction and graphics preservation too.
- Keep the spike isolated. Do not integrate an experimental PDF dependency throughout the app.

### Deliverables and acceptance

Record the chosen approach, alternatives considered, supported initial PDF class, known limitations, dependencies, and Windows distribution strategy. Demonstrate extraction and reconstruction of representative pages with readable text and preserved illustrations. Full-book translation is not required.

Library selection remains open until this phase is completed. If no approach meets the layout requirement, present the tradeoff to the user before changing scope.

## Phase 2 — Job model, persistence, and recovery

### Tasks

- Implement the document model, job state machine, independent store, and a test translator.
- Save completed blocks transactionally, with idempotent recovery and no duplicate results.
- Detect changed sources using content hashes, not only filenames or timestamps.
- Preserve a reproducible settings snapshot and detect incompatible model/parser/configuration changes on resume.
- Require an explicit decision before mixing translations from different models or configurations.
- Recover interrupted jobs as stopped/interrupted, not automatically consuming GPU resources at startup.
- Define retention and deletion of source copies, extracted resources, intermediate results, and exported files. Never delete a user-owned source or export as an incidental cleanup action.

### Tests

Exercise restart/reopen, interruption around save boundaries, duplicate processing, changed source, missing temporary data, corrupted job data, and store-schema migration.

### Acceptance

A synthetic multi-block job resumes correctly after process restart without losing committed results or altering the existing history database.

## Phase 3 — Coordinate background and interactive inference

This is the highest-regression-risk integration phase. Inspect every inference entry point before choosing the integration seam.

### Initial policy

- One active model request at a time.
- Interactive requests take priority over the next document block.
- Do not forcibly preempt a request already running.
- Yield between document blocks; keep blocks bounded to avoid excessive interactive latency.
- Pause a document when the selected model/runtime changes, rather than silently switching the global runtime back.
- Coordinate runtime lifecycle operations that can stop the server: executable changes, llama.cpp updates, and application exit.
- Preserve ordinary translation behavior and contracts when no document job is active.

Do not hold settings, history, or job-store locks over inference/network waits. Define lock ordering and shutdown behavior explicitly.

### Cancellation semantics for the first version

The current blocking HTTP implementation does not imply instant request cancellation:

- Pause stops scheduling after the active request finishes.
- Cancel prevents subsequent blocks and export; define how a late response is discarded or retained internally.
- Waiting is bounded by configured request timeouts.
- Do not kill the shared server to cancel a single document.
- Closing the main window still hides the app to the tray; only actual exit stops workers. Jobs must remain recoverable after exit.

### Tests and acceptance

Use a controllable mock runtime to test document plus desktop/API/word requests, model changes, runtime updates, timeout, errors, cancellation, and shutdown. Check for deadlocks and unwanted server switching. Consider existing client timeouts when testing queued interactive requests.

Interactive work runs after the active block and before subsequent background blocks. Existing history rules and IPC/HTTP shapes remain compatible. Document jobs do not introduce artificial waits when idle.

## Phase 4 — Segmentation and block translation

### Tasks

- Split at structural boundaries; split oversized paragraphs at sentence boundaries.
- Budget input plus prompt plus output against the actual runtime constraints. Do not equate character counts with exact token counts.
- Use conservative estimates and reserves if tokenization is unavailable. Reduce oversized blocks on context-limit failures.
- Do not assume LM Studio uses the Standalone context limit.
- Preserve stable identities, ordering, and reconstruction mappings after subdivision.
- Protect code, formulas, URLs, identifiers, and format markers using source structure where possible.
- Limit retries to appropriate failures; retain successes when another block fails.
- Check empty output, missing protected markers, truncation indicators, suspiciously short output, and unfinished blocks.
- Treat these checks as diagnostics, not proof of semantic completeness.
- Resolve automatic source language from suitable document samples and allow correction. Do not redetect a short caption independently and silently change language direction. Define how mixed-language content is handled.

Adjacent-block context and glossary prompting require adapter-specific verification. Keep them separate from existing normal-translation prompts.

### Acceptance

A large synthetic document completes or stops recoverably with correct block order. Failure of one block does not discard prior work. Resource use is bounded rather than proportional to the entire translated book in frontend memory.

## Phase 5 — TXT and the first end-to-end UI

### Backend

- Support UTF-8 with/without BOM and UTF-16 with BOM.
- Handle ambiguous encodings explicitly; do not silently replace undecodable characters. Additional encodings may be a follow-up with explicit selection.
- Preserve paragraph boundaries and define CRLF/LF behavior.
- Export to a separate output using a temporary file and safe completion/rename semantics on Windows.
- Handle collisions and reject accidental replacement of the source.

### Frontend

Add a Documents/Files section with file picker, drag/drop, languages, model information, analysis, start, block progress, pause/resume/cancel, diagnostics, and open-output/open-folder actions.

- Keep components and CSS scoped to the feature.
- Switching sections must preserve the existing text, result, history position, and dictionary state.
- Backend owns the job. UI events are notifications; query a current snapshot when mounting/reopening to recover from missed events.
- Do not block the UI thread with parsing, archive operations, or translation.
- Keep file-job records separate from normal translation history.

### Acceptance

TXT supports the complete workflow including application restart. The existing translator and extension remain usable during a document job. Existing settings load without migration failures.

## Phase 6 — DOCX

### Tasks

- Process the ZIP/XML package, retaining untouched parts and relationships.
- Translate supported paragraphs, headings, lists, and table cells.
- Preserve images, styles, hyperlinks, numbering, and package relationships.
- Define support for footnotes/endnotes, headers/footers, text boxes, tracked changes, fields, and embedded objects explicitly.
- Translate semantic paragraphs rather than each individual text run.
- Implement and validate a strategy for formatting/link boundaries across reordered translated text.
- If boundary reconstruction fails, use a documented fallback and warning rather than dropping text or silently corrupting links.
- Inventory unsupported text-bearing parts, such as some SmartArt content, and report them. Do not label a partially translated document fully translated.

### Tests and acceptance

Verify mixed formatting, links, lists, tables, images, Unicode, and unchanged package parts. Open outputs in Word and LibreOffice without repair prompts. Preserve the source. Exact pagination is not guaranteed because translated text length changes.

**Release milestone A:** TXT and DOCX are ready for user testing after the full regression gate.

## Phase 7 — EPUB and FB2

### EPUB

- Follow package `spine` reading order, not filename order.
- Translate XHTML text while retaining structure, resources, and CSS.
- Update supported navigation labels and language metadata without rewriting unrelated bibliographic fields.
- Preserve anchors, note links, and identifiers.
- Rebuild the container correctly and validate it.
- Detect protected or unsupported books and return a clear limitation.

### FB2

- Preserve XML namespaces and document structure.
- Process sections, headings, paragraphs, epigraphs, and notes.
- Retain binary resources, image links, and technical identifiers.
- Define metadata translation policy explicitly.
- Treat archived FB2 support as a separately scoped addition.

### Acceptance

EPUB passes an appropriate validator; FB2 passes structural validation. Both open in readers with working navigation, images, and notes. No chapter is lost because one translation block failed.

**Release milestone B:** electronic books. This phase and production PDF support may be reordered based on Phase 1 results and the user's priorities.

## Phase 8 — Text PDFs with main-layout preservation

Initial scope: PDFs with extractable text; OCR is not required to claim this phase complete. Mixed/scanned pages must be identified and reported rather than silently skipped.

### Analysis

- Identify reading order, columns, headings, captions, repeated headers/footers, code, and formulas.
- Account for page rotation and crop regions.
- Associate captions with illustrations where reliable.
- Preserve raster and vector graphics.

### Reconstruction

- Replace/remove original text correctly while retaining graphical content.
- Do not merely paint white boxes that leave hidden original text or cover diagrams.
- Embed suitable fonts and position translated text in source regions.
- Preserve numbering and technical references.
- Leave text inside raster illustrations unchanged in the initial version and report this limitation.

### Overflow policy

1. Recalculate line wrapping.
2. Use available space inside the intended region.
3. Apply bounded font-size and line-spacing adjustments with readability thresholds.
4. Mark unresolved regions/pages for review rather than clipping text or shrinking it indefinitely.

Offer reflowable export where layout cannot be preserved satisfactorily. Distinguish completed output from output requiring review.

### Acceptance

For representative fixtures, inspect rendered pages as well as automated geometry/content checks. No overlapping captions/illustrations, clipped lines, lost formulas/code, or mixed original/translated text when copying. List all unresolved pages.

**Release milestone C:** layout-preserving translation for a documented class of technical PDFs, not an unrestricted promise for every PDF.

## Phase 9 — PDF to DOCX / EPUB

Use the extracted document structure with separate reflowable exporters.

### Tasks

- Reconstruct headings, chapters, and reading sequence.
- Remove repeated page furniture from the reading flow.
- Join line-break hyphenation carefully without damaging identifiers, terms, or code.
- Position illustrations near relevant text and captions.
- Reconstruct simple tables where reliable.
- Retain difficult formulas/tables as images if structured conversion is unreliable, and disclose any untranslated content this preserves.
- Optionally retain source-page references for navigation.

### Acceptance

The output reads in logical order on narrow screens and preserves technical illustrations. Navigation and links work where supported. Untranslated image-only material and uncertain reconstruction are reported.

**Release milestone D:** alternate reading-friendly outputs for PDFs.

## Phase 10 — OCR and quality improvements

Implement as separately approved follow-up tasks after the main path is stable.

### OCR

- Local recognition with explicit language/model packages.
- Rotation/deskew and mixed-document handling.
- Low-confidence diagnostics.
- Windows packaging and licensing of required components.

### Glossary

- Document or user-level terms, abbreviations, and names.
- Adapter-specific integration and adherence checks.
- Persist glossary version with the job.
- Explicit handling when glossary changes after partial translation.

### Further features

- Chapter/page-range selection.
- Sample translation before processing a whole book.
- Multiple-document queues.
- Better remaining-time estimates.
- Targeted retry of problematic blocks.

## File handling and resource requirements

Apply these incrementally as formats are introduced:

- Bound archive entry counts, decompressed sizes, and processing resources.
- Prevent archive paths from escaping the working directory.
- Disable external XML entities and external-resource fetching.
- Support Unicode paths and test Windows path/locking behavior.
- Keep content local except for requests to the configured runtime; do not introduce third-party document upload services.
- Do not include whole documents in ordinary diagnostic logs.
- Treat document text as translation data, not instructions to the application.
- Handle disk exhaustion, missing sources, locked outputs, and cancellation without corrupting originals or completed exports.
- Avoid loading entire large books and images into frontend state.
- Keep dependencies compatible with Windows installer distribution and document their licenses.

## Validation

Use deterministic mock translation for automated tests. Real-model quality checks are separate smoke tests, not mandatory network/GPU dependencies for every test run.

### Integration gate

```text
npm run check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
```

Run focused relevant tests first. Run the integration gate before marking a phase complete. Do not repeatedly rerun passing gates without new changes or unresolved concerns.

Run the native build at release milestones and when changing native dependencies, permissions, or packaging:

```text
npm run desktop:build
```

Verify an installed build on a clean Windows environment before shipping new native/document dependencies.

### Existing-feature regression matrix

| Area               | Required checks                                                                                        |
| ------------------ | ------------------------------------------------------------------------------------------------------ |
| Text translation   | Direction, model, output, timing, copy                                                                 |
| History            | Existing paging/search/favorites/export; one entry per ordinary translation; no document-block entries |
| Word selection     | Forward/reverse alignment without history insertion                                                    |
| Dictionaries       | Selection, lookup, separate provider cards, media                                                      |
| Clipboard popup    | First request, current settings/model, error reporting                                                 |
| Extension          | Pairing, API response compatibility, overlay, detached window                                          |
| Runtime            | Standalone and LM Studio, model switches, llama.cpp update                                             |
| Tray and shortcuts | Hide/reopen, single instance, shortcuts, Quit                                                          |
| Settings           | Existing saved settings still load; unchanged defaults                                                 |
| Background jobs    | Pause/recovery/error behavior and concurrent interactive requests                                      |

## Per-phase handoff template

At the end of an implementation phase report:

1. Assigned scope and what was implemented.
2. Changed files and any shared integration points.
3. Tests/builds actually run, with results; identify checks not performed.
4. Remaining limitations and blockers.
5. Exact next action and relevant files.
6. Whether the phase's acceptance criteria are met.

Update `STATUS.md` and `TODO.md` accurately; record architectural decisions in `DECISIONS.md`. Do not mark a feature complete based on mocks alone when its acceptance requires actual document viewers or native runtime smoke tests.

### Suggested implementation prompt

> Implement only Phase N (or the specified subtask) of `docs/FILE_TRANSLATION_PLAN.md`. Read the repository instructions and inspect the current state first. Preserve existing contracts and workflows, avoid unrelated refactoring, add behavior-focused tests for new mechanisms and affected shared components, and run the phase's validation checks. Report changes, verified results, limitations, and the exact continuation point. Do not proceed to the next phase, change versions, publish, or commit without separate instructions.

If the user explicitly requests multiple agents, one can implement and another review the diff/tests independently. Do not have multiple agents concurrently edit shared runtime modules.

## Milestones and current continuation point

- [x] Phase 0: baseline and fixtures. Baseline checks pass; TXT, DOCX, EPUB, and FB2 self-authored fixtures are present.
- [x] Phase 1: PDF feasibility and dependency decision. `pdfium-render` is the isolated candidate for the documented initial class; reconstruction remains a later production phase and must report overflow and unsupported content.
- [x] Phase 2: persistent jobs and recovery.
- [x] Phase 3: inference coordination with regression coverage.
- [x] Phase 4: segmentation and block translation.
- [x] Phase 5: end-to-end TXT and UI.
- [x] Phase 6: DOCX implementation complete; first user-facing release milestone remains pending Word/LibreOffice viewer validation.
- [x] Phase 7 EPUB: implementation complete for bounded spine XHTML paragraphs, headings, list items, and table cells; validator/reader smoke validation remains open.
- [x] Phase 7 FB2: separate `fb2-v1` implementation supports section headings, body/epigraph/note paragraphs, safe XML-preserving export, and diagnostics; independent reader/validator validation remains open.
- [ ] Phase 7 validation: open deterministic translated EPUB and FB2 fixtures in independent readers or validators; DOCX Word/LibreOffice validation also remains open.
- [~] Phase 8: technical PDF layout preservation (first increment implemented; see STATUS.md for limits).
- [ ] Phase 9: PDF to DOCX/EPUB.
- [ ] Phase 10: separately scoped OCR and quality improvements.

**Current continuation point:** Phase 0 through Phase 6 and EPUB/FB2 Phase 7 implementations are complete. EPUB supports bounded spine XHTML analysis and package-preserving export; navigation labels and OPF bibliographic metadata are not translated. FB2 supports plain XML files, translates section titles, paragraphs, epigraphs, and notes, preserves metadata and binary resources, and reports unsupported text. Automated checks cover both, but independent EPUB/FB2 reader validation and DOCX viewer validation remain open. Next, validate deterministic outputs with independent readers/validators; keep `pdfium-render` and production PDF processing isolated until release validation is recorded.

**New-session handoff:** Before release, validate deterministic translated EPUB and FB2 fixtures with independent readers or validators, recording package/XML validity, navigation, images, and notes. Also manually open a deterministic translated DOCX fixture in Microsoft Word and LibreOffice, confirm no repair prompt, and record the results. The current TXT workflow is the regression baseline and must remain unchanged.
