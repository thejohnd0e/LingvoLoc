# PDF Feasibility Spike

This isolated utility compares a pure-Rust extraction baseline against the
fixture in `docs/fixtures/pdf/technical-fixture.pdf`. It is not a production
dependency and is not used by the desktop application.

For the current Windows x64 run, `pdfium.dll` is downloaded from the official
`bblanchon/pdfium-binaries` release `chromium/8076` into the ignored `bin/`
folder. The archive SHA-256 is
`808d36da9bc5a3104315fb307c80998121f565ee53953633bf33e80d7429e5ac`.
The distribution repository is MIT-licensed; review PDFium and third-party
notices before shipping the DLL in the application.

Run it from the repository root with:

```text
cargo run --manifest-path tools/pdf-feasibility/Cargo.toml -- docs/fixtures/pdf/technical-fixture.pdf
```

The spike intentionally reports extraction only. A renderer and a PDFium
binary are still required before judging layout-preserving reconstruction.
