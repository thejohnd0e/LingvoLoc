# LocalTranslate — Technical Specification

## 1. Project overview

LocalTranslate is a Windows-first local desktop translation application designed as an offline/private alternative to cloud translation services.

The application must support:

- local translation models, including different quantization levels;
- GPU acceleration, especially NVIDIA CUDA;
- model discovery, selection, loading and benchmarking;
- translation of words, phrases, sentences and larger text blocks;
- automatic language detection;
- lexical information such as lemmas, parts of speech, inflections, synonyms, antonyms, related words, definitions and examples;
- context-sensitive alternative translations;
- translation history;
- global hotkeys and clipboard integration;
- a browser extension that communicates with the desktop app;
- an architecture that is not tied to one model or one runtime.

The first supported translation model family is TranslateGemma, but the application must be designed to support additional translation models and general-purpose LLMs later.

---

## 2. Core product goals

### 2.1 Primary goals

1. Provide fast local translation with no mandatory cloud dependency.
2. Allow users to choose among multiple installed local models and quantizations.
3. Use GPU acceleration automatically when available.
4. Provide a DeepL-like translation workflow while adding local model control.
5. Provide dictionary and thesaurus functionality as a first-class feature.
6. Integrate with the browser through a custom extension.
7. Keep desktop app, lexical engine and model runtime modular.

### 2.2 Non-goals for MVP

Do not include these in the first implementation unless required by the architecture:

- OCR;
- PDF translation;
- DOCX document translation;
- speech-to-text;
- text-to-speech;
- cloud account synchronization;
- user accounts;
- mobile apps;
- browser-side inference;
- automatic model downloading from arbitrary remote repositories;
- advanced CAT-tool functionality.

---

## 3. Target platform

### 3.1 Primary platform

- Windows 10/11 x64.
- NVIDIA GPUs should receive first-class support.
- Initial reference GPU: NVIDIA GeForce RTX 3060 12 GB.

### 3.2 Future compatibility

Architecture should not prevent later support for:

- AMD GPUs;
- Intel GPUs;
- Linux;
- macOS.

---

## 4. Recommended technology stack

### 4.1 Desktop application

Recommended:

- Tauri 2
- React
- TypeScript
- Vite
- Rust backend

Rust should handle native desktop functionality, runtime/process integration, filesystem scanning, hardware detection, local API server, global shortcuts, clipboard integration and system tray behavior.

React/TypeScript should handle the UI.

### 4.2 Local persistence

Use SQLite.

Recommended usage:

- translation history;
- favorites;
- application settings;
- benchmark results;
- model metadata cache;
- lexical cache where useful.

### 4.3 Browser extension

Use:

- Chromium Manifest V3;
- TypeScript;
- browser content scripts;
- background/service worker;
- popup/selection UI.

Primary browsers:

- Chrome;
- Chromium;
- Vivaldi;
- Edge.

---

## 5. High-level architecture

```text
Browser Extension
       │
       │ localhost API / Native Messaging
       ▼
LocalTranslate Desktop
       │
       ├── UI Layer
       │
       ├── Translation Service
       │     ├── Translation Model Adapter
       │     ├── Language Detection
       │     └── Result Normalization
       │
       ├── Lexical Engine
       │     ├── Lemmatizer
       │     ├── Morphology
       │     ├── Dictionary Database
       │     ├── Synonyms
       │     ├── Antonyms
       │     ├── Related Words
       │     └── Examples
       │
       ├── Model Manager
       │     ├── Model Discovery
       │     ├── Metadata
       │     ├── Quantization Detection
       │     └── Model Selection
       │
       ├── Runtime Manager
       │     ├── LM Studio Runtime
       │     └── llama.cpp Runtime
       │
       ├── Hardware Manager
       │     ├── GPU Detection
       │     ├── CUDA Detection
       │     ├── VRAM Detection
       │     └── Offload Strategy
       │
       ├── History Service
       │
       └── Local Integration API
```

---

## 6. Repository structure

Recommended monorepo structure:

```text
/apps
  /desktop
  /extension

/packages
  /core
  /protocol
  /model-adapters
  /lexical
  /shared-ui

/native
  /runtime
  /hardware
```

