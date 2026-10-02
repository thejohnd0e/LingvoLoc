# Token Usage Statistics Implementation Plan

> **Status:** Deferred for future implementation. The current scope is the simplified per-request token display plan in `2026-10-02-simple-token-display.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить точный учет входных и выходных токенов для пользовательских переводов, показатели текущего запроса и агрегаты `Today` / `All time`.

**Architecture:** Runtime нормализует provider-reported usage. Интерактивные и документные переводы хранят отдельные дневные агрегаты в своих SQLite-БД. UI объединяет их через одну Tauri-команду.

**Tech Stack:** Rust, Tauri 2, rusqlite, serde, React, strict TypeScript, Vitest, Chromium MV3 extension.

**Spec:** Approved in-chat design from 2026-10-02; no separate spec file was requested.

## Global Constraints

- Учитывать только точные данные, переданные provider; не оценивать токены по символам или SSE chunks.
- Не учитывать `translate_word` и служебное сопоставление слов.
- Разделять агрегаты по локальной дате, provider и модели.
- История и статистика сбрасываются независимо.
- Не сохранять raw usage events и тексты в статистике.
- Публичные LLM API пока не реализуются, но контракт должен допускать их добавление.
- Сохранять совместимые идентификаторы `com.lingoloc.desktop`, `lingoloc.sqlite` и `lingoloc.settings`.
- Не менять поведение безопасного экспорта документов.

---

### Task 1: Runtime Usage Contract

**Files:**

- Modify: `apps/desktop/src-tauri/src/domain.rs`
- Modify: `apps/desktop/src-tauri/src/runtimes/lm_studio.rs`
- Modify: `apps/desktop/src-tauri/src/runtimes/llama_server.rs`
- Modify: `apps/desktop/src-tauri/src/services/translation.rs`
- Test: existing Rust tests in the same files

**Interfaces:**

- Add `TokenUsage { input_tokens: u64, output_tokens: u64 }`.
- Extend `CompletionResponse` with `usage: Option<TokenUsage>`.
- Add `ModelRuntime::provider_id(&self) -> &'static str`.
- Extend `TranslationResult` with `provider_id`, `prompt_tokens`, `completion_tokens`, and `total_tokens`.

- [ ] Write tests for ordinary JSON and SSE responses containing `prompt_tokens`, `completion_tokens`, and `total_tokens`.
- [ ] Test missing usage and inconsistent provider totals.
- [ ] Remove the inaccurate SSE-delta token fallback.
- [ ] Add provider ids `llama.cpp` and `lm-studio`.
- [ ] Compute `total_tokens` as input plus output; use provider total only as a consistency check.
- [ ] Pass normalized usage through `translation::translate`.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml lm_studio
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml domain
```

- [ ] Commit as `Add exact token usage to runtime results`.

### Task 2: Interactive Usage Aggregates

**Files:**

- Modify: `apps/desktop/src-tauri/src/services/history.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/api.rs`
- Test: `apps/desktop/src-tauri/src/services/history.rs`

**Interfaces:**

- Add `usage_daily` to `lingoloc.sqlite`.
- Add `HistoryStore::usage_statistics()` and `HistoryStore::reset_usage_statistics()`.
- Make `HistoryStore::add()` update history and usage atomically.

Use schema version `3` and this aggregate shape:

```sql
CREATE TABLE usage_daily (
  day TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  completed_operations INTEGER NOT NULL DEFAULT 0,
  model_responses INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  measured_latency_ms INTEGER NOT NULL DEFAULT 0,
  unknown_usage_responses INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (day, provider_id, model_id)
);
```

- [ ] Add migration tests for an existing history database.
- [ ] Add tests aggregating multiple providers and models.
- [ ] Update `HistoryStore::add()` to use one SQLite transaction for history and usage.
- [ ] Count successful interactive translations as one operation and one model response.
- [ ] Count missing usage as an unknown response without adding token sums.
- [ ] Keep `clear_history` independent from usage reset.
- [ ] Ensure `translate_word` never writes usage.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml history
```

- [ ] Commit as `Persist interactive token aggregates`.

### Task 3: Extension Paragraph Operation Counting

**Files:**

- Modify: `apps/desktop/src-tauri/src/api.rs`
- Modify: `apps/extension/src/api.ts`
- Modify: `apps/extension/src/api.test.ts`
- Modify: `apps/extension/src/popup.ts`
- Test: `apps/extension/src/popupFormatting.test.ts`

**Interfaces:**

- Add optional API request field `operationStart: boolean`.
- Default `operationStart` to `true` for backward compatibility.

Example request:

```json
{
  "text": "...",
  "sourceLanguage": "en",
  "targetLanguage": "ru",
  "operationStart": true
}
```

- [ ] Add an API test for the default `operationStart` behavior.
- [ ] Extend extension `translate()` with an `operationStart` argument.
- [ ] Pass `true` only for the first paragraph of one Translate action.
- [ ] Count every successful paragraph as a model response, but only the first as a user operation.
- [ ] Do not count an operation when the first paragraph fails.
- [ ] Run:

```powershell
npm run extension:test
npm run extension:typecheck
```

- [ ] Commit as `Count extension paragraph requests as one operation`.

### Task 4: Document Block Usage

**Files:**

- Modify: `apps/desktop/src-tauri/src/documents/domain.rs`
- Modify: `apps/desktop/src-tauri/src/documents/store.rs`
- Modify: `apps/desktop/src-tauri/src/documents/worker.rs`
- Modify: `apps/desktop/src-tauri/src/documents/commands.rs`
- Test: existing document store and worker tests

