# PDF Feasibility Phase 1 Handoff

## Scope and status

This report records the Phase 1 PDF feasibility investigation for LingvoLoc.
The repository has no PDF CLI or renderer installed. No production PDF
dependency was added, and no library has been selected for production use.

## Verified fixture and extraction results

- A self-authored two-page fixture was generated at
  `docs/fixtures/pdf/technical-fixture.pdf` by
  `docs/fixtures/pdf/generate_fixture.py` using `pypdf`.
- The fixture contains headings, a repeated footer, two text columns, a table,
  formula-like text, a URL, identifier `ADC-02`, vector paths, and selectable
  Cyrillic text using an embedded Arial font.
- `pypdf` extracted two pages and the text from page 1, but decoded the
  embedded Cyrillic glyphs incorrectly in its text extractor.
- The isolated Rust utility at `tools/pdf-feasibility` uses `pdf-extract`
  `0.12.1`. It extracted both pages, 478 characters per page, including
  readable Cyrillic text, with correct page order.
- The official `bblanchon/pdfium-binaries` Windows x64 archive for release
  `chromium/8076` was downloaded into the ignored spike runtime folder. Its
  SHA-256 is
  `808d36da9bc5a3104315fb307c80998121f565ee53953633bf33e80d7429e5ac`.
- With that DLL, `pdfium-render` `0.9.4` loaded the document, reported 463 text
  characters per page, measured bounds `(42.0, 216.75, 630.0, 779.664)`, and
  rendered both pages to 1200x1698 PNG images. The images are ignored generated
  outputs, not application assets.
- `docs/fixtures/pdf/translated-fixture.pdf` replaces the text operators with
  longer Cyrillic strings while preserving the vector paths and page structure.
  PDFium reported bounds `(42.0, 216.75, 760.0, 779.664)`, beyond the page
  width of 595 points. The translated render also produced a non-empty pixel
  difference from the original in `(84, 352, 1200, 1168)` at 1200x1698.

These results establish extraction, basic text geometry, rendering, and a
controlled text-replacement experiment for a self-authored text PDF with
vector content. The experiment intentionally demonstrates an overflow; it is
not a complete arbitrary-PDF reconstruction algorithm. `pypdf` still decodes
the embedded Cyrillic glyphs incorrectly, while `pdf-extract` and PDFium return
readable Cyrillic text.

## Candidate comparison

| Candidate       | Assessment                                                                                                                                                                  | Source                                      |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| `pdfium-render` | Primary technical candidate. PDFium exposes rendering, text geometry, page objects, and graphics. It requires a packaged `pdfium.dll` and a separate license/update review. | <https://github.com/ajrcarey/pdfium-render> |
| `pdf-extract`   | MIT and pure Rust; useful extraction baseline, but not a complete renderer or reconstructor.                                                                                | <https://github.com/jrmuizel/pdf-extract>   |
| `lopdf`         | MIT and pure Rust; useful for low-level PDF object manipulation, but not a layout engine.                                                                                   | <https://github.com/J-F-Liu/lopdf>          |
| MuPDF           | Technically strong extraction and rendering option, but AGPL/commercial licensing is unsuitable for the current MIT app.                                                    | <https://mupdf.com/>                        |

## Remaining acceptance work

Phase 1 is accepted for the documented initial class: PDFs with extractable
text, embedded fonts, simple text regions, and vector graphics. It is not a
claim of unrestricted PDF support. The downloaded distribution is MIT-licensed,
but PDFium and third-party notices still require review before redistribution.
Raster text, complex tables/formulas, arbitrary reading order, and robust
translated-text replacement remain production limitations. Unsupported content
and visual differences must be reported; perfect layout preservation is not an
acceptance assumption.

## Exact next action

Proceed to Phase 2 with the PDFium candidate kept isolated. Build the
document-job state machine and recovery model before integrating any PDF
format processing into the desktop runtime. Keep the PDFium/third-party notice
review as a release prerequisite.
