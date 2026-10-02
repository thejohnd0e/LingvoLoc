# Simplified Token Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Показывать точные токены текущего запроса без общей статистики и постоянного хранения.

**Architecture:** Runtime возвращает нормализованный `TokenUsage`. Интерактивные интерфейсы отображают его непосредственно из ответа. Для документов последнее usage временно хранится в памяти процесса.

**Tech Stack:** Rust, Tauri 2, React, strict TypeScript, Vitest, Chromium MV3 extension.

**Spec:** Approved in-chat simplified design from 2026-10-02.

## Global Constraints

- Учитывать только точные данные provider; не оценивать токены по символам или SSE chunks.
- Не добавлять SQLite-миграции, дневные или общие агрегаты.
- Не добавлять Settings Statistics, reset, стоимость или историю usage.
- Поддержать основное окно, clipboard popup, extension и document progress.
- Контракт должен оставаться пригодным для будущих публичных LLM API.
- Сохранять совместимые идентификаторы и существующее поведение безопасного экспорта документов.

---

### Task 1: Runtime Usage Contract

**Files:**
- Modify: `apps/desktop/src-tauri/src/domain.rs`
- Modify: `apps/desktop/src-tauri/src/runtimes/lm_studio.rs`
- Modify: `apps/desktop/src-tauri/src/services/translation.rs`
- Test: existing Rust tests in the same files

**Interfaces:**

```rust
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub struct CompletionResponse {
    pub model: String,
    pub content: String,
    pub usage: Option<TokenUsage>,
}
```

Extend `TranslationResult` with:

```rust
pub prompt_tokens: Option<u64>,
pub completion_tokens: Option<u64>,
pub total_tokens: Option<u64>,
```

- [ ] Add failing tests for ordinary JSON and SSE responses with complete usage.
- [ ] Add a test for a response without usage.
- [ ] Read `prompt_tokens` and `completion_tokens` from provider usage.
- [ ] Compute total as input plus output.
- [ ] Remove the inaccurate SSE delta-count fallback.
- [ ] Pass usage through `translation::translate`.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml lm_studio
```

- [ ] Commit as `Add exact token usage to runtime results`.

### Task 2: Desktop And Clipboard Display

**Files:**
- Modify: `apps/desktop/src/lib/commands.ts`
- Modify: `apps/desktop/src/lib/commands.test.ts`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/app/ClipboardPopup.tsx`
- Modify: `apps/desktop/src/app/ClipboardPopup.test.tsx`

Display complete usage as:

```text
3779 ms · 30.7 tok/s · 214 in · 118 out · 332 total
```

Display missing usage as:

```text
3779 ms · token usage unavailable
```

- [ ] Extend the TypeScript `TranslationResult` type.
- [ ] Add formatter tests for complete and unavailable usage.
- [ ] Update `formatTiming()` without adding persistent statistics state.
- [ ] Reuse the formatter in the main window and clipboard popup.
- [ ] Run:

```powershell
npm exec vitest run apps/desktop/src/lib/commands.test.ts
npm exec vitest run apps/desktop/src/app/App.test.tsx
npm exec vitest run apps/desktop/src/app/ClipboardPopup.test.tsx
```

- [ ] Commit as `Show token usage in desktop translations`.

### Task 3: Extension Display

**Files:**
- Modify: `apps/extension/src/api.ts`
- Modify: `apps/extension/src/api.test.ts`
- Modify: `apps/extension/src/popup.ts`
- Modify: `apps/extension/src/popup.css`
- Modify: `apps/extension/src/popupFormatting.test.ts`

- [ ] Extend the extension `TranslationResult` with token fields.
- [ ] Accumulate input and output usage only for the current Translate action.
- [ ] Show the summed usage below response time.
- [ ] Show a `partial` marker when only some paragraphs provide usage.
- [ ] Show `Token usage unavailable` when no paragraph provides usage.
- [ ] Clear the previous total when a new translation starts.
- [ ] Add tests for complete, partial, and unavailable usage.
- [ ] Run:

```powershell
npm run extension:test
npm run extension:typecheck
npm run extension:build
```

- [ ] Commit as `Show token usage in browser extension`.

### Task 4: Transient Document Request Usage

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/documents/domain.rs`
- Modify: `apps/desktop/src-tauri/src/documents/worker.rs`
- Modify: `apps/desktop/src-tauri/src/documents/commands.rs`
- Modify: `apps/desktop/src/lib/commands.ts`
- Modify: `apps/desktop/src/app/DocumentsPanel.tsx`
- Test: corresponding Rust and React tests

Use process-memory state only:

```rust
pub struct RequestTokenCounts {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}
```

- [ ] Add a transient `job_id -> RequestTokenCounts` map to `AppState`.
- [ ] Change the document translator to retain `TranslationResult` instead of only its text.
- [ ] Update the map after every successful block.
- [ ] Add `last_request_usage` to `DocumentJobSummary`.
- [ ] Return the transient value from `get_document_progress`.
- [ ] Clear the value when a job starts or is deleted.
- [ ] Do not write usage to `document-jobs.sqlite`.
- [ ] Show this value in `DocumentsPanel`:

```text
Last request: 186 in · 94 out · 280 total
```

- [ ] Verify that the value disappears after application restart.
- [ ] Test updates after each block and absence for a new process state.
- [ ] Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml documents::worker
npm exec vitest run apps/desktop/src/app/App.test.tsx
```

- [ ] Commit as `Show last document request token usage`.

### Task 5: Documentation And Final Gate

**Files:**
- Modify: `README.md`
- Modify: `README.ru.md`
- Modify: `docs/LOCAL_API.md`

- [ ] Document per-request token display in both README files.
- [ ] Document the new response fields in `docs/LOCAL_API.md`.
- [ ] Explicitly state that usage is not persisted and no aggregate statistics exist yet.
- [ ] Run the final checks:

```powershell
npm run check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
npm run extension:build
```

- [ ] Manually verify Text, clipboard popup, multi-paragraph extension translation, and a multi-block document.
- [ ] Commit as `Document per-request token display`.

## Self-Review Checklist

- Runtime accepts only exact provider usage.
- Main window and clipboard popup show current request usage.
- Extension sums only the current action in memory.
- Documents show only the last completed block request.
- No usage is written to SQLite or local storage.
- Missing and partial usage are shown honestly.
- The expanded aggregate plan remains deferred in `2026-10-02-token-usage-statistics.md`.
