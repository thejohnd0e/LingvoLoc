# Phase 7 EPUB Design

## Scope

Add EPUB analysis, recoverable translation, diagnostics, and safe separate
export to the existing desktop document-job workflow. This increment covers
EPUB only. FB2 and production PDF work remain separate tasks.

The first EPUB version supports ordinary XHTML paragraphs, headings, list
items, and table cells in the package spine. It preserves the EPUB container,
OPF metadata, CSS, images, fonts, hyperlinks, anchors, and unrelated package
entries. Unsupported XHTML constructs remain unchanged and produce diagnostics.

## Package Boundary

Read the ZIP package without extracting it to disk. Validate `mimetype` when
present, require `META-INF/container.xml`, resolve the rootfile safely, parse
the OPF manifest and spine, and process spine documents in spine order rather
than ZIP or filename order. Reject duplicate/unsafe names, encrypted entries,
missing required files, oversized entries, and external resource references.

Export rewrites only supported spine XHTML entries and raw-copies every other
ZIP entry. The source and an existing output are never replaced. XHTML is
serialized with namespace-aware XML events; resource URLs, ids, anchors,
styles, and non-text elements are retained.

## Blocks And Reconstruction

Create one stable `DocumentBlock` per supported non-empty semantic element.
The id is `<spine-entry>#<element-ordinal>`, and the ordinal follows spine
reading order. Source text is the concatenation of descendant text nodes.
Unsupported text-bearing elements are skipped atomically and produce a
diagnostic. Multi-run reconstruction distributes translated text across the
original text nodes, preserves leading/trailing XML whitespace, and reports a
formatting-boundary warning when a translation crosses multiple nodes.

EPUB jobs use `format = "epub"`, parser/configuration version `epub-v1`, and
the existing job store, worker, inference coordinator, pause/resume/cancel
commands, and diagnostics table. Export requires a fully translated active
job, rechecks the source hash, writes a same-directory temporary file, and
finishes as `completed_with_warnings` when diagnostics exist.

## Frontend

The Documents panel accepts `.txt`, `.docx`, and `.epub`, dispatches EPUB
analysis/start/resume/export commands, derives output extension and labels
from `job.format`, and keeps the existing TXT/DOCX behavior unchanged.

## Security And Limits

All archive and XML limits reuse the DOCX bounded-package policy unless EPUB
needs a stricter limit for nested package processing. ZIP paths cannot escape
the package namespace. XML external entities and external fetching are not
enabled. The parser does not follow HTTP URLs, JavaScript, or remote fonts.

## Validation

Use a self-authored EPUB fixture containing two spine documents in reverse ZIP
order, headings, lists, tables, an image, CSS, an anchor, and an unsupported
script/foreign element. Tests cover spine ordering, package limits, path
validation, unchanged resources, XHTML escaping, whitespace, diagnostics,
segmentation coalescing, source/output collision, and incomplete export.
Automated gates must pass. Word/LibreOffice validation does not apply; an EPUB
validator or a reader smoke check is required before release of this phase.