Suggested responsibilities:

### `/apps/desktop`

Desktop GUI and Tauri integration.

### `/apps/extension`

Browser extension.

### `/packages/core`

Shared application types, translation interfaces, service contracts and business logic.

### `/packages/protocol`

Shared request/response schemas for desktop ↔ extension communication.

### `/packages/model-adapters`

Translation model-specific adapters.

### `/packages/lexical`

Lexical data interfaces and language-specific analyzers.

---

## 7. Translation engine

The UI must never directly depend on a specific model.

Define a normalized request type similar to:

```ts
export type TranslationRequest = {
  text: string;
  sourceLanguage: string | 'auto';
  targetLanguage: string;
  mode?: 'natural' | 'literal' | 'formal' | 'casual';
  modelId?: string;
};
```

Normalized result:

```ts
export type TranslationResult = {
  text: string;
  detectedSourceLanguage?: string;
  sourceLanguage: string;
  targetLanguage: string;
  modelId: string;
  latencyMs: number;
  promptTokens?: number;
  outputTokens?: number;
};
```

The translation service should expose something equivalent to:

```ts
interface TranslationService {
  translate(request: TranslationRequest): Promise<TranslationResult>;
}
```

---

## 8. Model adapter abstraction

Do not hardcode model-specific prompt logic into the translation service.

Define an adapter interface similar to:

```ts
interface TranslationModelAdapter {
  id: string;

  supports(model: LocalModel): boolean;

  buildRequest(
    model: LocalModel,
    request: TranslationRequest,
  ): RuntimeGenerationRequest;

  parseResponse(response: RuntimeGenerationResponse): TranslationResult;
}
```

Initial adapters:

1. `TranslateGemmaAdapter`
2. `GenericLLMTranslationAdapter`

Potential future adapters:

- NLLB;
- MADLAD;
- Qwen-based translators;
- other specialized translation models.

Avoid code such as:

```ts
if (modelName.includes('gemma')) {
}
```

Model support should be implemented through explicit adapters and metadata.

---

## 9. TranslateGemma integration

TranslateGemma is the initial reference model family.

The adapter must support the input structure expected by the model, including explicit source and target language information.

Conceptually:

```text
source_lang_code: ru
target_lang_code: en
text: ...
```

The exact prompt/template must be isolated inside `TranslateGemmaAdapter` and must not leak into UI code.

The result parser must normalize model output and remove unwanted artifacts where necessary, such as:

- `Translation:` prefixes;
- surrounding quotation marks;
- markdown wrappers;
- explanatory text if not requested.

Default translation generation should be deterministic or near-deterministic.

Recommended defaults:

```text
temperature: 0 or minimal supported value
```

---

## 10. Runtime abstraction

Translation model and runtime are separate concepts.

Define a runtime contract such as:

```ts
interface ModelRuntime {
  id: string;

  listModels(): Promise<LocalModel[]>;
  loadModel(modelId: string, options?: LoadOptions): Promise<void>;
  unloadModel(modelId?: string): Promise<void>;
  generate(
    request: RuntimeGenerationRequest,
  ): Promise<RuntimeGenerationResponse>;
  getStatus(): Promise<RuntimeStatus>;
}
```

Initial runtimes:

### 10.1 LM Studio Runtime

Use the OpenAI-compatible local API.

Default endpoint:

```text
http://127.0.0.1:1234/v1
```

The application should:

- check whether the endpoint is reachable;
- request available models;
- allow the user to choose among available models;
- send generation requests;
- show useful connection errors.

The application must not assume a specific model ID.

### 10.2 llama.cpp Runtime

Planned as the standalone/native runtime.

Requirements:

- GGUF support;
- CUDA-enabled Windows builds;
- CPU fallback;
- configurable context size;
- configurable GPU offload;
- model load/unload;
- runtime health/status reporting.

The architecture must allow llama.cpp to eventually become the default built-in runtime without requiring major changes to the UI or translation layer.

---

## 11. Model Manager

The Model Manager is responsible for discovering and describing local models.

Model representation should be similar to:

```ts
export type LocalModel = {
  id: string;
  name: string;
  family?: string;
  path?: string;
  format?: 'gguf' | 'api' | 'unknown';
  quantization?: string;
  sizeBytes?: number;
  contextLength?: number;
  runtimeId: string;
  available: boolean;
  loaded?: boolean;
  capabilities: {
    translation: boolean;
    languageDetection?: boolean;
  };
};
```

### 11.1 Model discovery

Support two discovery mechanisms.

#### LM Studio

Query runtime/API for available model IDs.

#### Local filesystem

Allow the user to register one or more model directories, for example:

```text
D:\AI\Models
```

Scan recursively or one level deep for:

```text
*.gguf
```

Do not assume quantization only from filename if GGUF metadata can provide it.

### 11.2 Quantization awareness

The UI must treat quantization as model metadata.

Examples:

```text
TranslateGemma 4B F16
TranslateGemma 4B Q8_0
TranslateGemma 4B Q6_K
TranslateGemma 4B Q5_K_M
TranslateGemma 4B Q4_K_M
```

Group model variants by family/model where practical.

### 11.3 Model status

Display states such as:

```text
Installed
Loaded
Unavailable
Loading
Error
```

---

## 12. GPU and hardware support

GPU acceleration must be a first-class feature.

### 12.1 NVIDIA support

Primary implementation target:

- CUDA;
- RTX 20/30/40/50 series where runtime compatibility allows.

Reference hardware:

```text
NVIDIA GeForce RTX 3060
VRAM: 12 GB
```

### 12.2 Hardware Manager

Hardware Manager should detect where possible:

- GPU name;
- GPU vendor;
- total VRAM;
- free VRAM;
- CUDA availability;
- available runtime backends;
- system RAM;
- CPU logical cores.

Expose normalized hardware information to the runtime manager.

### 12.3 Compute modes

UI options:

```text
Device
  Auto
  NVIDIA GeForce RTX 3060
  CPU

GPU acceleration
  Enabled / Disabled

GPU offload
  Auto
  Full
  Custom
```

Default should be `Auto`.

### 12.4 Automatic GPU offload

For llama.cpp runtime:

1. estimate whether the selected model can fit in VRAM;
2. prefer full GPU offload when practical;
3. use partial GPU offload if necessary;
4. retry with reduced GPU layers after CUDA OOM where possible;
5. offer CPU fallback if the model cannot be loaded on the GPU.

Never silently fall back without notifying the user.

Example status message:

```text
Model could not fit entirely in VRAM.
Running with partial GPU acceleration.
```

### 12.5 Model memory estimate

Before loading a local GGUF model, show where possible:

```text
Model size
Estimated VRAM use
Available VRAM
Expected mode: Full GPU / Partial GPU / CPU
```

These are estimates and should be labeled accordingly.

---

## 13. Model benchmarking

Add optional benchmark functionality.

The benchmark should measure at least:

- model load time;
- prompt processing speed;
- generation speed;
- peak or approximate VRAM use;
- runtime/backend;
- benchmark timestamp.

Example:

```text
TranslateGemma 4B Q8_0
Backend: CUDA
Load time: 2.4 s
Prompt speed: 330 tok/s
Generation: 73 tok/s
VRAM: 6.1 GB
```

Persist benchmark results locally.

Do not use benchmark results to automatically claim translation quality.

---

## 14. Language detection

Do not use the translation LLM for basic language detection unless required as fallback.

Use a lightweight local detector such as a Lingua-compatible implementation or another reliable offline detector.

Requirements:

- fast local detection;
- confidence score where available;
- configurable fallback;
- remember previous source language for ambiguous very short inputs.

For short strings like:

```text
OK
Chat
No
```

return `unknown` when confidence is insufficient rather than guessing aggressively.

---

## 15. Main translation UI

The primary desktop screen should use a two-panel layout.

Concept:

```text
┌──────────────────────────────────────────────────────────────┐
│ Source language        ⇄        Target language             │
├─────────────────────────────┬────────────────────────────────┤
│                             │                                │
│ Source text                 │ Translation                    │
│                             │                                │
│                             │                                │
├─────────────────────────────┴────────────────────────────────┤
│ Model | Runtime | GPU | latency | status                     │
└──────────────────────────────────────────────────────────────┘
```

Required controls:

