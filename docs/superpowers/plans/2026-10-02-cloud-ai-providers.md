# Cloud AI Providers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить в LingvoLoc явно включаемый BYOK-режим перевода через OpenAI, Anthropic, Google Gemini, DeepL и произвольный OpenAI-compatible endpoint, сохранив локальный режим основным и полностью автономным.

**Architecture:** Новый высокоуровневый `TranslationBackend` располагается над существующим разделением `TranslationModelAdapter` / `ModelRuntime`. Локальный backend переиспользует Standalone и LM Studio без изменения их поведения; облачные backend-ы преобразуют один структурированный `TranslationRequest` в нативный протокол провайдера. Все точки входа остаются за единым inference coordinator, а API-ключи доступны только Rust backend через Windows Credential Manager.

**Tech Stack:** Rust stable, Tauri 2, reqwest 0.12, tokio, Windows Credential Manager through `keyring`, serde, React, strict TypeScript, Vitest, Chromium MV3 extension.

**Spec:** Approved section-by-section in chat on 2026-10-02; this plan contains the complete approved design and no separate spec file is required.

## Global Constraints

- `standalone` остается runtime по умолчанию. Облачный backend никогда не включается автоматически.
- Между Local и Cloud нет автоматического fallback. Ошибка выбранного backend возвращается пользователю.
- MVP использует BYOK. ChatGPT Plus/Pro, Claude Pro/Max, Google AI Pro/Ultra и DeepL Translator subscriptions не считаются API-доступом и не заменяют отдельные API key/billing plans.
- Поддерживаемые cloud backend-ы MVP: OpenAI, Anthropic, Gemini, DeepL и OpenAI-compatible.
- Text, Files, clipboard popup и browser extension используют один выбранный backend.
- API-ключи хранятся только в Windows Credential Manager. Они не попадают в `Settings`, localStorage, SQLite, document snapshots, loopback API, логи или сообщения об ошибках.
- Direct-provider endpoints фиксированы в коде. Пользовательский endpoint разрешен только для OpenAI-compatible; plain HTTP разрешен только для loopback addresses.
- Cloud consent обязателен до первого сетевого перевода и хранится локально без ключа.
- OpenAI, Anthropic и Gemini используют provider-native API. OpenAI-compatible использует Chat Completions. DeepL использует `/v2/translate` для каждого LingvoLoc block и не получает исходный документ целиком.
- DeepL поддерживает только `neutral`. Остальные LingvoLoc style presets должны быть явно недоступны, а не молча проигнорированы.
- Recommended models встроены в релиз. Пользователь может вручную обновить доступные модели и всегда может ввести custom model id.
- Model refresh никогда не выполняется автоматически при запуске.
- Usage показывается без расчета цены: exact provider tokens для LLM и billed characters для DeepL.
- Session usage хранится только в памяти процесса и обнуляется после перезапуска.
- Интерактивные cloud-запросы автоматически не повторяются. Document worker делает не более одного retry только для `429`, `502`, `503`, `504`, учитывая `Retry-After`.
- Старые настройки, локальные document snapshots, history rows и установленное расширение должны продолжать работать.
- Сохранять compatibility identifiers `com.lingoloc.desktop`, `lingoloc.sqlite` и `lingoloc.settings`.
- Автотесты используют только локальные mock servers и fake credential store. Реальные ключи запрещены в коде, fixtures и CI.
- `README.md` и `README.ru.md` обновляются синхронно.

## Target File Structure

