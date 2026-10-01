# PDF v2 Layout Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace independent PDF paragraph fitting with deterministic page-level flow layout that preserves readable spacing, lists, columns, tables, captions, and conservatively joined cross-page paragraphs, then ship the required fonts and PDFium notices in a verified Windows installer.

**Architecture:** PDFium remains responsible for reading, editing, and saving the original PDF. Pure Rust modules build a deterministic logical document model from PDF objects, identify page regions and reading order, and solve vertical placement for all translated paragraphs in each flow instead of fitting each paragraph independently. Export re-runs the same `pdf-v2` analysis on the hash-checked source, removes only translated source text objects, and draws the planned text while retaining images, paths, panels, annotations, and skipped content.

**Tech Stack:** Rust 2021, `pdfium-render 0.9.4`, `ttf-parser 0.25`, SQLite document jobs, Tauri 2 resources/NSIS, self-authored PDF fixtures, existing PDFium render probes, React/Vitest for the Files panel.

**Spec:** `PDF_solution.md`, especially the Design Contract and Acceptance Criteria sections below. This plan records the design approved in chat on 2026-09-30.

## Global Constraints

- Keep PDF export in-place: preserve original pages, vector graphics, images, backgrounds, annotations, and all intentionally skipped text.
- Never cover source text with white rectangles; remove only source text objects owned by translated logical blocks.
- Keep code, page numbers, short all-caps labels, lone list markers, and lone dashes unchanged.
- A page-sized fill is page paper, not a panel; preserve the existing `MAX_PANEL_HEIGHT` behavior.
- Do not let translated text cross image bounds, panel bounds, table rules, table cells, underline rules, or column gutters.
- Do not move ordinary paragraphs to another page. Only a source paragraph proven to span adjacent selected pages may use frames on both pages.
- Use one scale and one line-spacing ratio for compatible body text within a flow whenever possible. Individual emergency shrinking is the final fallback and must produce a diagnostic.
- Keep PDF geometry ephemeral. Store plain logical blocks in `document-jobs.sqlite`; re-run deterministic analysis during export.
- Bump the PDF parser/configuration version to `pdf-v2`. Reject start, resume, and export for older PDF jobs with instructions to remove and add the source again.
- Any analysis change that can alter grouping, reading order, skip rules, or block numbering requires clearing and re-adding existing PDF jobs.
- Preserve page-range syntax and partial-output naming. Cross-page joins may occur only inside each contiguous run of selected pages.
- Use bundled static Noto Sans and Noto Serif version 2.015 fonts under SIL OFL 1.1. Production PDF export must not depend on `C:\Windows\Fonts`.
- Keep scans, raster-image text, rotated pages, full justification, OCR, and Phase 9 EPUB/DOCX export outside this plan.
- Preserve unrelated worktree changes. Do not reset or rewrite files outside the task being executed.
- Run the focused test after every red/green cycle. Run the complete local gate before release.

---

## Design Contract

### Module Boundaries

`apps/desktop/src-tauri/src/documents/pdf/layout.rs` owns primitive geometry, fragment-to-line grouping, line-to-paragraph grouping, text classification, wrapping, and font measurement interfaces. It must not call PDFium.

`apps/desktop/src-tauri/src/documents/pdf/regions.rs` owns structural page analysis: sections, full-width spans, body columns, panels, table cells, captions, obstacles, reading order, list identity, and conservative cross-page paragraph joins. It consumes pure geometry and must not call PDFium.

`apps/desktop/src-tauri/src/documents/pdf/reflow.rs` owns translated-text layout. It consumes ordered flows, logical blocks, styles, frames, and obstacles, then returns text lines with page, x, baseline, size, and owning block/segment. It must not call PDFium.

`apps/desktop/src-tauri/src/documents/pdf/mod.rs` owns PDFium adaptation and orchestration: reading page objects into pure inputs, building one deterministic document model for analysis/export, mapping stored translations back to logical blocks, removing source objects, drawing planned lines, saving bytes, and diagnostics.

`apps/desktop/src-tauri/src/documents/pdf/fonts.rs` owns bundled font bytes and metrics only.

### Core Interfaces

The implementation may add private helper fields, but these names and responsibilities must remain stable across tasks:

```rust
// regions.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Body,
    Panel,
    TableCell,
    Caption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowId {
    pub section: usize,
    pub lane: usize,
}

#[derive(Debug, Clone)]
pub struct FlowRegion {
    pub id: FlowId,
    pub kind: RegionKind,
    pub bounds: Rect,
    pub paragraph_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PageRegions {
    pub flows: Vec<FlowRegion>,
    pub reading_order: Vec<usize>,
    pub obstacles: Vec<Rect>,
    pub diagnostics: Vec<String>,
}

pub fn detect_regions(
    paragraphs: &[Paragraph],
    backgrounds: &[Background],
    rules: &[Rect],
    images: &[Rect],
    page_bounds: Rect,
) -> PageRegions;
```