- source language selector;
- auto-detect source option;
- target language selector;
- swap languages;
- clear input;
- copy source;
- copy translation;
- translate button if manual mode is enabled;
- loading state;
- model selector;
- runtime status;
- GPU status;
- error feedback.

### 15.1 Translation behavior

Configurable modes:

- translate while typing after debounce;
- manual translation;
- translate on Ctrl+Enter.

Default behavior may be selected during implementation based on responsiveness.

---

## 16. Translation styles

Support optional translation profiles:

```text
Natural
Literal
Formal
Casual
Custom
```

These should map to adapter-level instructions and must not modify lexical/dictionary data.

Allow custom profiles later.

---

## 17. Lexical Engine

Lexical functionality is a core product feature.

Do not rely solely on an LLM to invent dictionary data.

The lexical engine should combine:

- local dictionary data;
- lemmatization;
- morphology;
- thesaurus relations;
- optional model-based contextual ranking/explanation.

Potential data source: processed Wiktionary-derived local datasets or other legally compatible local lexical datasets.

The exact dataset is an implementation decision, but data provenance and license must be documented.

---

## 18. Dictionary mode

Automatically detect when input is likely a word or short phrase.

Possible heuristic:

- single token;
- short lexical phrase;
- no sentence-ending punctuation;
- dictionary match available.

Do not require the user to manually switch modes in normal use.

For dictionary-mode input show:

- original form;
- lemma;
- part of speech;
- main translation(s);
- definitions;
- word forms;
- synonyms;
- antonyms;
- related words;
- alternative translations;
- examples.

---

## 19. Lemmatization and morphology

The lexical engine must normalize inflected forms before lookup.

Examples:

```text
running → run
went → go
cats → cat
кошками → кошка
людьми → человек
красивейшей → красивый
```

Where available, show morphological information such as:

### English

- part of speech;
- number;
- tense;
- participle form;
- comparative/superlative.

### Russian

- lemma;
- part of speech;
- gender;
- number;
- case;
- person;
- tense;
- aspect where relevant;
- animacy where relevant.

Language-specific analyzers may be implemented independently.

---

## 20. Word forms

Dictionary entries should provide forms where data exists.

Example English verb:

```text
run

Infinitive       run
3rd person       runs
Past             ran
Past participle  run
Gerund           running
```

Example Russian adjective:

```text
красивый

Masculine        красивый
Feminine         красивая
Neuter           красивое
Plural           красивые
Comparative      красивее
Superlative      красивейший
```

Do not fabricate forms when the lexical source does not support them reliably.

---

## 21. Synonyms

Synonyms are a first-class feature.

They must be separated by lexical sense when possible.

Example:

```text
weak

1. lacking physical strength
   frail
   feeble
   delicate

2. unconvincing
   poor
   flimsy
   unpersuasive

3. low in intensity
   faint
   low
```

Avoid flattening unrelated senses into a single list.

Optional synonym metadata:

```ts
export type LexicalRelation = {
  word: string;
  relation: 'synonym' | 'antonym' | 'related';
  senseId?: string;
  register?: 'neutral' | 'formal' | 'informal' | 'slang' | 'literary';
  strength?: 'weaker' | 'similar' | 'stronger';
  notes?: string[];
};
```

Do not expose numeric similarity scores in the main UI unless placed in an advanced/debug view.

---

## 22. Antonyms

Antonyms must be a separate category.

Do not force antonyms for words that do not have a meaningful lexical opposite.

Example:

```text
table
→ No direct antonyms
```

Distinguish where possible between:

- direct antonym;
- semantic opposite;
- absence of a property;
- weaker contrast.

Example:

```text
love
  hate           semantic opposite
  indifference   absence / different opposition
```

---

## 23. Related words

Related words must not be merged into synonyms.

Example:

```text
anger

Synonyms
  rage
  fury
  irritation

Related words
  angry
  furious
  irritated
  enraged
```

This category is important for vocabulary exploration.

---

## 24. Alternative translations

Alternative translations are distinct from synonyms.

For a translated word or phrase, show candidate translations suitable for the current source phrase.

Example:

```text
звероподобный

Primary translation
  beast-like

Alternatives
  animal-like
  bestial
  brutish
  feral-looking
```

