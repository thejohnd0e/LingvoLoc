# Local Language Detection

LingvoLoc supports local source-language detection through the `whatlang` Rust library. Automatic detection is enabled by default and runs before translation; the translation model is never used as a detector.

Supported results are English (`en`), Russian (`ru`), German (`de`), Spanish (`es`), French (`fr`), Italian (`it`), Portuguese (`pt`), Polish (`pl`), Ukrainian (`uk`), Chinese (`zh`), Korean (`ko`), and Thai (`th`). Unsupported languages and empty input return a normalized input error. Manual language selection remains available for the same set.

The two independent pair selectors default to English and Russian. When detection is enabled, input in one selected language is translated to the other. If the detected language is outside the selected pair, the first selected language is used as the target.