```rust
// reflow.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub pitch: f32,
    pub bold: bool,
    pub italic: bool,
    pub serif: bool,
    pub color: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct FlowItem {
    pub block_id: String,
    pub text: String,
    pub block_type: BlockType,
    pub style: TextStyle,
    pub original_bounds: Rect,
    pub original_lines: usize,
    pub preferred_gap_after: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowFrame {
    pub page: usize,
    pub bounds: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedLine {
    pub block_id: String,
    pub page: usize,
    pub text: String,
    pub x: f32,
    pub baseline: f32,
    pub size: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowPlan {
    pub lines: Vec<PlannedLine>,
    pub scale: f32,
    pub spacing_ratio: f32,
    pub overflow_blocks: Vec<String>,
}

pub fn place_flow(
    items: &[FlowItem],
    frames: &[FlowFrame],
    measure: &dyn Measure,
) -> FlowPlan;
```

`place_flow()` uses this fallback order: preferred gaps, compressed gaps, tighter shared leading, smaller shared scale, then per-block emergency placement. It never shrinks a list item merely to keep the source line count.

### Logical Blocks

A deterministic internal `LogicalBlock` in `mod.rs` maps one stored `DocumentBlock` to one or more source segments:

```rust
struct PageAnalysis {
    page: usize,
    fragments: Vec<Fragment>,
    paragraphs: Vec<Paragraph>,
    regions: PageRegions,
    backgrounds: Vec<Background>,
    rules: Vec<Rect>,
    images: Vec<Rect>,
    bounds: Rect,
    supported: bool,
}

struct SourceSegment {
    page: usize,
    paragraph: usize,
    objects: Vec<usize>,
    bounds: Rect,
}

struct LogicalBlock {
    id: String,
    block_type: BlockType,
    source_text: String,
    segments: Vec<SourceSegment>,
}
```

Single-page block IDs remain `pdf#pNNNN-KKK`, where `NNNN` is the one-based anchor page and `KKK` is the logical translatable-block index in that page's reading order. A cross-page block keeps the ID assigned on its first page and does not allocate another stored block on its continuation page.

### Reflow Policy

- Preferred body gap is the source gap clamped to `0.35..=1.0` times the dominant line pitch.
- Minimum heading gap is `0.6em` after the heading.
- Minimum gap between a heading and a following table or panel is `0.5em`.
- Extra vertical space remains after the final item once preferred gaps have been satisfied; it is not stretched into oversized paragraph gaps.
- Shared leading may tighten from the source median to no less than `1.08 * scaled_font_size`.
- Shared font scale uses the existing discrete scale sequence and should stay at or above `0.80` for body text whenever possible.
- A scale below `0.68`, an unbreakable over-wide word, exhausted frames, or any emergency per-block scale produces a review diagnostic containing the page and block ID.
- List markers remain untouched source objects. List text begins after the marker, and continuation lines align with list text rather than overlapping the marker.
- Table-cell and panel flows never borrow space from another cell or panel.

### Region And Reading-Order Policy

- Detect persistent vertical gutters from body-paragraph horizontal coverage within a section.
- A gutter must be at least `1.5em` wide and remain empty through at least 60% of the section's usable height.
- Support one, two, or three body columns. Ambiguous geometry falls back to one column and emits a diagnostic rather than interleaving text.
- A paragraph spanning at least 75% of page text width is full-width and divides column sections when it is vertically separated from neighboring body lines.
- Filled panels form independent regions. Page-sized fills remain excluded by `MAX_PANEL_HEIGHT`.
- Rule grids and repeated row-aligned text form table cells. Each cell is its own fixed region.
- A short paragraph directly above or below an image, horizontally aligned to at least 60% of the image width, is a caption region.
- Reading order is section top-to-bottom, then columns left-to-right, with paragraphs top-to-bottom inside each column. Full-width spans occur at their vertical position between sections.

### Cross-Page Join Policy

Join the last logical paragraph of page N to the first logical paragraph of page N+1 only when all conditions hold:

- Both pages belong to the same contiguous selected-page run.
- Both segments are `Body` or both are the same `ListItem` continuation.
- They occupy the same column index and have compatible left/right bounds.
- Font size differs by no more than 5%, and bold, italic, serif, and color match.
- The first segment reaches the bottom text margin and the second begins at the top text margin.
- The first segment does not end with `.`, `!`, `?`, `:`, `;`, a closing list marker, or another strong paragraph terminator.
- A trailing source hyphen is removed only when both adjacent characters are alphabetic and concatenating them produces one word.

Export wraps the single translation, allocates complete lines to the original segment frames in order, and then lets each page-local flow position its allocated lines. It never splits inside a word.

## Acceptance Criteria

- Real-model pages 1-10 of `example_books/Wahba E. - Claude Code Mastery - 2026.pdf` have no overlapping translated paragraphs, table crossings, panel crossings, or bullet collisions.
- Body paragraphs of the same style in one flow use the same font size unless an emergency block is explicitly diagnosed.
- A translated list item that naturally wraps to two lines at the flow scale remains two lines and moves following content instead of shrinking to one line.
- Shorter translations do not leave inconsistent holes between neighboring body paragraphs.
- Headings immediately above tables retain at least the specified minimum gap.
- Two-column and three-column fixtures produce reading-order blocks column-by-column rather than row-by-row across columns.
- Table text remains inside its source cell; caption text remains associated with its image.
- A conservatively detected cross-page paragraph becomes one translation block and renders across both source segment frames.
- Exported text extraction returns U+0020 for ordinary spaces and contains no replaced source text.
- Rotated, scanned, image-only, code, page-number, and short-label content remains unchanged and appears in diagnostics where already required.
- Old `pdf-v1` jobs cannot start, resume, or export under `pdf-v2`; the error instructs the user to remove and add the PDF again.
- The NSIS installer contains `pdfium/pdfium.dll`, `pdfium/PDFIUM-LICENSE.txt`, every file from `pdfium/licenses/`, and the Noto OFL notice.
- `npm run check`, strict Rust clippy, desktop frontend build, and NSIS build pass.