- Create `apps/desktop/src-tauri/src/backends/mod.rs`: backend trait, factory, capabilities and common provider helpers.
- Create `apps/desktop/src-tauri/src/backends/local.rs`: wrapper around current local adapter/runtime pipeline.
- Create `apps/desktop/src-tauri/src/backends/http.rs`: bounded HTTP, streaming, cancellation, retry metadata and safe error parsing.
- Create `apps/desktop/src-tauri/src/backends/openai.rs`: native OpenAI Responses API.
- Create `apps/desktop/src-tauri/src/backends/openai_compatible.rs`: configurable Chat Completions API.
- Create `apps/desktop/src-tauri/src/backends/anthropic.rs`: Anthropic Messages API.
- Create `apps/desktop/src-tauri/src/backends/gemini.rs`: Gemini generateContent API.
- Create `apps/desktop/src-tauri/src/backends/deepl.rs`: DeepL text translation and languages API.
- Create `apps/desktop/src-tauri/src/services/credentials.rs`: serialized credential-store abstraction.
- Create `apps/desktop/src-tauri/src/services/request_control.rs`: cancellation for active cloud streams.
- Create `apps/desktop/src-tauri/src/services/session_usage.rs`: process-lifetime usage aggregation.
- Create `apps/desktop/src/app/CloudRuntimeSettings.tsx`: cloud provider settings UI.
- Create `apps/desktop/src/app/SessionUsageTable.tsx`: current-process usage UI.
- Modify `domain.rs`, `services/translation.rs`, `services/inference_coordinator.rs`, `lib.rs`, document worker/store/commands, history, loopback API, frontend settings/commands, popup, extension and both READMEs.

## Shared Interfaces

Add provider identities without replacing persisted local runtime names:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    LlamaCpp,
    LmStudio,
    OpenAi,
    Anthropic,
    Gemini,
    DeepL,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeMode {
    LmStudio,
    #[default]
    Standalone,
    OpenAi,
    Anthropic,
    Gemini,
    DeepL,
    OpenAiCompatible,
}
```

Keep current token fields in `TranslationResult` for extension compatibility and add only optional metadata:

```rust
pub struct TranslationResult {
    pub text: String,
    pub model_id: String,
    pub adapter_id: String,
    pub latency_ms: u128,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub provider_id: Option<ProviderId>,
    #[serde(default)]
    pub billed_characters: Option<u64>,
}
```

Backend capabilities must drive UI behavior instead of provider-name checks in React:

```rust
pub struct BackendCapabilities {
    pub model_list: bool,
    pub custom_model_id: bool,
    pub token_usage: bool,
    pub billed_characters: bool,
    pub translation_styles: bool,
}

pub trait TranslationBackend {
    fn provider_id(&self) -> ProviderId;
    fn capabilities(&self) -> BackendCapabilities;
    fn status(&self) -> Result<RuntimeStatus, RuntimeError>;
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError>;
    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError>;
}
```

The frontend settings contract stores provider configuration but no secret:

```ts
export interface CloudModelConfig {
  modelId: string;
  availableModels: LocalModel[];
  modelsRefreshedAt: number | null;
}

export interface CloudSettings {
  consentAccepted: boolean;
  openAi: CloudModelConfig;
  anthropic: CloudModelConfig;
  gemini: CloudModelConfig;
  deepL: {
    plan: 'free' | 'pro';
    availableLanguages: string[];
    languagesRefreshedAt: number | null;
  };
  openAiCompatible: CloudModelConfig & { endpoint: string };
}
```

---

### Task 1: Domain Contracts And Settings Migration

**Files:**

- Modify: `apps/desktop/src-tauri/src/domain.rs`
- Modify: `apps/desktop/src-tauri/src/services/inference_coordinator.rs`
- Modify: `apps/desktop/src/lib/settings.ts`
- Test: `apps/desktop/src/lib/settings.test.ts`
- Test: existing Rust tests in `domain.rs` and `inference_coordinator.rs`

**Interfaces:**

- Produce `ProviderId`, expanded `RuntimeMode`, `BackendCapabilities`, optional provider usage fields and cloud settings defaults.
- Preserve the exact current local snapshot string produced for `standalone` and `lmStudio`.
- Produce cloud snapshots containing runtime mode, provider endpoint category and model id, but no key, consent flag or cached model list.

- [ ] Add a failing Rust deserialization test proving the pre-cloud JSON settings fixture still resolves to `RuntimeMode::Standalone` with empty/default cloud settings.
- [ ] Add failing TypeScript tests for loading old localStorage settings, invalid cloud provider data and each new runtime mode.
- [ ] Add a failing coordinator test asserting the existing standalone snapshot string is unchanged.
- [ ] Add a failing coordinator test asserting an OpenAI snapshot contains provider/model but not any supplied credential marker.
- [ ] Implement the enum variants, serde defaults and frontend defaults.
- [ ] Add `provider_id` and `billed_characters` as optional fields without removing current token fields.
- [ ] Extend `RuntimeError` with distinguishable authentication, quota, rate-limit, content-rejected and cancelled variants. Rate-limit errors carry `retry_after: Option<Duration>` internally but serialize a safe user-facing string.
- [ ] Run the focused tests and confirm all pass:

```powershell
npm exec vitest run apps/desktop/src/lib/settings.test.ts
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml inference_coordinator
```

- [ ] Commit implementation and direct tests together as `Add cloud backend domain contracts`.

### Task 2: Windows Credential Store

**Files:**

- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/services/mod.rs`
- Create: `apps/desktop/src-tauri/src/services/credentials.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/lib/commands.ts`

