# File Translation Fixtures

These are small self-authored fixtures for the file-translation work. They do
not contain user books or copied publisher content.

## Contents

- `txt/mixed.txt`: UTF-8 text with a BOM-sensitive heading, paragraphs, a URL,
  an identifier, and a blank-line boundary.
- `docx/`: the source tree for a minimal DOCX package. It exercises headings,
  mixed formatting, a hyperlink, a table, and an image relationship. The
  package is intentionally kept unpacked until the DOCX phase defines its
  fixture builder.
- `epub/`: a minimal EPUB source tree with a spine, navigation, a note link,
  and an image resource.
- `fb2/sample.fb2`: a small FB2 book with sections, an epigraph, a note, and
  an embedded-resource reference.
- `pdf/README.md`: the PDF fixture status and the required page features.
- `pdf/translated-fixture.pdf`: a controlled replacement experiment with
  longer Cyrillic text and preserved vector content.

The fixtures are deliberately small enough for parser tests and visual
inspection. They should be copied into temporary archives or files by tests;
the original fixtures must remain unchanged.