---

### Task 1: Capture A Reproducible Real-Translation Baseline

**Files:**

- Modify: `apps/desktop/src-tauri/examples/pdf_try.rs`
- Modify: `docs/fixtures/pdf/README.md`
- Do not commit: `example_books/`, extracted user translations, rendered real-book pages

**Interfaces:**

- Consumes: existing `PDF_TRANSLATIONS=<json-path>` replay path and `document-jobs.sqlite` schema
- Produces: `PDF_JOB_DB` and `PDF_JOB_ID` developer inputs that load `block_id -> translated_text` directly for `pdf_try`

- [ ] **Step 1: Write a failing unit-testable loader**

Move database loading into a function that can be tested with a temporary SQLite database:

```rust
fn load_job_translations(
    database: &std::path::Path,
    job_id: &str,
) -> Result<std::collections::HashMap<String, String>, String>;
```

Add a test that inserts translated and untranslated rows for two jobs and expects only non-empty translations for the requested job.

- [ ] **Step 2: Run the focused test and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --example pdf_try load_job_translations`

Expected: FAIL because `load_job_translations()` does not exist.

- [ ] **Step 3: Implement the database loader and precedence**

Use `rusqlite::Connection::open()` and this query:

```sql
SELECT block_id, translated_text
FROM document_blocks
WHERE job_id = ?1
  AND translated_text IS NOT NULL
  AND trim(translated_text) <> ''
ORDER BY ordinal
```

Keep `PDF_TRANSLATIONS` as the first-priority input. If it is absent, read `PDF_JOB_DB` plus `PDF_JOB_ID`. If neither source is configured, retain the existing fake translation.

- [ ] **Step 4: Run the focused test and replay the real job**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --example pdf_try load_job_translations`

Expected: PASS.

Run from `apps/desktop/src-tauri` after building the example:

```powershell
$env:LINGVOLOC_PDFIUM = 'D:\MyProjects\LoTran\tools\pdf-feasibility\bin\pdfium.dll'
$env:PDF_JOB_DB = "$env:APPDATA\com.lingoloc.desktop\document-jobs.sqlite"
$env:PDF_JOB_ID = '<the completed Wahba PDF job id>'
target\release\examples\pdf_try.exe '..\..\..\example_books\Wahba E. - Claude Code Mastery - 2026.pdf' "$env:TEMP\wahba-pdf-v1.pdf" '' '1-10'
```

Expected: `real translations` is greater than zero and output is created outside the repository.

- [ ] **Step 5: Record the baseline review matrix**

Add a table to `docs/fixtures/pdf/README.md` with columns `Case`, `Source`, `Pages`, `Expected`, `Observed before PDF v2`, and `Observed after PDF v2`. Record the known cases: uneven body gaps, tight long list item, split paragraph, columns, heading before table, panel/code preservation.

- [ ] **Step 6: Commit the baseline tooling**

```bash
git add apps/desktop/src-tauri/examples/pdf_try.rs docs/fixtures/pdf/README.md
git commit -m "test(pdf): add real translation replay baseline"
```

### Task 2: Split Pure Layout Responsibilities Without Behavior Changes

**Files:**

- Create: `apps/desktop/src-tauri/src/documents/pdf/regions.rs`
- Create: `apps/desktop/src-tauri/src/documents/pdf/reflow.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/layout.rs`

**Interfaces:**

- Consumes: existing `Rect`, `Background`, `Fragment`, `Line`, `Paragraph`, `Measure`, `wrap()`, and `place()` behavior
- Produces: module declarations and the Core Interfaces types; existing export output remains unchanged in this task

- [ ] **Step 1: Add compile-time interface tests**

Add minimal tests in `regions.rs` and `reflow.rs` that construct every public type from the Core Interfaces section. The `detect_regions()` smoke test should return one body flow for two vertically ordered paragraphs. The `place_flow()` smoke test should return lines for one item in one frame.

- [ ] **Step 2: Run the PDF module tests and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: FAIL because the modules and interfaces do not exist.

- [ ] **Step 3: Create the modules with minimal behavior**

Declare `mod regions; mod reflow;` in `pdf/mod.rs`. Implement `detect_regions()` as a single body flow ordered by descending paragraph top. Implement `place_flow()` by adapting existing `layout::place()` results without changing production export yet.

- [ ] **Step 4: Keep old behavior green**

Do not delete `layout::place()` yet. Run:

`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: all existing PDF tests and the two new smoke tests PASS.

- [ ] **Step 5: Commit the module seam**

```bash
git add apps/desktop/src-tauri/src/documents/pdf
git commit -m "refactor(pdf): separate regions and reflow modules"
```

### Task 3: Version PDF v2 And Reject Stale Jobs

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs:22`
- Modify: `apps/desktop/src-tauri/src/documents/commands/pdf.rs:28-51,150-221`
- Test: `apps/desktop/src-tauri/src/documents/commands/pdf.rs`

**Interfaces:**

