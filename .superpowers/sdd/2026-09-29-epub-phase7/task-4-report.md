# Task 4 Report

## Status

Implemented the EPUB-aware Documents panel changes from Task 4.

- Added `analyzeEpub`, `startEpubJob`, `resumeEpubJob`, and `exportEpubJob` wrappers.
- Accepted `.epub` in the Documents picker.
- Routed analysis, start, resume, and export by document format.
- Derived EPUB output extension, save filter, labels, and status messages from the job format.
- Kept TXT/DOCX behavior and existing frontend assertions in place.
- Kept `DocumentJobView.diagnostics` additive and unchanged.

## TDD Evidence

- Red: the new EPUB tests initially failed while the existing seven tests passed; the panel did not dispatch EPUB analysis/start/resume/export.
- Green: `npm test --workspace @lingvoloc/desktop -- src/app/App.test.tsx` passed with 10 tests.
- Typecheck: `npm run typecheck` passed.
- Formatting: Prettier check passed after formatting the three touched frontend files.

## Concerns

- Full repository gates were not run; Task 4 requested focused frontend tests and desktop typecheck.
- EPUB reader/validator validation remains a later Task 5 concern.
- The worktree contained unrelated pre-existing changes; they were not reverted or staged.

## Task 4 Review Fixes

- Added parameterized TXT, DOCX, and EPUB workflow coverage for picker filters, analysis, start, resume, export dispatch, output extension, and save filters.
- Added direct command-wrapper tests covering native command names and payloads for all three formats.
- Added accessible duplicate-diagnostics rendering coverage and changed diagnostic keys to include the item index.

## Review-Fix Verification

- Red: the new diagnostics test reproduced React's duplicate-key warning while the workflow and wrapper tests passed.
- Green: focused frontend tests passed with 17 tests across `App.test.tsx` and `commands.test.ts`.
- Desktop typecheck and Prettier checks passed.