**Interfaces:**

- Add `keyring = { version = "4.2", default-features = false, features = ["windows-native"] }`.
- Produce `CredentialStore` with `status`, `set`, `get` and `delete` operations keyed by `ProviderId`.
- Use target names `com.lingoloc.desktop.ai.openai`, `.anthropic`, `.gemini`, `.deepl` and `.openai-compatible`.
- Expose Tauri commands `get_provider_credential_status`, `save_provider_credential` and `delete_provider_credential`. No command returns the stored secret.

- [ ] Write failing unit tests with an in-memory store for missing, saved, replaced and deleted keys.
- [ ] Write a failing test proving an error rendered with `Debug` or `Display` does not include the test secret.
- [ ] Implement one mutex-protected Windows store because Windows Credential Manager does not guarantee same-entry operation ordering across threads.
- [ ] Reject empty or whitespace-only keys before touching the OS store.
- [ ] Return `{ configured: boolean, hint: string | null }`, where a hint contains at most the first four and last four characters and is computed only during save/status handling.
- [ ] Register Tauri commands and TypeScript wrappers.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml credentials
npm run typecheck
```

- [ ] Commit as `Store provider keys in Windows credentials`.

### Task 3: TranslationBackend And Local Regression Lock

**Files:**

- Create: `apps/desktop/src-tauri/src/backends/mod.rs`
- Create: `apps/desktop/src-tauri/src/backends/local.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/services/translation.rs`
- Test: existing tests in adapters, runtimes and translation service

**Interfaces:**

- Produce the approved `TranslationBackend` trait and a factory selected from `Settings.runtime_mode`.
- `LocalBackend` consumes the current `Family::from_model_id`, `StandaloneRuntime`, `LmStudioRuntime` and `CompletionResponse` without duplicating their logic.
- `services::translation::translate` remains the single service entry point used by Tauri, documents, clipboard and loopback API.

- [ ] Add failing tests for LocalBackend provider ids, capabilities and adapter selection.
- [ ] Add a regression test asserting a known TranslateGemma request produces the same `CompletionRequest` before and after the wrapper.
- [ ] Move only orchestration into `LocalBackend`; do not rewrite streaming or llama-server lifecycle.
- [ ] Make `translation::status`, `list_models` and `translate` delegate through the backend factory.
- [ ] Run all adapter/runtime tests:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml adapters
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml lm_studio
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml llama_server
```

- [ ] Commit as `Route local translation through backend interface`.

### Task 4: Safe Cloud HTTP And Cancellation

**Files:**

- Create: `apps/desktop/src-tauri/src/backends/http.rs`
- Create: `apps/desktop/src-tauri/src/services/request_control.rs`
- Modify: `apps/desktop/src-tauri/src/services/mod.rs`
- Modify: `apps/desktop/src-tauri/src/domain.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

**Interfaces:**

- Produce `RequestCancellation` and an active-request registry capable of cancelling the current document request.
- Produce common connect timeout `3 s`, idle timeout `45 s`, whole-request cap `900 s` and error body cap `1 MiB`, matching current local request safety.
- Produce a provider-neutral SSE line reader. Provider modules remain responsible for interpreting their event JSON.
- Produce `safe_http_error(status, headers, body)` that never includes request headers/body and truncates provider details to 500 Unicode scalars.

- [ ] Add a mock server test that accepts a request and never returns headers; expect `RuntimeError::Timeout` before five seconds under a test-only shortened timeout.
- [ ] Add a stalled-stream test that emits one event then stops; expect idle timeout.
- [ ] Add a cancellation test that closes an active stream and returns `RuntimeError::Cancelled`.
- [ ] Add tests mapping `401/403`, quota responses, `429` with seconds/date `Retry-After`, `502/503/504`, malformed JSON and oversized error bodies.
- [ ] Add a redaction test whose key and source sentence appear in the outgoing request but never in the returned error or trace payload.
- [ ] Implement the minimal HTTP helper and cancellation registry.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml backends::http
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml request_control
```