- Consumes: `DocumentJob.parser_version`, `format::PARSER_VERSION`
- Produces: `fn validate_parser_version(job: &DocumentJob) -> Result<(), RuntimeError>`

- [ ] **Step 1: Write stale-job tests**

Add tests that set `job.parser_version = "pdf-v1"` and assert that start/resume/export validation returns:

```text
PDF layout analysis changed; remove this job and add the PDF again
```

Also assert that a `pdf-v2` job passes the parser check.

- [ ] **Step 2: Run the focused command tests and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::commands::pdf::tests`

Expected: FAIL because stale parser versions are accepted.

- [ ] **Step 3: Implement strict version validation**

Change:

```rust
pub const PARSER_VERSION: &str = "pdf-v2";
```

Call `validate_parser_version()` before transitions or worker calls in `start_pdf_job()` and `resume_pdf_job()`, and before block/export validation in `export_pdf_job()`.

- [ ] **Step 4: Verify stale and current jobs**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::commands::pdf::tests`

Expected: PASS, including existing page-range configuration tests.

- [ ] **Step 5: Commit the version boundary**

```bash
git add apps/desktop/src-tauri/src/documents/pdf/mod.rs apps/desktop/src-tauri/src/documents/commands/pdf.rs
git commit -m "feat(pdf): reject jobs from the v1 layout engine"
```

### Task 4: Build Deterministic Page Regions And Reading Order

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/pdf/layout.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/regions.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs:129-234`
- Modify: `docs/fixtures/pdf/generate_fixture.py`
- Regenerate: `docs/fixtures/pdf/technical-fixture.pdf`

**Interfaces:**

- Consumes: `detect_regions()` and existing paragraph/background/rule geometry
- Produces: stable sections, one-to-three columns, fixed panels, table cells, caption regions, image obstacles, and region reading order

- [ ] **Step 1: Add pure region tests**

Create tests with synthetic paragraph rectangles for:

```rust
#[test]
fn two_columns_read_down_left_then_down_right() { /* expect [0, 1, 2, 3] */ }

#[test]
fn full_width_heading_splits_column_sections() { /* heading occurs between sections */ }

#[test]
fn table_grid_creates_independent_cells() { /* no cell borrows another cell */ }

#[test]
fn caption_stays_with_horizontally_aligned_image() { /* RegionKind::Caption */ }

#[test]
fn page_sized_background_is_not_a_panel() { /* RegionKind::Body */ }

#[test]
fn marker_backed_paragraph_is_a_list_item() { /* BlockType::ListItem */ }
```

Use coordinates that make row-major order differ from the expected column-major order, so the column test cannot pass accidentally.

- [ ] **Step 2: Run the region tests and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::regions::tests`

Expected: FAIL because `detect_regions()` still creates one global body flow.

- [ ] **Step 3: Capture image rectangles from PDFium**

Extend `PageModel` with `images: Vec<Rect>` and `bounds: Rect`. In `read_page()`, collect visible image-object bounds and keep paths/backgrounds/rules behavior unchanged.

- [ ] **Step 4: Implement section and gutter detection**

Implement the Region And Reading-Order Policy exactly. Use body-font `em` units, require the 60% persistent-height gutter threshold, cap body lanes at three, and place uncertain paragraphs into a one-column fallback region with a diagnostic flag returned alongside `PageRegions`.

Classify a paragraph whose first line has `item_start` as `Kind::ListItem`, map it to `BlockType::ListItem`, and retain the marker as an untouched source object.

- [ ] **Step 5: Implement table and caption classification**

Use intersecting horizontal/vertical rule bounds to derive table cells. Assign a paragraph to one cell only when its center lies inside that cell. Detect captions only for short paragraphs immediately adjacent to an image and meeting the 60% horizontal-alignment threshold.

- [ ] **Step 6: Expand the authored fixture**

Update `generate_fixture.py` to create dedicated pages for two columns, a ruled table with a close heading, an image rectangle with a caption, a dark code panel, and a page-sized background. Keep the fixture project-authored and deterministic.

- [ ] **Step 7: Verify regions and existing classification**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: PASS. Existing code/panel/list-marker tests remain green.

- [ ] **Step 8: Commit structural analysis**

```bash
git add apps/desktop/src-tauri/src/documents/pdf docs/fixtures/pdf
git commit -m "feat(pdf): detect page regions and reading order"
```

### Task 5: Implement Flow-Level Vertical Reflow

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/pdf/reflow.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/layout.rs:483-621`

**Interfaces:**

- Consumes: `FlowItem`, `FlowFrame`, `Measure`, and existing scale steps
- Produces: `place_flow()` implementing the Reflow Policy and returning one `FlowPlan`

- [ ] **Step 1: Write failing reflow tests**

Add tests proving these exact behaviors:

```rust
#[test]
fn shorter_translation_keeps_preferred_gaps_at_top() { /* extra space remains below */ }

#[test]
fn longer_translation_moves_following_paragraph_before_shrinking() { /* scale == 1.0 */ }

#[test]
fn gaps_compress_before_shared_font_scale_changes() { /* min gaps first */ }

#[test]
fn compatible_body_items_share_scale_and_leading() { /* one scale and ratio */ }

#[test]
fn long_list_item_wraps_to_two_lines_instead_of_becoming_one_tiny_line() { /* two lines */ }