The model may rank alternatives based on context.

The lexical database should remain the source of truth for dictionary facts where possible.

---

## 25. Context-sensitive alternatives

When translating a sentence, allow the user to select a translated word and request alternatives that fit the sentence.

Example:

```text
He was an unpleasant man.
```

Selecting `unpleasant` could show:

```text
disagreeable
unlikable
obnoxious       stronger
repulsive       much stronger
off-putting     informal
```

When the user selects an alternative, the application should update/retranslate the sentence where necessary to preserve grammar and context.

Do not perform simple string replacement if the replacement may require grammatical adjustment.

---

## 26. Definitions and sense handling

Dictionary entries should support multiple senses.

Each sense may contain:

- definition;
- translations;
- synonyms;
- antonyms;
- labels/register;
- examples.

Example conceptual structure:

```ts
export type LexicalSense = {
  id: string;
  definition?: string;
  translations?: string[];
  synonyms?: LexicalRelation[];
  antonyms?: LexicalRelation[];
  related?: LexicalRelation[];
  examples?: LexicalExample[];
};
```

---

## 27. Examples

Support two example sources:

1. examples from local lexical datasets;
2. optional model-generated examples.

Generated examples must be visually distinguished from sourced dictionary examples.

Example action:

```text
Generate examples
```

Model-generated examples are supplemental and must not overwrite dictionary data.

---

## 28. Dictionary UI

Suggested layout:

```text
┌──────────────────────────────────────────────────┐
│ obnoxious                              adjective │
├──────────────────────────────────────────────────┤
│ Meaning                                           │
│ extremely unpleasant or annoying                  │
│                                                   │
│ Russian                                           │
│ противный · мерзкий · неприятный                  │
├──────────────────────────────────────────────────┤
│ SYNONYMS                                          │
│ annoying       neutral                            │
│ disagreeable   formal                             │
│ repulsive      stronger                           │
│                                                   │
│ ANTONYMS                                          │
│ pleasant                                          │
│ agreeable                                         │
│ likable                                           │
├──────────────────────────────────────────────────┤
│ RELATED WORDS                                     │
│ offensiveness · annoy · repulsion                 │
├──────────────────────────────────────────────────┤
│ FORMS                                             │
│ ...                                               │
├──────────────────────────────────────────────────┤
│ EXAMPLES                                          │
│ ...                                               │
└──────────────────────────────────────────────────┘
```

Sections should be collapsible if the entry is large.

---

## 29. Translation history

Store translation history locally in SQLite.

Suggested schema:

```text
translations
------------
id
created_at
source_language
target_language
source_text
translated_text
model_id
runtime_id
latency_ms
favorite
```

Requirements:

- history screen;
- full-text search where practical;
- filter by language pair;
- favorites;
- delete individual entries;
- clear history;
- option to disable history entirely.

---

## 30. Global hotkeys

Desktop application must support configurable global hotkeys.

Initial defaults may be:

```text
Open translator          Ctrl+Alt+D
Translate selection      Ctrl+Alt+T
Open history             Ctrl+Alt+H
```

Do not hardcode the shortcuts permanently.

Detect conflicts where possible.

---

## 31. Clipboard and selected-text workflow

Primary workflow:

```text
select text
→ invoke hotkey
→ app obtains selected/copied text
→ translate
→ show compact popup
```

Popup should support:

- translation;
- copy;
- replace where supported;
- open full application;
- dictionary lookup for a word;
- alternatives.

Clipboard monitoring must be optional.

Settings:

```text
Show popup after translation
Monitor clipboard
Auto-translate copied text
```

Auto-translation should be disabled by default unless UX testing suggests otherwise.

---

## 32. System tray

Desktop application should support:

- minimize to tray;
- open main window;
- quick language pair status;
- active model display;
- pause/resume clipboard features;
- quit.

---

## 33. Browser extension architecture

The browser extension is a thin client.

It must not run its own translation model.

Architecture:

```text
Browser Extension
      ↓
Desktop Local API
      ↓
Translation Service / Lexical Engine
      ↓
Local Runtime / GPU
```

All settings, active models and lexical logic belong to the desktop app.

---