- [ ] Commit as `Add cancellable cloud HTTP transport`.

### Task 5: OpenAI And OpenAI-Compatible Backends

**Files:**

- Create: `apps/desktop/src-tauri/src/backends/openai.rs`
- Create: `apps/desktop/src-tauri/src/backends/openai_compatible.rs`
- Modify: `apps/desktop/src-tauri/src/backends/mod.rs`

**Interfaces:**

- OpenAI uses fixed base URL `https://api.openai.com/v1`, `Authorization: Bearer`, `/responses` and `/models`.
- OpenAI-compatible uses user base URL, `Authorization: Bearer`, `/chat/completions` and optional `/models`.
- Both consume the same neutral translation prompt built from source language, target language, style and text.
- Native OpenAI parses `usage.input_tokens`, `usage.output_tokens`, `usage.total_tokens`; compatible parsing accepts ordinary OpenAI Chat Completions usage.

- [ ] Write failing OpenAI tests for request path, auth header, prompt content, streamed output and exact usage.
- [ ] Write a failing test proving native OpenAI does not silently use Chat Completions.
- [ ] Write failing compatible tests for plain JSON, SSE, missing usage, missing `/models`, custom model id and a base URL with a trailing slash.
- [ ] Write URL validation tests: allow `https://host`, `http://127.0.0.1`, `http://localhost` and `[::1]`; reject remote plain HTTP, URL credentials, fragments and non-HTTP schemes.
- [ ] Implement native and compatible backend modules without sharing provider-specific response structs.
- [ ] Ensure `/models` failure on a compatible endpoint does not make translation unavailable.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml backends::openai
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml backends::openai_compatible
```

- [ ] Commit as `Add OpenAI cloud backends`.

### Task 6: Anthropic Backend

**Files:**

- Create: `apps/desktop/src-tauri/src/backends/anthropic.rs`
- Modify: `apps/desktop/src-tauri/src/backends/mod.rs`

**Interfaces:**

- Use fixed base URL `https://api.anthropic.com`.
- Send `POST /v1/messages`, `x-api-key`, `anthropic-version: 2023-06-01`, top-level system prompt and user message.
- Refresh available models through `GET /v1/models` with pagination support.
- Normalize `usage.input_tokens` and `usage.output_tokens` into existing result token fields.

- [ ] Write failing tests for required headers and absence of the API key from the JSON body.
- [ ] Write a streaming fixture containing `message_start`, text deltas, `message_delta` usage and `message_stop`; assert assembled translation and exact usage.
- [ ] Write tests for paginated model listing, invalid key, exhausted credits, rate limit and content rejection.
- [ ] Implement `AnthropicBackend` and register it in the factory.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml backends::anthropic
```

- [ ] Commit as `Add Anthropic translation backend`.

### Task 7: Gemini Backend

**Files:**

- Create: `apps/desktop/src-tauri/src/backends/gemini.rs`
- Modify: `apps/desktop/src-tauri/src/backends/mod.rs`

**Interfaces:**

- Use fixed base URL `https://generativelanguage.googleapis.com/v1beta`.
- Authenticate with `x-goog-api-key`.
- Generate through `models/{model}:streamGenerateContent` using SSE and list through `/models`.
- Normalize `usageMetadata.promptTokenCount`, `candidatesTokenCount` and `totalTokenCount`.

- [ ] Write failing request-shape and auth-header tests.
- [ ] Write a streaming fixture with multiple candidates events and final usage metadata; accept text only from the first candidate.
- [ ] Write model-refresh tests filtering entries whose supported generation methods include `generateContent`.
- [ ] Write tests for safety-blocked output, empty candidates, quota and rate-limit errors.
- [ ] Implement `GeminiBackend` and register it in the factory.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml backends::gemini
```

- [ ] Commit as `Add Gemini translation backend`.

### Task 8: DeepL Backend

**Files:**

- Create: `apps/desktop/src-tauri/src/backends/deepl.rs`
- Modify: `apps/desktop/src-tauri/src/backends/mod.rs`

**Interfaces:**

- Free base URL is `https://api-free.deepl.com`; Pro base URL is `https://api.deepl.com`.
- Authenticate with `Authorization: DeepL-Auth-Key <key>`.
- Translate through `POST /v2/translate` with one `text` element, `source_lang` when known, `target_lang` and `show_billed_characters: true`.
- Refresh source/target language capabilities through the official languages endpoints.
- Return `provider_id = DeepL`, `billed_characters`, no token fields and an adapter id identifying direct DeepL translation.