#[test]
fn heading_before_table_keeps_minimum_gap() { /* >= 0.5em */ }

#[test]
fn exhausted_frame_reports_block_and_page() { /* overflow_blocks contains id */ }
```

- [ ] **Step 2: Run the reflow tests and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::reflow::tests`

Expected: FAIL on gap redistribution, list wrapping, or shared-scale assertions.

- [ ] **Step 3: Implement natural wrapping and preferred heights**

For each `FlowItem`, wrap at scale `1.0`, compute line height from the shared median source pitch, and compute the preferred gap according to block type and the clamped source gap.

- [ ] **Step 4: Implement the ordered fallback solver**

Calculate frame capacity and apply these passes in order:

```text
1. preferred source-derived gaps
2. body gaps reduced to 0.35 line pitch; heading/table minima retained
3. shared leading reduced to max(1.08 * font size, required leading)
4. shared scale steps 0.94, 0.89, 0.85, 0.81, 0.78, 0.74, 0.70, 0.66, 0.62, 0.58, 0.55
5. emergency per-block placement plus overflow diagnostic
```

Do not include source line count as a fit condition. Fit against frame geometry and obstacles only.

- [ ] **Step 5: Preserve list indentation**

Store the list text left edge in the item frame. Keep the marker outside the frame and align every continuation line to the list text left edge.

- [ ] **Step 6: Run reflow and complete PDF tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::reflow::tests`

Expected: PASS.

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: PASS.

- [ ] **Step 7: Commit the pure solver**

```bash
git add apps/desktop/src-tauri/src/documents/pdf/layout.rs apps/desktop/src-tauri/src/documents/pdf/reflow.rs
git commit -m "feat(pdf): add flow-level vertical reflow"
```

### Task 6: Integrate Regions And Reflow Into PDF Export

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs:244-794`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/layout.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/regions.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/reflow.rs`

**Interfaces:**

- Consumes: `PageRegions`, `FlowPlan`, stored `DocumentBlock` translations
- Produces: one deterministic in-memory PDF model used by both `analyze()` and `export()`

- [ ] **Step 1: Write integration tests that expose independent fitting**

Extend the PDFium fixture with three vertically adjacent paragraphs and deterministic translations where the first grows, the second shrinks, and the third stays equal. Assert extracted translated text, no overlap between planned line bounds, stable body font size, and minimum gaps.

- [ ] **Step 2: Run the integration test and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::tests::flow_rebalances_adjacent_translations -- --exact`

Expected: FAIL because production export still calls `layout::place()` independently.

- [ ] **Step 3: Build one deterministic document model**

Read all selected pages before drawing. For each supported page, retain fragments, paragraphs, regions, backgrounds, rules, images, page bounds, and diagnostics. Build logical blocks from each page's `reading_order` rather than the old global y-sort order.

- [ ] **Step 4: Map stored translations to flow items**

Resolve each `LogicalBlock.id` once, construct page-local `FlowItem`s, and call `place_flow()` once per `FlowRegion`. Keep skipped paragraphs and marker objects out of removal lists.

- [ ] **Step 5: Replace independent placement and removal**

Delete `bottom_limit()`, `shift_room()`, `left_slack()`, the old `region()` helper, the `Job` struct, and both `place_all()` passes only after all call sites use `FlowPlan`. Remove original objects by logical-block ownership, deduplicate object indexes, remove in descending index order, then draw `PlannedLine`s.

- [ ] **Step 6: Produce precise diagnostics**

Replace the page-only shrink warning with messages containing page number, block ID, final scale, and reason (`shared scale below review threshold`, `unbreakable word`, `frame exhausted`, or `emergency individual shrink`). Keep the summary bounded when many blocks fail.

- [ ] **Step 7: Verify integration and replay real pages**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: PASS.

Replay Wahba pages 1-10 with Task 1's database path, render the output with `tools/pdf-feasibility/examples/render.rs`, and fill the `Observed after PDF v2` baseline cells for uneven gaps, lists, panels, and table spacing.

- [ ] **Step 8: Commit production reflow**

```bash
git add apps/desktop/src-tauri/src/documents/pdf docs/fixtures/pdf/README.md
git commit -m "feat(pdf): reflow translated page regions"
```

### Task 7: Join Conservative Cross-Page Paragraphs

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/pdf/regions.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/reflow.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs`
- Modify: `docs/fixtures/pdf/generate_fixture.py`
- Regenerate: `docs/fixtures/pdf/technical-fixture.pdf`

**Interfaces:**

- Consumes: selected page indexes, page regions, logical block assembly, `place_flow()`
- Produces: `fn join_cross_page_blocks(pages: &[PageAnalysis], selected: &[usize]) -> Vec<LogicalBlock>` and multi-frame translation slicing

- [ ] **Step 1: Add join-decision unit tests**

Cover all Cross-Page Join Policy conditions. Include negative tests for terminal punctuation, different columns, style mismatch, non-contiguous page selection, heading/body mismatch, and source text that is not near both page margins.

- [ ] **Step 2: Add translation-slicing tests**

Given one translation and two segment frames, assert that complete wrapped lines are allocated in order, no word is split, the first frame cannot consume more lines than fit, and remaining lines begin the second frame.

- [ ] **Step 3: Run cross-page tests and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::regions::tests::cross_page`