**Interfaces:**

- Extend `DocumentBlock` with optional prompt tokens, completion tokens, and latency.
- Extend `DocumentJobView` and `DocumentJobSummary` with aggregate usage fields.
- Change the injected worker translator to return `TranslationResult` rather than only text.

Raise `document-jobs.sqlite` schema version from `3` to `4` and add to `document_blocks`:

```sql
prompt_tokens INTEGER,
completion_tokens INTEGER,
latency_ms INTEGER,
usage_reported INTEGER NOT NULL DEFAULT 0
```

Add a document aggregate keyed by local day, provider, model, and job id. Add `usage_operation_counted` to `document_jobs`.

- [ ] Add migration tests for existing document jobs.
- [ ] Save translated text, block usage, and the daily aggregate in one transaction.
- [ ] Skip already saved blocks on resume without counting them again.
- [ ] After all blocks finish, atomically count one document operation using `usage_operation_counted`.
- [ ] Preserve usage from successful blocks when a later block fails, but do not count the document operation.
- [ ] Keep block usage after global statistics reset so the job card remains auditable.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::worker
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::store
```

- [ ] Commit as `Track token usage for document jobs`.

### Task 5: Combined Statistics Commands

**Files:**

- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src/lib/commands.ts`
- Modify: `apps/desktop/src/lib/commands.test.ts`

**Interfaces:**

```ts
interface UsagePeriod {
  completed_operations: number;
  model_responses: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  measured_latency_ms: number;
  unknown_usage_responses: number;
}

interface UsageStatistics {
  today: UsagePeriod;
  all_time: UsagePeriod;
}
```

Add Tauri commands:

```text
get_usage_statistics
reset_usage_statistics
```

- [ ] Add Rust tests that combine interactive and document aggregates.
- [ ] Sum only exact input/output token values.
- [ ] Keep unknown response counts separate from token totals.
- [ ] Make reset clear both aggregate stores and remain idempotent.
- [ ] Register the commands in the Tauri handler.
- [ ] Add TypeScript wrappers and command mapping tests.
- [ ] Run:

```powershell
npm exec vitest run apps/desktop/src/lib/commands.test.ts
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml usage
```

- [ ] Commit as `Expose combined usage statistics`.

### Task 6: Desktop Usage UI

**Files:**

- Modify: `apps/desktop/src/lib/commands.ts`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/app/DocumentsPanel.tsx`
- Modify: `apps/desktop/src/styles.css`

- [ ] Extend `formatTiming()` to display latency, output speed, input, output, and total tokens.
- [ ] Display `token usage unavailable` when the response has no exact usage.
- [ ] Add `Settings -> Statistics` after Dictionaries and before Browser extension.
- [ ] Show `Today` and `All time` cards with operations, model responses, input, output, total, and average tok/s.
- [ ] Show exact-usage coverage such as `Exact token usage: 47 of 49 responses`.
- [ ] Add `Reset statistics...` with confirmation and reload behavior.
- [ ] Display cumulative token usage for the selected document job.
- [ ] Add tests for complete, missing, and partial usage.
- [ ] Check narrow Settings layout and mobile rendering.
- [ ] Run:

```powershell
npm exec vitest run apps/desktop/src/lib/commands.test.ts
npm exec vitest run apps/desktop/src/app/App.test.tsx
npm test
npm run typecheck
```

- [ ] Commit as `Show token statistics in desktop UI`.

### Task 7: Extension Usage UI

**Files:**

- Modify: `apps/extension/src/api.ts`
- Modify: `apps/extension/src/popup.ts`
- Modify: `apps/extension/src/popup.css`
- Modify: `apps/extension/src/api.test.ts`
- Create if needed: `apps/extension/src/usageFormatting.ts`
- Test if created: `apps/extension/src/usageFormatting.test.ts`

- [ ] Extend the extension `TranslationResult` with normalized usage fields.
- [ ] Sum latency and exact usage across paragraphs.
- [ ] Show known totals with a `partial` marker if any paragraph lacks usage.
- [ ] Render token usage on a separate line below response time.
- [ ] Add complete, missing, and partial usage tests.
- [ ] Run:

```powershell
npm run extension:test
npm run extension:typecheck
npm run extension:build
```

- [ ] Commit as `Show token usage in browser extension`.

### Task 8: Documentation And Final Verification

**Files:**

- Modify: `README.md`
- Modify: `README.ru.md`
- Modify: `docs/LOCAL_API.md`
- Modify: `DECISIONS.md`
- Modify: `TODO.md`

- [ ] Document Statistics in both README files with matching content.
- [ ] Document the API response and `operationStart` in `docs/LOCAL_API.md`.
- [ ] Record aggregate-only storage and provider/model dimensions in `DECISIONS.md`.
- [ ] State that pricing, cached tokens, reasoning tokens, and public providers are future work.
- [ ] Run formatting on changed files.
- [ ] Run the full verification gate:

```powershell
npm run check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
npm run extension:build
```

- [ ] Manually smoke-test Text, clipboard popup, multi-paragraph extension translation, document pause/resume, and statistics reset.
- [ ] Commit as `Document token usage statistics`.

## Self-Review Checklist

- Runtime usage is exact and normalized for future providers.
- Interactive history and document blocks update usage atomically.
- Resume cannot double-count blocks or completed document operations.
- Extension paragraphs count as one user operation.
- Word translation is excluded.
- Today and All time combine both SQLite stores.
- Reset does not delete history or job-level usage.
- Desktop and extension display missing or partial usage honestly.
- README files remain synchronized.
