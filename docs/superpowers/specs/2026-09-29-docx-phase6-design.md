# Phase 6 DOCX Design

## Scope

Phase 6 adds DOCX analysis, translation, recovery, and safe separate export to
the existing document-job subsystem. It reuses the persisted job store,
segmentation worker, inference coordinator, and Documents panel. TXT behavior,
the extension API, ordinary translation history, runtime settings, and global
translation prompts remain unchanged.

The first DOCX version translates supported paragraphs in
`word/document.xml`: ordinary paragraphs, headings, numbered/list paragraphs,
and paragraphs inside table cells. It preserves images, styles, numbering,
hyperlinks, content types, and package relationships.

Headers, footers, footnotes, endnotes, comments, text boxes, tracked changes,
fields, SmartArt, Office Math, content controls, and embedded objects are not
translated in this phase. They remain byte-for-byte unchanged where they live
in untouched package parts. Their presence is reported as a diagnostic, and an
export containing such content finishes as `completed_with_warnings`.

## Package Boundary

DOCX processing is isolated in `documents/docx.rs`. The module reads an OPC ZIP
package without extracting it to disk. Analysis validates required package
parts and extracts format-neutral `DocumentBlock` values. Export reopens the
source package, rewrites only `word/document.xml`, and raw-copies every other
entry so compression, relationships, media, and producer-specific extensions
are retained.

`quick-xml` 0.42 becomes a direct dependency because it is already present in
the lock graph and provides checked, event-based XML reading and writing. The
implementation does not use a high-level DOCX object model or serialize a new
package from an incomplete schema.

Input is bounded before processing: package size, entry count, individual
uncompressed entry size, total uncompressed size, XML size, and XML nesting
depth have fixed limits. Duplicate names, absolute paths, traversal segments,
encrypted entries, and missing required parts are rejected. XML entities are
not resolved externally and no package resource is fetched from the network.

## Block Extraction

The parser walks `word/document.xml` in document order and emits one block per
supported semantic paragraph. A stable block id contains the main-part name and
paragraph ordinal. The block type is selected in this order:

1. `TableCell` when the paragraph is inside `w:tc`.
2. `ListItem` when paragraph properties contain `w:numPr`.
3. `Heading` when the paragraph style is `Title` or starts with `Heading`.
4. `Paragraph` otherwise.

Visible text is the concatenation of supported `w:t` nodes. Empty paragraphs
and drawing-only paragraphs do not become translation blocks. A paragraph that
contains a field, revision, text box, object, math, or another unsupported
text-bearing construct is skipped intact and produces a diagnostic rather than
being partially translated.

## Reconstruction

Export parses the main document as XML events and copies every event by default.
For each supported paragraph it looks up the persisted translation by stable
block id. Segmented `::part-NNNN` rows are grouped back into their parent block
in ordinal order before reconstruction.

The translated paragraph is distributed across the original `w:t` nodes using
their source Unicode-scalar proportions, with split points moved to nearby
whitespace where possible. This retains the original runs, run properties, and
hyperlink containers. Because translated word order can differ, any paragraph
with more than one text boundary records a formatting-boundary diagnostic; the
document remains structurally valid, but exact semantic formatting alignment is
not promised.

If a supported paragraph has no usable text target during export, export fails
rather than dropping text. Replacement text is escaped by the XML writer.
`xml:space="preserve"` is retained or added when a replacement fragment starts
or ends with XML whitespace.

## Jobs And Commands

DOCX jobs use `format = "docx"`, parser version `docx-v1`, and configuration
version `docx-v1`. Additive native commands mirror the TXT boundary:

- `analyze_docx`
- `start_docx_job`
- `resume_docx_job`
- `export_docx_job`

Shared get, pause, and cancel commands remain unchanged. Starting or resuming a
job validates its source hash and runtime snapshot through the existing worker.
Export requires all blocks translated, refuses source replacement and existing
targets, writes a same-directory temporary file, and renames it on success.

Diagnostics are persisted in a dedicated document-job diagnostics table rather
than overloading the job error field. `DocumentJobView` exposes diagnostics as
an additive array. Export transitions to `completed_with_warnings` when the
array is non-empty, otherwise to `completed`.

## Frontend

The Documents panel accepts `.txt` and `.docx`, dispatches format-specific
analysis/start/resume/export commands, and derives labels, filters, and default
output extensions from `job.format`. Progress uses “blocks” rather than
“paragraphs”. Diagnostics are displayed with the active job. The existing
single persisted job id remains sufficient because the job snapshot includes
the format.

No broad App or CSS redesign is included.

## Validation

Tests first build a temporary `.docx` from the self-authored unpacked fixture.
Coverage includes headings, mixed runs, hyperlinks, lists, tables, images,
Unicode, XML whitespace, segmented block reconstruction, unsupported-content
diagnostics, archive limits, unsafe names, source/output collision handling,
and incomplete export rejection.

Package tests compare every untouched entry byte-for-byte, including `.rels`,
media, styles, numbering, and content types. The rewritten main document must be
well-formed and contain every translated block. Existing TXT, store, worker,
frontend, extension, and full repository gates remain regression requirements.

Acceptance also requires opening generated output in Microsoft Word and
LibreOffice without a repair prompt. If either viewer is unavailable, automated
package verification may pass but Phase 6 remains explicitly unverified for
that viewer.