Expected: FAIL because pages always create independent blocks.

- [ ] **Step 4: Implement conservative joining**

Build logical blocks after every page has regions. Process only adjacent pages in each contiguous selected-page run. Preserve the first segment's ID and append source text using either a space or the validated dehyphenation rule.

- [ ] **Step 5: Implement multi-segment translation allocation**

Wrap once at the shared style/scale, allocate complete lines by segment frame capacity, and expose page-local slices to each flow. If all lines do not fit at the minimum scale, emit one diagnostic for the logical block and preserve all text at the emergency scale.

- [ ] **Step 6: Add a two-page authored fixture**

Generate one true continuation and one visually similar non-continuation. Assert analysis creates one block for the continuation and two blocks for the terminated pair.

- [ ] **Step 7: Run all PDF tests**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: PASS with stable block IDs across repeated analysis.

- [ ] **Step 8: Commit cross-page blocks**

```bash
git add apps/desktop/src-tauri/src/documents/pdf docs/fixtures/pdf
git commit -m "feat(pdf): join cross-page paragraph continuations"
```

### Task 8: Bundle Reproducible Noto Fonts And Preserve U+0020 Spaces

**Files:**

- Add: `apps/desktop/src-tauri/resources/fonts/NotoSans-Regular.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSans-Bold.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSans-Italic.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSans-BoldItalic.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSerif-Regular.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSerif-Bold.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSerif-Italic.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/NotoSerif-BoldItalic.ttf`
- Add: `apps/desktop/src-tauri/resources/fonts/OFL.txt`
- Add: `apps/desktop/src-tauri/resources/fonts/SOURCE.md`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/fonts.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs:734-755,841-899`

**Interfaces:**

- Consumes: Noto Sans v2.015 and Noto Serif v2.015 static TTF releases from `notofonts/latin-greek-cyrillic`, SIL OFL 1.1
- Produces: `FontSet::bundled() -> FontSet` backed by `include_bytes!()`, deterministic metrics, and non-CID TrueType embedding for Latin/Greek/Cyrillic text

- [ ] **Step 1: Add failing bundled-font tests**

Assert all eight styles parse with `ttf_parser`, contain Latin and Cyrillic glyphs (`A`, `Я`, `ё`), and return positive advances. Remove the current test skip that accepts missing Windows fonts.

- [ ] **Step 2: Add a strict extracted-space regression test**

Export `"Первое обычное предложение"`, extract the page text through PDFium, and assert it contains U+0020 and no U+00A0. Do not normalize extracted text in the test.

- [ ] **Step 3: Run tests and verify failure**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::fonts::tests`

Expected: FAIL because fonts still come from Windows.

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf::tests::exported_spaces_are_u0020 -- --exact`

Expected: FAIL because current CID font extraction maps spaces to U+00A0.

- [ ] **Step 4: Add and document official font assets**

Use static TTF files from official `NotoSans-v2.015` and `NotoSerif-v2.015` releases. Put the unmodified SIL OFL 1.1 text in `OFL.txt`. Record repository URL, release tags, original asset names, SHA-256 for each committed TTF, and retrieval date in `SOURCE.md`.

- [ ] **Step 5: Embed the font bytes at compile time**

Replace filesystem search with `include_bytes!("../../../resources/fonts/<file>.ttf")`. Store `&'static [u8]` in `FontSet`; remove `WINDIR`, `PathBuf`, Times New Roman, Arial, and missing-font errors.

- [ ] **Step 6: Load Latin/Greek/Cyrillic fonts as non-CID**

Change `load_true_type_from_bytes(bytes, true)` to `load_true_type_from_bytes(bytes, false)`. Noto Sans/Serif Latin-Greek-Cyrillic do not require the Asian CID path; PDFium then generates an ordinary Unicode mapping where spaces extract as U+0020.

- [ ] **Step 7: Verify fonts and text extraction**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: PASS without accessing `C:\Windows\Fonts`; extracted ordinary spaces are exactly U+0020.

- [ ] **Step 8: Commit bundled fonts**

```bash
git add apps/desktop/src-tauri/resources/fonts apps/desktop/src-tauri/src/documents/pdf
git commit -m "feat(pdf): bundle reproducible Noto fonts"
```

### Task 9: Complete Column, Table, Caption, And Obstacle Integration QA

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/pdf/mod.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/regions.rs`
- Modify: `apps/desktop/src-tauri/src/documents/pdf/reflow.rs`
- Modify: `docs/fixtures/pdf/README.md`
- Regenerate: `docs/fixtures/pdf/translated-fixture.pdf`
- Regenerate: `docs/fixtures/pdf/translated-fixture.pdf-pdfium-page-1.png`
- Regenerate: `docs/fixtures/pdf/translated-fixture.pdf-pdfium-page-2.png`

**Interfaces:**

- Consumes: complete `pdf-v2` region and flow pipeline
- Produces: structural integration coverage for every supported region type and visual evidence for authored fixtures

- [ ] **Step 1: Add end-to-end authored-fixture assertions**

Analyze the fixture twice and assert identical block IDs, source text, block types, and order. Verify `ListItem`, `TableCell`, and `Caption` block types are emitted where appropriate.

- [ ] **Step 2: Add geometric export assertions**

After deterministic translations, inspect output text-object bounds and assert each object stays inside its assigned column, table cell, panel, or caption frame. Assert no translated object intersects image bounds or table rules.

- [ ] **Step 3: Run integration tests and fix only demonstrated gaps**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf`

