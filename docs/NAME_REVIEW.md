# Name review: "LingvoLoc"

Preliminary screening done on 2026-10-01 with web search, registry (RDAP) lookups and package-registry queries. It is **not legal advice and not a formal trademark clearance**: the official trademark registers (USPTO, EUIPO, WIPO Global Brand Database, Rospatent) are interactive sites that could not be queried from this environment, so they still have to be searched by a person or a trademark attorney.

Status: closed by the project owner on 2026-10-01; the official registers were not searched.

## Result

| Check                           | Result                                                                                                                                                                         |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Exact name in web search        | No product, company or trademark called "LingvoLoc" or "Lingvo Loc" found.                                                                                                     |
| GitHub                          | Only this repository; no user or organisation `lingvoloc`. The earlier name `LingoLoc` was dropped; unrelated `lingolocal`/`Lingoloco` repositories exist.                     |
| Package names                   | `lingvoloc` is free on npm, crates.io and PyPI (HTTP 404 on 2026-10-01).                                                                                                       |
| Domains (RDAP, not registered)  | `lingvoloc.com`, `lingvoloc.net`, `lingvoloc.app`, `lingvoloc.dev`. `.org`, `.io`, `.ai`, `.ru` could not be checked (registry timeout).                                       |
| Similar marks in the same field | **"Lingvo"** is a trademark of ABBYY (ABBYY Lingvo dictionaries, Lingvo Live, Lingvo API). **LingvoSoft** is a trademark of ECTACO Inc. (translation and dictionary software). |

## Assessment

- The exact name is unused and the technical handles are free, so there is no hard blocker found.
- The main risk is likeness to **ABBYY Lingvo**: LingvoLoc starts with the same distinctive word, is used for the same kind of goods (translation and dictionary software) and, with StarDict dictionary lookup, serves the same users, including Russian speakers where ABBYY Lingvo is very well known. A confusion or "association" claim cannot be ruled out; the "Loc" ending and the different logo reduce but do not remove it.
- The risk is higher for registering or commercially promoting the name than for a free open-source tool, but the repository is now public and the product is distributed under this name.

## Recommended next steps

1. Search the official registers for `LINGVOLOC`, `LINGVO` and `LINGVO*` in classes 9, 42 and 41: USPTO (https://tmsearch.uspto.gov), EUIPO TMview (https://www.tmdn.org/tmview), WIPO Global Brand Database (https://branddb.wipo.int), Rospatent (https://www1.fips.ru).
2. If ABBYY (or ECTACO) holds an active registration for "Lingvo" in these classes, either talk to a trademark attorney or rename the product while it is still young. A rename touches the user-visible name only; keep the legacy identifiers (`com.lingoloc.desktop`, `lingoloc.sqlite`, `lingoloc.settings`).
3. Independently of the outcome, consider registering `lingvoloc.com` (and `.app`) to hold the name, and do not describe the product as affiliated with ABBYY Lingvo.
4. Record the decision in the project documentation.