- [ ] Write failing tests for Free/Pro URLs, auth, source auto-detection omission, explicit source, target mapping and billed characters.
- [ ] Write a failing test that rejects literary/technical/conversational before opening an HTTP connection.
- [ ] Write tests for the 128 KiB request-size guard and supported-language refresh.
- [ ] Implement `DeepLBackend`; do not use the DeepL whole-document endpoint.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml backends::deepl
```

- [ ] Commit as `Add DeepL translation backend`.

### Task 9: Provider Commands, Manual Refresh And Connection Test

**Files:**

- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/lib/commands.ts`
- Modify: `apps/desktop/src/lib/commands.test.ts`

**Interfaces:**

- Add `get_backend_capabilities`.
- Add `refresh_provider_models`; for DeepL expose `refresh_deepl_languages`.
- Add `test_provider_connection`, which performs a read-only models/languages/usage request and never sends user text.
- Credential lookup occurs inside Rust immediately before constructing a cloud backend.

- [ ] Write failing command tests for missing credentials, successful status, model refresh and DeepL languages refresh.
- [ ] Test that connection checks do not call any completion/translation endpoint.
- [ ] Implement commands and TypeScript response types.
- [ ] Preserve a custom model id even when it is absent from a refreshed list.
- [ ] Treat an unsupported compatible `/models` endpoint as `modelsUnavailable`, not a failed connection.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider
npm exec vitest run apps/desktop/src/lib/commands.test.ts
npm run typecheck
```

- [ ] Commit as `Add provider setup and model refresh commands`.

### Task 10: Session Usage And History Provider Migration

**Files:**

- Create: `apps/desktop/src-tauri/src/services/session_usage.rs`
- Modify: `apps/desktop/src-tauri/src/services/mod.rs`
- Modify: `apps/desktop/src-tauri/src/services/history.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/lib/commands.ts`

**Interfaces:**

- Produce an in-memory `SessionUsageTracker` keyed by provider/model.
- Track successful requests, failed requests, input/output/total tokens and billed characters.
- Add `get_session_usage`; no persistence or reset command is required because process restart is the approved reset.
- Add nullable `provider_id` to `translation_history`, then expose `local/unknown` for migrated rows.

- [ ] Write failing aggregation tests for OpenAI tokens, DeepL characters, missing usage and failed requests.
- [ ] Write a history migration test starting from current schema version `2` and asserting existing rows survive.
- [ ] Increase the history schema version and update insert/list/search/export paths consistently.
- [ ] Record provider metadata for successful interactive and loopback translations, but never persist usage totals.
- [ ] Ensure `translate_word` remains excluded from session usage unless it is already counted as a user-visible translation under current behavior; lock the chosen behavior with a test. Preferred behavior: exclude alignment/word helper requests.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml session_usage
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml history
```

- [ ] Commit as `Track cloud usage for the current session`.

### Task 11: Document Worker, Retry And Cancellation

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/domain.rs`
- Modify: `apps/desktop/src-tauri/src/documents/worker.rs`
- Modify: `apps/desktop/src-tauri/src/documents/store.rs`
- Modify: `apps/desktop/src-tauri/src/documents/commands.rs`
- Modify: `apps/desktop/src-tauri/src/documents/commands/fb2.rs`
- Modify: `apps/desktop/src-tauri/src/documents/commands/pdf.rs`
- Modify: `apps/desktop/src/lib/commands.ts`

**Interfaces:**

- Generalize transient `RequestTokenCounts` into `RequestUsage` with optional token fields and `billed_characters`.
- Register each active cloud document request under its job id in `request_control`.
- Preserve existing per-block coordinator acquisition and interactive priority.

- [ ] Add a failing test proving cloud document snapshots reject resume after provider/model/endpoint change.
- [ ] Add a regression test proving existing local job snapshots still resume.
- [ ] Add table-driven retry tests: retry once for `429/502/503/504`; do not retry authentication, quota, content, malformed response, timeout after completed response, or cancellation.
- [ ] Add a `Retry-After` test with a test clock/sleeper so the suite does not wait in real time.
- [ ] Add pause and cancel tests proving the active stream is cancelled and completed blocks remain stored.
- [ ] Persist no session-only usage fields to `document-jobs.sqlite`; expose only the current process/job usage through summaries.
- [ ] Keep the existing source-hash, parser-version and safe-export behavior unchanged.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::worker
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::store
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::commands
```

