# PDF Fixture Dependency

`technical-fixture.pdf` is a small self-authored extraction baseline generated
by `generate_fixture.py`. The environment still has no local PDF renderer, and
no user or publisher document may be copied into the repository.

The fixture now embeds `C:/Windows/Fonts/arial.ttf` and includes selectable
Cyrillic text. The PDFium DLL used for the isolated spike is not an application
runtime dependency and is intentionally ignored by git.

`translated-fixture.pdf` is the controlled reconstruction variant. It replaces
the text content with longer Cyrillic strings while retaining the vector paths
and page structure. It intentionally overflows the original text regions so
the spike can verify diagnostics without hiding the failure.

Phase 1 requires a redistribution-permitted, self-authored or explicitly
provided sample containing, on a small number of pages:

- selectable Unicode text and Cyrillic text;
- two columns and repeated header/footer material;
- a heading, caption, table, formula, code-like identifier, and hyperlink;
- raster and vector illustrations;
- at least one rotated or cropped page if available.

The sample must be accompanied by its license or permission and a short
expected-output note. If it cannot be provided, Phase 1 must record that PDF
reconstruction remains unvalidated rather than selecting a production engine.
