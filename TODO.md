# TODO

## Next

- [ ] Manually verify on real machines: Download/Update llama.cpp on AMD/Intel and no-GPU setups (NVIDIA was confirmed); Start with Windows toggle; Add to PATH; installer on a machine without WebView2.
- [ ] Optionally remove the `Start with Windows` registry value from an NSIS uninstall hook and refresh its path when the executable moves.
- [ ] Optionally allow resizing the extension overlay from its left and top edges.

- [ ] Consider a Windows job object so an abnormal app crash cannot orphan `llama-server` (normal exit and tray Quit already stop it).

- [x] Map source language for user-selected StarDict dictionaries from `bookname`/file name (`rus-eng`, `Russian-English`, ...); target language is not mapped yet.
- [ ] Improve StarDict record parsing and sense separation so common Russian-English dictionaries yield stable definitions, translations, forms, examples, synonyms, and antonyms where the source data provides them.
- [ ] Add regression tests (truncated index and language mapping done) for malformed indexes, dictionary type/markup variants, language metadata, and multi-sense records before changing the dictionary UI contract.

## Models

- [ ] Verify the Hunyuan-MT and Qwen adapters end to end in the desktop UI (only benchmark-script and unit-test coverage so far).
- [ ] Optionally translate long text paragraph by paragraph (split on blank lines in `services/translation.rs`, join the results) so weaker models cannot skip trailing paragraphs.
- [ ] Optionally show tokens/s in the extension popup (`apps/extension/src/popup.ts`); `completion_tokens` is already in the `TranslationResult` JSON returned by the loopback API.

## Before Public Release

- [ ] Complete formal trademark and domain review for the `LingvoLoc` name.