- [ ] Commit as `Route document jobs through cloud backends`.

### Task 12: Cloud Settings UI

**Files:**

- Create: `apps/desktop/src/app/CloudRuntimeSettings.tsx`
- Create: `apps/desktop/src/app/SessionUsageTable.tsx`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/styles.css`
- Modify: `apps/desktop/src/lib/settings.ts`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Test: new focused component tests beside the new components

**Interfaces:**

- Settings first chooses Local or Cloud, then the concrete backend.
- Cloud component receives settings plus command functions; it never receives a stored key.
- Model selector groups `Recommended by LingvoLoc`, `Available from provider` and `Custom model ID`.

- [ ] Write failing tests for switching Local/Cloud without automatic translation or fallback.
- [ ] Write failing tests for the first-use cloud consent dialog and refusal path.
- [ ] Test Save, Replace, Delete and Test connection using mocked Tauri commands; assert saved key text is cleared from React state after success.
- [ ] Test manual model refresh, timestamp display, failed refresh preserving the previous cache and custom model preservation.
- [ ] Test compatible endpoint validation messages.
- [ ] Test that DeepL forces/requests `neutral` before saving the selection and disables the Style control with an explanation.
- [ ] Test session usage rows for token and character providers and empty-session state.
- [ ] Extract cloud-specific UI from `App.tsx`; do not move unrelated dictionary or local llama.cpp settings.
- [ ] Show an always-visible `Cloud · Provider · Model` status whenever cloud is selected.
- [ ] Add links to each provider's official API-key and API-billing pages and state that chat subscriptions are separate.
- [ ] Run:

```powershell
npm exec vitest run apps/desktop/src/app/App.test.tsx
npm exec vitest run apps/desktop/src/app/CloudRuntimeSettings.test.tsx
npm exec vitest run apps/desktop/src/app/SessionUsageTable.test.tsx
npm run lint
npm run typecheck
```

- [ ] Commit as `Add cloud provider settings interface`.

### Task 13: Clipboard, Loopback API And Extension

**Files:**

- Modify: `apps/desktop/src/app/ClipboardPopup.tsx`
- Modify: `apps/desktop/src/app/ClipboardPopup.test.tsx`
- Modify: `apps/desktop/src-tauri/src/api.rs`
- Modify: `apps/extension/src/api.ts`
- Modify: `apps/extension/src/api.test.ts`
- Modify: `apps/extension/src/popup.ts`
- Modify: relevant popup formatting tests and CSS if a provider badge is added

**Interfaces:**

- Loopback translation response retains all current fields and adds optional `provider_id` and `billed_characters`.
- Extension request contains no API key and continues authenticating only to the desktop loopback API with its pairing token.
- Clipboard and extension show usage for their last request, not global session totals.

- [ ] Add a Rust API test showing that selecting a cloud backend routes through the same coordinator and returns provider metadata.
- [ ] Add a test proving the loopback request and response never expose provider credentials.
- [ ] Add extension tests for a new response, an old response without provider fields, token usage and DeepL billed characters.
- [ ] Update popup status text to include Cloud provider after a response while preserving current latency formatting.
- [ ] Update clipboard popup tests for cloud provider and billed-character formatting.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml api
npm exec vitest run apps/desktop/src/app/ClipboardPopup.test.tsx
npm run extension:test
npm run extension:typecheck
```

- [ ] Commit as `Expose cloud translations to clipboard and extension`.

### Task 14: Documentation, Security Review And Release Gate

**Files:**

- Modify: `README.md`
- Modify: `README.ru.md`
- Modify: `DECISIONS.md`
- Modify local-only handoff files only if they are used during the implementation session; never publish them unless explicitly requested