## 34. Browser extension communication

### 34.1 MVP

Use a localhost HTTP API and optionally WebSocket/SSE for status/events.

Bind only to loopback:

```text
127.0.0.1
```

Do not expose the API to the LAN by default.

### 34.2 Future

Support Chromium Native Messaging if it improves security/reliability.

The protocol layer must be designed so transport can be changed without rewriting extension business logic.

---

## 35. Local API security

A localhost API must not trust arbitrary browser pages.

Implement a pairing/token mechanism.

Requirements:

- generated local secret/token;
- extension pairing or registration;
- authenticated API requests;
- CORS restrictions;
- reject arbitrary website origins;
- bind to `127.0.0.1` only;
- rotate/revoke token capability where practical.

Never store the token in page DOM or expose it to website JavaScript.

---

## 36. Suggested local API

Example endpoints:

```text
GET  /api/v1/status
GET  /api/v1/models
GET  /api/v1/settings/language-pair

POST /api/v1/translate
POST /api/v1/lookup
POST /api/v1/alternatives
POST /api/v1/examples
```

Example translation request:

```json
{
  "text": "This is an awkward situation.",
  "sourceLanguage": "auto",
  "targetLanguage": "ru"
}
```

Example dictionary lookup:

```json
{
  "text": "awkward",
  "language": "en",
  "targetLanguage": "ru",
  "context": "This is an awkward situation."
}
```

Use versioned endpoints from the beginning.

---

## 37. Browser extension features

Required initial features:

### Selection button

When the user selects text, optionally show a small LocalTranslate button near the selection.

### Selection popup

For a word:

```text
awkward
неловкий · неудобный

Synonyms
clumsy · uncomfortable · embarrassing

Antonyms
graceful · comfortable

[More] [Copy]
```

For a sentence:

```text
Original sentence
Translated sentence

[Copy] [Replace] [Open]
```

### Context menu

Add entries:

```text
Translate with LocalTranslate
Open dictionary
Show synonyms
Show antonyms
```

### Double click

Optional setting:

```text
Double-click word → dictionary popup
```

### Editable fields

Where technically safe and supported, allow replacement of selected text in:

- textarea;
- input fields;
- contenteditable elements.

Do not break rich editors by aggressively mutating DOM.

---

## 38. Extension settings

Possible settings:

```text
Show button on text selection
Double-click word opens dictionary
Show synonyms
Show antonyms
Show related words
Enable Replace
Use desktop language pair
```

The desktop application remains the authoritative source for active model/runtime settings.

---

## 39. Settings screen

Recommended sections:

### General

```text
Start with Windows
Minimize to tray
Remember window size
Theme
```

### Translation

```text
Default source language
Default target language
Auto detect
Default translation style
Translate while typing
```

### Models

```text
Runtime
Active model
Model folders
Refresh models
Load/unload model
```

### Compute

```text
Device: Auto / GPU / CPU
GPU acceleration
GPU offload
Context size
Advanced runtime options
```

### Dictionary

```text
Show dictionary automatically for words
Show synonyms
Show antonyms
Show related words
Show word forms
Allow model-generated examples
```

### Integration

```text
Global hotkeys
Clipboard options
Local API status
Browser extension pairing
```

### Privacy

```text
Save translation history
Clear history
Local-only status
```

---

## 40. Privacy requirements

Core mode should work without sending translation text to third-party servers.

Requirements:

- local model requests stay local;
- lexical database stays local;
- history stays local;
- browser extension sends content only to the local desktop service;
- no analytics/telemetry by default;
- no hidden cloud fallback.

If cloud providers are added later, they must be explicitly opt-in and visibly identified.

---

## 41. Error handling

User-facing errors must be understandable.

Examples:

### Runtime unavailable

```text
LM Studio is not reachable at http://127.0.0.1:1234.
```

### Model unavailable

```text
The selected model is no longer available.
Choose another model or refresh the model list.
```

### CUDA OOM

```text
The model does not fit fully in available VRAM.
Retrying with partial GPU offload.
```

### Translation failure

```text
Translation failed.
The model returned an invalid response.
```

Detailed technical logs should be available in a diagnostics view/file.

---

## 42. Logging and diagnostics

