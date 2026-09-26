# Phase 1 Integration Spike

## LM Studio

Verified on 2026-09-26 against `http://127.0.0.1:1234/v1`.

- `GET /v1/models` returned HTTP 200 with seven model records.
- The installed TranslateGemma IDs include `translategemma-4b-it@q8_0`, `translategemma-4b-it@f16`, and `translategemma-12b-it`.
- A chat request with a string `messages[].content` returned HTTP 200.
- A chat request with an object `messages[].content` containing `source_lang_code`, `target_lang_code`, and `text` returned HTTP 400: LM Studio requires a string or an array of content objects.
- An array of text content parts returned HTTP 200, but the adapter uses a plain string because it is the smallest accepted payload.
- `temperature: 0`, `max_tokens: 64`, and `stream: false` were accepted.
- `translategemma-4b-it@q8_0` returned `Bonjour, monde !` for English to French.
- English to Russian returned the correct Unicode codepoints `Привет, мир!`; an earlier PowerShell console display made the output look corrupted. The raw HTTP body is valid UTF-8.

The Phase 1 adapter therefore emits an explicit text prompt containing source and target language codes and expects the OpenAI-compatible completion response at `choices[0].message.content`.

Requests and errors are summarized in the runtime; complete user text is not logged.