**Documentation content:**

- State that Local remains default and works offline after model installation.
- Replace absolute claims that LingvoLoc never sends text to the internet with conditional wording for explicitly selected Cloud mode.
- Document supported providers, exact authentication mechanism, separate API billing, session-only usage and no automatic fallback.
- Explain that consumer chat subscriptions do not include API use.
- Explain DeepL API Free/paid API plans separately from DeepL Translator subscriptions.
- Explain provider data processing is governed by that provider's account and policy.
- Document how to remove credentials from Settings and Windows Credential Manager.

- [ ] Review all log sites and error conversions for authorization header, key, prompt and source-text leakage.
- [ ] Search tracked files for realistic key prefixes and reject any fixture that resembles a live secret.
- [ ] Run formatting and the full repository gate:

```powershell
npm run check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
npm run extension:build
```

- [ ] Fetch PDFium, build the desktop app and verify staged resources:

```powershell
pwsh -File scripts/fetch-pdfium.ps1
npm run desktop:build
npm run desktop:verify-bundle
```

- [ ] Perform the manual smoke matrix below with provider-owned test accounts and minimal billable text.
- [ ] Commit documentation separately as `Document optional cloud translation`.
- [ ] Do not bump versions, package a release ZIP, tag or publish unless separately requested.

## Manual Smoke Matrix

Run each applicable row for OpenAI, Anthropic, Gemini, DeepL Free/Pro and one OpenAI-compatible endpoint:

| Scenario                     | Expected result                                                                |
| ---------------------------- | ------------------------------------------------------------------------------ |
| Save credential              | Key disappears from the form; only configured state and masked hint remain     |
| Restart application          | Credential still works; no key appears in settings/localStorage                |
| Replace credential           | New key is used after one serialized store operation                           |
| Delete credential            | Provider becomes unconfigured and translation is blocked before network access |
| Test connection              | Read-only endpoint succeeds without sending user text                          |
| Refresh models/languages     | List and timestamp update only after explicit button press                     |
| Custom model id              | Value survives refresh even when absent from provider list                     |
| Text translation             | Correct direction, provider badge, latency and exact usage appear              |
| Clipboard popup              | Uses selected backend and displays last-request usage                          |
| Browser extension            | Uses desktop backend; extension stores no provider key                         |
| Multi-block TXT              | Blocks persist independently and session usage accumulates                     |
| DOCX/EPUB/FB2/PDF            | Existing reconstruction and safe export behavior remain intact                 |
| Pause document               | Active cloud stream closes; translated blocks remain                           |
| Resume document              | Continues only with matching provider/model snapshot                           |
| Cancel document              | Request is cancelled locally with no promise of provider billing cancellation  |
| Rate limit                   | Interactive request fails visibly; document waits/retries once                 |
| Invalid key                  | Clear authentication error with no key/header/body leakage                     |
| Quota exhausted              | No automatic retry and no local/cloud fallback                                 |
| DeepL non-neutral style      | Blocked before network request with actionable UI explanation                  |
| Remote HTTP compatible URL   | Rejected before credential or text transmission                                |
| Loopback HTTP compatible URL | Accepted for a user-managed local server                                       |
| Session restart              | Session usage returns to zero; history remains                                 |
| Standalone and LM Studio     | Behavior, prompts, model discovery and document resume remain unchanged        |

## Self-Review Checklist

- [x] Every approved provider has a native or explicitly compatible backend task.
- [x] Every application entry point is covered.
- [x] Credential storage, redaction and manual deletion are covered.
- [x] Manual model refresh and custom model ids are covered.
- [x] DeepL style and character-usage differences are explicit.
- [x] Document retry and cancellation semantics are testable and bounded.
- [x] Existing settings, snapshots, history and extension compatibility are preserved.
- [x] Consumer subscriptions versus API billing is documented.
- [x] No task requires a real key in automated tests.
- [x] No placeholders, deferred MVP requirements or implicit provider-specific behavior remain.

## Execution Handoff

Preferred execution is subagent-driven development with one fresh implementation agent and one review gate per task. If the work is executed inline, use `superpowers:executing-plans` and stop for review after Tasks 4, 9, 11 and 14. Create an isolated worktree before implementation, keep tests with their implementation commits, and never commit unrelated working-tree changes.