Expected: PASS. If a case fails, adjust region assignment or constraints without weakening another acceptance assertion.

- [ ] **Step 4: Render and inspect fixture pages**

Use the existing PDFium renderer:

```powershell
cargo run --manifest-path tools/pdf-feasibility/Cargo.toml --example render -- docs/fixtures/pdf/translated-fixture.pdf docs/fixtures/pdf/translated-fixture.pdf-pdfium-page 0 1
```

Expected: column order is visually correct, cells and captions are contained, code panels remain untouched, and heading/table spacing is visible.

- [ ] **Step 5: Update fixture documentation**

Record every authored page's purpose, expected reading order, expected unchanged objects, and the command used to regenerate source/output/render PNGs.

- [ ] **Step 6: Commit integration evidence**

```bash
git add apps/desktop/src-tauri/src/documents/pdf docs/fixtures/pdf
git commit -m "test(pdf): cover complex page regions end to end"
```

### Task 10: Bundle PDFium Notices And Font License Recursively

**Files:**

- Modify: `apps/desktop/src-tauri/tauri.conf.json:42`
- Add: `scripts/verify-desktop-bundle.ps1`
- Modify: `package.json`
- Modify: `STATUS.md`
- Modify: `TODO.md`

**Interfaces:**

- Consumes: Tauri 2 directory resource mapping, `resources/pdfium/`, `resources/fonts/OFL.txt`
- Produces: recursive resource mapping and `npm run desktop:verify-bundle`

- [ ] **Step 1: Write a failing bundle verification script**

The script accepts `-Root <installed-or-staged-resource-root>` and checks exact required paths:

```text
pdfium/pdfium.dll
pdfium/PDFIUM-LICENSE.txt
pdfium/licenses/abseil.txt
pdfium/licenses/agg23.txt
pdfium/licenses/fast_float.txt
pdfium/licenses/freetype.txt
pdfium/licenses/icu.txt
pdfium/licenses/lcms.txt
pdfium/licenses/libjpeg_turbo.ijg
pdfium/licenses/libjpeg_turbo.md
pdfium/licenses/libopenjpeg.txt
pdfium/licenses/libpng.txt
pdfium/licenses/llvm-libc.txt
pdfium/licenses/pdfium.txt
pdfium/licenses/simdutf.txt
pdfium/licenses/zlib.txt
fonts/OFL.txt
fonts/SOURCE.md
```

It exits non-zero and lists every missing file rather than stopping at the first one.

- [ ] **Step 2: Run verification against the current staged resources**

Run: `pwsh -File scripts/verify-desktop-bundle.ps1 -Root apps/desktop/src-tauri/resources`

Expected: PASS for source resources. Run against the current installer/staged bundle and confirm nested PDFium licenses are reported missing before changing Tauri config.

- [ ] **Step 3: Fix Tauri resource mapping**

Use directory mappings so subdirectories are preserved:

```json
"resources": {
  "resources/pdfium/": "pdfium/",
  "resources/fonts/OFL.txt": "fonts/OFL.txt",
  "resources/fonts/SOURCE.md": "fonts/SOURCE.md"
}
```

The TTF files are already embedded in the executable by `include_bytes!()` and do not need duplicate resource copies.

- [ ] **Step 4: Add the package script**

Add:

```json
"desktop:verify-bundle": "pwsh -File scripts/verify-desktop-bundle.ps1"
```

With no `-Root`, the script locates the Tauri NSIS staging/output tree after `npm run desktop:build` and validates the packaged resource set.

- [ ] **Step 5: Build and inspect the installer**

Run:

```powershell
pwsh -File scripts/fetch-pdfium.ps1
npm run desktop:build
npm run desktop:verify-bundle
```

Expected: all required resource paths are present and the NSIS installer is produced.

- [ ] **Step 6: Record license review results**

Update `STATUS.md` and `TODO.md` to state exactly which PDFium and Noto notices ship, where they are installed, and whether legal review found any additional attribution requirement. Do not mark clean-machine validation complete in this task.

- [ ] **Step 7: Commit packaging**

```bash
git add apps/desktop/src-tauri/tauri.conf.json scripts/verify-desktop-bundle.ps1 package.json STATUS.md TODO.md
git commit -m "build(pdf): bundle PDFium and font notices"
```

### Task 11: Add The User-Facing Stale-Job And Review Diagnostics Tests

**Files:**

- Modify: `apps/desktop/src/app/DocumentsPanel.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/app/documentProgress.ts`
- Modify: `apps/desktop/src/app/documentProgress.test.ts`

**Interfaces:**

- Consumes: native stale-parser errors and block-specific PDF diagnostics
- Produces: clear recovery text and bounded grouped review messages in the Files panel

- [ ] **Step 1: Write frontend tests**

Test that a native stale-job error is shown without rewriting its text and that PDF review diagnostics group repeated block warnings by page while retaining the affected block count.

- [ ] **Step 2: Run frontend tests and verify failure**

Run: `npm test -- --run apps/desktop/src/app/App.test.tsx apps/desktop/src/app/documentProgress.test.ts`

Expected: at least the new diagnostic grouping assertion FAILS.

