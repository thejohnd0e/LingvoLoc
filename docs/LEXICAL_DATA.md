# Lexical Data

The lexical service combines a small project-authored seed lexicon and a generated FreeDict bilingual index.

- Provenance: original project-authored example entries.
- License: covered by this repository's MIT license.
- Scope: lemma lookup, supported inflected forms, part of speech, definitions, translations, synonyms, antonyms, and related words.
- Current priority languages: Russian (`ru`) and English (`en`), with bidirectional FreeDict coverage.
- FreeDict notice: `apps/desktop/src-tauri/data/lexical/FREEDICT-NOTICE.txt`.

The project-authored seed contains the cross-language translations and curated fields. Lookup does not display model-generated or partial thesaurus records; a missing local record is reported as not found rather than presented as a dictionary fact.

## Bundled FreeDict Dataset

- Sources: FreeDict+WikDict `eng-rus` and `rus-eng` StarDict archives, version `2025.11.23`.
- Coverage: 62,181 English-to-Russian and 42,600 Russian-to-English source entries, merged into one generated JSON index.
- License: CC BY-SA 3.0 for the source dictionary data; the upstream COPYING and attribution notice are bundled with the generated index.
- Distribution format: a generated compact index containing only the fields used by LingvoLoc.
- Russian-first behavior: `rus-eng` records retain the `FreeDict RUS-ENG` provider label and are tested independently from English-to-Russian records.
- User dictionaries: extracted StarDict sets are loaded from the per-user `lexical/user` directory. The UI rescans folders on `Refresh dictionaries` and persists checkbox selection locally. The runtime supports `.ifo`, `.idx`/`.idx.gz`, and `.dict`/`.dict.dz`; user records are labeled with the StarDict `bookname` and use language `und` unless a future dictionary metadata mapping provides a language. Referenced audio and image files are loaded on demand from loose files or `res.zip`, avoiding media I/O while scanning the dictionary.
- Raw source policy: do not commit the upstream StarDict archives; keep the generated index and reproducible conversion command.
- Converter coverage: FreeDict StarDict pairs are language-independent; Kaikki/Wiktionary JSONL and morphology TSV accept an explicit source language code and preserve language-specific identities.
- Bundling policy: new language pairs require their own source notices and a regenerated index review before being bundled into the desktop binary.
- Future source: OpenCorpora-derived morphology and Kaikki/Wiktionary records remain candidates for additional forms and definitions in a separately reviewed multilingual index.
- RuWordNet: deferred until the maintainers confirm redistribution terms for the XML data in writing.

The generated index must preserve source attribution, the applicable Creative Commons license links, and a notice that LingvoLoc's transformations are not the upstream sources' original data.