Implement structured local logs.

Log:

- application startup;
- runtime connection;
- model discovery;
- model load/unload;
- backend selection;
- translation timing;
- runtime failures;
- CUDA errors;
- extension API errors.

Do not log full user text by default.

Allow debug logging to be enabled explicitly.

---

## 43. Performance requirements

Goals for normal interactive use:

- UI must remain responsive during model loading and generation;
- no blocking work on the UI thread;
- dictionary lookup should feel near-instant once indexes are loaded;
- language detection should be effectively immediate for normal text;
- streamed model output may be supported, but translation text should not visibly flicker unnecessarily;
- model loading must show progress/state where possible.

---

## 44. Caching

Consider local caching for:

- language detection results;
- lexical lookups;
- repeated translations;
- model metadata;
- benchmark results.

Translation cache key should include at least:

```text
source text
source language
target language
model
translation profile
```

Cache must be bounded and clearable.

---

## 45. Data models

Example common types:

```ts
export type RuntimeId = 'lmstudio' | 'llamacpp';

export type LocalModel = {
  id: string;
  name: string;
  family?: string;
  path?: string;
  runtimeId: RuntimeId;
  format?: 'gguf' | 'api' | 'unknown';
  quantization?: string;
  sizeBytes?: number;
  contextLength?: number;
  loaded?: boolean;
  available: boolean;
};

export type TranslationRequest = {
  text: string;
  sourceLanguage: string | 'auto';
  targetLanguage: string;
  mode?: 'natural' | 'literal' | 'formal' | 'casual';
  modelId?: string;
};

export type TranslationResult = {
  text: string;
  sourceLanguage: string;
  detectedSourceLanguage?: string;
  targetLanguage: string;
  modelId: string;
  runtimeId: RuntimeId;
  latencyMs: number;
};

export type LexicalEntry = {
  query: string;
  language: string;
  lemma?: string;
  partOfSpeech?: string;
  forms?: Record<string, string | string[]>;
  senses: LexicalSense[];
};
```

---

## 46. Development phases

Implementation should proceed incrementally.

### Phase 0 — Project bootstrap

Create:

- monorepo/workspace;
- Tauri desktop app;
- React/Vite frontend;
- shared TypeScript packages;
- linting/formatting;
- build scripts;
- basic settings persistence;
- documentation files.

### Phase 1 — Translation proof of concept

Implement:

- LM Studio connectivity;
- model enumeration;
- model selection;
- TranslateGemma adapter;
- source/target language selection;
- basic two-panel translation UI;
- error handling;
- latency display.

Acceptance criterion:

```text
Desktop UI → LM Studio → selected TranslateGemma model → translation result
```

### Phase 2 — Model Manager

Implement:

- model registry;
- normalized model metadata;
- quantization display;
- refresh model list;
- saved selected model;
- model state UI.

### Phase 3 — Language detection

Implement:

- local language detector;
- auto detect;
- ambiguity handling;
- detected language UI.

### Phase 4 — History and desktop integration

Implement:

- SQLite history;
- search;
- favorites;
- tray;
- global hotkeys;
- clipboard translation popup.

### Phase 5 — Lexical MVP

Implement:

- lexical data source;
- lemma lookup;
- part of speech;
- definitions;
- basic forms;
- synonyms;
- antonyms;
- related words;
- dictionary UI.

### Phase 6 — Context-aware lexical tools

Implement:

- sense-aware synonym grouping;
- context-sensitive alternatives;
- lexical annotations such as register/strength;
- contextual replacement in translations;
- model-generated examples.

### Phase 7 — Browser extension

Implement:

- localhost API;
- secure pairing;
- extension selection button;
- popup translation;
- word dictionary popup;
- synonyms/antonyms;
- context menu;
- replace in editable fields where safe.

### Phase 8 — llama.cpp runtime

Implement:

- local GGUF scanning;
- llama.cpp sidecar/process integration;
- CUDA backend;
- model load/unload;
- automatic GPU offload;
- CPU fallback;
- runtime settings.

### Phase 9 — Hardware and benchmarking

Implement:

- GPU detection;
- VRAM reporting;
- model memory estimates;
- benchmark action;
- benchmark history;
- model performance display.

