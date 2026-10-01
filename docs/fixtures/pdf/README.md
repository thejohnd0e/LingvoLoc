# PDF Fixture Dependency

`technical-fixture.pdf` is a small self-authored extraction baseline generated
by `generate_fixture.py`. The environment still has no local PDF renderer, and
no user or publisher document may be copied into the repository.

The fixture embeds the repository's bundled Noto Sans font and includes
selectable Cyrillic text. The PDFium DLL used for the isolated spike is not an
application runtime dependency and is intentionally ignored by git.

`translated-fixture.pdf` is the controlled reconstruction variant. It replaces
the text content with longer Cyrillic strings while retaining the vector paths
and page structure. It is used to verify reading order, captions, obstacles,
table-cell classification, and review diagnostics.

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

## PDF v2 Review Matrix

Use the release `pdf_try` example to replay real translations from a completed
job. Set `PDF_JOB_DB` to `%APPDATA%\com.lingoloc.desktop\document-jobs.sqlite`
and `PDF_JOB_ID` to the job id; `PDF_TRANSLATIONS` remains the higher-priority
JSON override for deterministic fixtures. Keep outputs and rendered pages
outside the repository because the Wahba source is user-provided copyrighted
content.

| Case                    | Source                                     | Pages         | Expected                                                                           | Observed before PDF v2                                 | Observed after PDF v2 |
| ----------------------- | ------------------------------------------ | ------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------ | --------------------- |
| Uneven body gaps        | Wahba book                                 | 1-10          | Natural paragraph spacing remains after translations grow or shrink                | Independent paragraph fitting leaves inconsistent gaps |                       |
| Tight long list item    | Wahba book                                 | 1-10          | List item receives a second line and following items move down                     | A tight item can remain a tiny one-line block          |                       |
| Page-break continuation | Wahba book                                 | 1-10          | A visually continuous paragraph becomes one translation block with two page frames | Continuation is stored as two blocks                   |                       |
| Multiple columns        | Authored fixture or external permitted PDF | fixture pages | Reading order is column-by-column                                                  | Single-column analysis interleaves columns             |                       |
| Heading before table    | Authored fixture or external permitted PDF | fixture pages | Heading retains visible space before table                                         | Local fitting can remove the gap                       |                       |
| Panels and code         | Authored fixture                           | fixture pages | Code and panel geometry remain unchanged                                           | Existing behavior is the regression baseline           |                       |

The authored fixture has two pages. Each page contains a full-width heading
and footer, two body columns, a vector illustration with a caption, a shaded
code-like panel, a ruled table, and an underlined URL. The expected reading
order is heading, left column, right column, caption, panel/table content, URL,
and footer; code-like content and the page furniture remain unchanged.

Regenerate the source and translated fixtures with:

```powershell
python docs/fixtures/pdf/generate_fixture.py
```

Render translated pages when PDFium is available with the existing
`tools/pdf-feasibility` render example. Rendered PNGs are review artifacts and
must not be regenerated from user-provided books.