- [ ] **Step 3: Implement bounded PDF diagnostic grouping**

Keep full native diagnostics in the job, but render one summary per page/reason and a total affected-block count. Preserve existing diagnostics for DOCX, EPUB, FB2, and TXT unchanged.

- [ ] **Step 4: Add an explicit re-analysis action for stale PDF jobs**

When a PDF action fails with the exact stale-layout message, show `Remove and add PDF again` using the existing remove/re-analyze path. Require user action; never silently delete translated data.

- [ ] **Step 5: Run frontend tests**

Run: `npm test -- --run apps/desktop/src/app/App.test.tsx apps/desktop/src/app/documentProgress.test.ts`

Expected: PASS.

- [ ] **Step 6: Commit Files panel behavior**

```bash
git add apps/desktop/src/app/DocumentsPanel.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/app/documentProgress.ts apps/desktop/src/app/documentProgress.test.ts
git commit -m "feat(pdf): explain stale jobs and layout warnings"
```

### Task 12: Run Real-Model And Clean-Machine Release Gates

**Files:**

- Modify after evidence is collected: `docs/fixtures/pdf/README.md`
- Modify after evidence is collected: `STATUS.md`
- Modify after evidence is collected: `TODO.md`
- Modify only after all gates pass: root and workspace version metadata listed in `AGENTS.md`

**Interfaces:**

- Consumes: completed PDF v2 implementation, newest NSIS installer, real local model, Windows Sandbox or clean VM
- Produces: recorded QA evidence and release-ready version metadata

- [ ] **Step 1: Run focused Rust quality gates**

Run:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::pdf
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: all commands exit zero.

- [ ] **Step 2: Run the complete repository gate**

Run: `npm run check`

Expected: formatting, desktop lint/typecheck/tests, extension typecheck/tests, Rust format, and Rust tests all exit zero.

- [ ] **Step 3: Build and verify the installer**

Quit `lingvoloc.exe`, then run:

```powershell
pwsh -File scripts/fetch-pdfium.ps1
npm run desktop:build
npm run desktop:verify-bundle
```

Expected: NSIS installer builds and contains all required files.

- [ ] **Step 4: Perform real-model PDF QA**

Install the newest build, remove all old PDF jobs, add Wahba pages `1-10`, translate with the real model, and inspect rendered output. Repeat on the authored two-column/table/caption fixture and at least three additional legally usable text PDFs with different producers.

Record per document: source producer, selected pages, model, source/target languages, block count, diagnostics, review pages, overlaps, spacing defects, column order, table containment, caption placement, copied-space code points, and pass/fail.

- [ ] **Step 5: Perform clean-machine installer QA**

In Windows Sandbox or a clean VM, install only the generated NSIS package. Confirm PDF analysis/export works without development files and without relying on separately installed Times New Roman or Arial. Confirm the installed resource directory includes PDFium DLL/notices and Noto notices.

- [ ] **Step 6: Update project status from evidence**

Mark only observed checks complete. Keep scans, rotated pages, raster text, and justification documented as limitations. Record any remaining review pages with screenshots or page numbers.

- [ ] **Step 7: Bump the release version after every PDF gate passes**

Update together: root `package.json`, desktop `package.json`, extension `package.json`, extension `manifest.json`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/Cargo.toml`, and generated lock metadata. Do not choose or publish the version before QA passes.

- [ ] **Step 8: Re-run release gates after the version bump**

Run:

```powershell
npm run check
npm run extension:package
npm run desktop:build
npm run desktop:verify-bundle
```

Expected: all commands exit zero and artifact names contain the same new version.

- [ ] **Step 9: Commit the verified release preparation**

```bash
git add package.json package-lock.json apps/desktop/package.json apps/extension/package.json apps/extension/manifest.json apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock docs/fixtures/pdf/README.md STATUS.md TODO.md
git commit -m "release: prepare verified PDF v2 build"
```

## Manual Review Checklist

- [ ] Source and translated pages have the same page count for full export.
- [ ] Partial export contains exactly the selected pages and keeps original block page numbers.
- [ ] No translated text overlaps another translated block.
- [ ] No translated text crosses an image, panel, table rule, underline, or column gutter.
- [ ] Body font size is visually consistent within each flow.
- [ ] Paragraph gaps remain natural when translations grow or shrink.
- [ ] Long list items wrap normally and do not collide with markers.
- [ ] Headings retain visible space before tables and panels.
- [ ] Multi-column reading order is correct in analysis and copied text.
- [ ] Cross-page paragraphs are joined only when visually continuous.
- [ ] Code, page numbers, labels, graphics, backgrounds, and annotations remain unchanged.
- [ ] Copied ordinary spaces are U+0020 rather than U+00A0.
- [ ] Review diagnostics identify exact pages and affected block counts.
- [ ] The installed app exports PDF on a clean machine.
- [ ] PDFium and Noto license files are present in the installation.

## Explicitly Deferred Work

- OCR and translation of scanned/image-only pages.
- Translation of text drawn inside raster images.
- Rotation-aware editing of rotated pages.
- Full typographic justification and source-font substitution beyond serif/sans, bold, and italic.
- Arbitrary movement of ordinary content between pages.
- Phase 9 reading-oriented EPUB or DOCX export from PDF blocks. Start it as a separate design and implementation plan only after PDF v2 passes the release gates above.