### Phase 10 — Product hardening

Implement:

- installer/update strategy;
- crash handling;
- diagnostics;
- migration/versioning;
- extension packaging;
- UI polish;
- accessibility;
- performance testing.

---

## 47. MVP definition

The MVP is complete when the following workflow works reliably:

1. User starts LocalTranslate.
2. App connects to LM Studio.
3. App detects all available models.
4. User selects a TranslateGemma model/quantization.
5. User enters text.
6. App detects the source language or accepts manual selection.
7. Translation is produced locally.
8. Translation can be copied.
9. Model/runtime/latency status is visible.
10. Word lookup provides at least lemma, part of speech, translation, synonyms and antonyms from local lexical data.
11. Translation history works locally.

The browser extension and embedded llama.cpp runtime may follow after the core MVP, but the architecture must support them from the beginning.

---

## 48. UX principles

1. Do not expose unnecessary model/runtime complexity to normal users.
2. Keep advanced controls available for power users.
3. Default to automatic hardware settings.
4. Never silently switch model or compute backend without informing the user.
5. Distinguish dictionary facts from model-generated suggestions.
6. Keep synonyms, antonyms, related words and alternative translations as separate concepts.
7. Make word lookup nearly as fast and natural as translation.
8. Preserve privacy and local-first behavior.

---

## 49. Suggested status bar

Example:

```text
TranslateGemma 4B Q8_0 | LM Studio | CUDA | RTX 3060 | 742 ms
```

For llama.cpp:

```text
TranslateGemma 4B Q8_0 | llama.cpp | CUDA | Full GPU | 6.2 GB VRAM
```

---

## 50. Acceptance requirements for model portability

The project must satisfy these architectural tests:

### Test A

Switching from one TranslateGemma quantization to another must require no UI code changes.

### Test B

Adding a new translation model family should primarily require a new adapter, not changes throughout the application.

### Test C

Switching from LM Studio runtime to llama.cpp must not require changes to the main translation UI.

### Test D

The browser extension must use the same translation and lexical services as the desktop app through a stable protocol.

### Test E

Synonyms, antonyms and morphology must remain available even if the active translation model is changed.

---

## 51. Code quality requirements

- Strict TypeScript where applicable.
- Avoid `any` except at external boundaries with justification.
- Shared schemas for desktop/extension API requests.
- Validate all IPC/API input.
- Keep model-specific code out of UI components.
- Keep runtime-specific code out of translation adapters.
- Add unit tests for adapters and lexical normalization.
- Add integration tests for local API endpoints.
- Add a minimal end-to-end smoke test for translation.
- Document non-obvious architectural decisions.

---

## 52. Security requirements

- Bind local API to loopback only.
- Authenticate browser extension requests.
- Do not execute model-generated text as code.
- Validate local file paths.
- Do not allow arbitrary web pages to invoke privileged desktop actions.
- Do not expose filesystem browsing through the extension API.
- Sanitize HTML shown in extension popups.
- Treat translation content as untrusted text.

---

## 53. Future extensions

Architecture may later support:

- cloud model providers as optional backends;
- Ollama runtime;
- additional local model formats;
- VS Code extension;
- Notepad++ integration;
- file translation;
- custom glossaries;
- user terminology lists;
- translation memory;
- automatic model download/install UI;
- model quality comparison tools;
- additional lexical datasets;
- optional TTS.

These are future enhancements and must not delay the initial implementation.

---

## 54. Initial implementation instruction for the coding agent

Start with Phase 0 and Phase 1 only.

Do not attempt to implement the entire specification in one pass.

Before writing significant code:

1. inspect the repository;
2. create or update the project architecture documentation;
3. propose the exact workspace structure;
4. identify dependencies;
5. implement the smallest end-to-end translation slice;
6. verify it against a running LM Studio OpenAI-compatible endpoint;
7. keep all model-specific behavior behind `TranslationModelAdapter`;
8. keep all runtime-specific behavior behind `ModelRuntime`;
9. ensure the UI is not tied to TranslateGemma by name;
10. leave clear TODO boundaries for later phases.

The first milestone is successful local translation through a selectable LM Studio model, not UI polish.
