# LingvoLoc Implementation Plan

## Scope

This plan covers Phase 0 (project bootstrap) and Phase 1 (the first end-to-end translation slice) from the technical specification. History, lexical data, browser integration, and the embedded llama.cpp runtime are later phases.

## Confirmed Decisions

- Working product name: **LingvoLoc**.
- Primary platform: Windows 10/11 x64.
- Desktop stack: Tauri 2, React, TypeScript, Vite, and Rust.
- Workspace tooling: npm workspaces.
- Initial runtime: LM Studio through its local OpenAI-compatible API.
- Initial model family: TranslateGemma.
- Repository visibility: private.
- License: none until a separate decision is made.
- GitHub status checked on 2026-09-26: the former working-name repository was private and no exact public `LingvoLoc` repository was found. Availability is not a reservation.

## Verified Development Environment

- Node.js 22.14.0 and npm 10.9.2 are installed.
- Rust 1.98.1 and Cargo 1.98.1 are installed.
- NVIDIA GeForce RTX 3060 with 12 GB VRAM is available.
- LM Studio responds at `http://127.0.0.1:1234/v1`.
- LM Studio currently exposes TranslateGemma 12B, 4B F16, and 4B Q8_0 models.
- MSVC Build Tools were not found in `PATH`; verify the Tauri Windows prerequisites before building.

## Initial Repository Structure

```text
/
|-- apps/
|   `-- desktop/
|       |-- src/
|       |   |-- app/
|       |   |-- features/
|       |   |   |-- models/
|       |   |   `-- translation/
|       |   `-- lib/
|       `-- src-tauri/
|           `-- src/
|               |-- adapters/
|               |   `-- translategemma.rs
|               |-- commands/
|               |-- domain/
|               |-- runtimes/
|               |   `-- lm_studio.rs
|               `-- services/
|-- packages/
|   `-- protocol/
|-- docs/
|-- Cargo.toml
`-- package.json
```

Do not create empty packages for the lexical engine, extension, shared UI, or llama.cpp before their implementation phases require them.

## Phase 0: Bootstrap

1. Update project documentation to use the LingvoLoc name and record the agreed architecture.
2. Create the private `thejohnd0e/LingvoLoc` GitHub repository and add it as `origin`.
3. Rename the default branch from `master` to `main`.
4. Verify or install the Tauri Windows prerequisites: MSVC Build Tools with Desktop development with C++, Windows SDK, and WebView2.
5. Configure npm workspaces and root commands for development, build, test, lint, formatting, and type checking.
6. Scaffold the Tauri 2 desktop application with React, strict TypeScript, and Vite.
7. Configure ESLint, Prettier, Rustfmt, and Clippy.
8. Add Vitest and React Testing Library; run Rust tests through Cargo.
9. Add minimal settings persistence for the LM Studio endpoint, selected model, selected adapter, and language pair.
10. Add a Windows GitHub Actions workflow for install, lint, type checking, unit tests, and build.
11. Replace remaining setup, usage, build, and test placeholders with verified commands.

## Phase 1: Translation Slice

### 1. Validate LM Studio Integration

- Verify `GET /v1/models` against the running local server.
- Determine the exact `POST /v1/chat/completions` payload accepted for TranslateGemma.
- Confirm whether LM Studio accepts TranslateGemma's structured content containing `source_lang_code` and `target_lang_code`.
- Test the 4B Q8_0 model and at least one additional installed variant.
- Record response and error shapes without logging complete user text.

This spike must happen before finalizing the adapter request builder because TranslateGemma requires an opinionated chat template.

### 2. Define Runtime-Neutral Contracts

Define normalized domain types and Rust traits for:

- `ModelRuntime`;
- `TranslationModelAdapter`;
- `TranslationRequest`;
- `TranslationResult`;
- `LocalModel`;
- runtime status and normalized errors.

The UI must depend only on normalized commands and DTOs, not LM Studio or TranslateGemma implementation details.

### 3. Implement LM Studio Runtime

- Check endpoint availability.
- Enumerate models without assuming a particular model ID.
- Submit generation requests with timeouts.
- Normalize connection, timeout, HTTP, and malformed-response failures.
- Read the endpoint from persisted settings.

### 4. Implement TranslateGemma Adapter

- Build requests using explicit source and target language codes.
- Keep the model-specific chat template inside the adapter.
- Use deterministic or near-deterministic generation settings.
- Normalize output by removing unwanted prefixes, surrounding quotes, and Markdown wrappers where appropriate.
- Unit-test request building and response parsing.

Do not detect model families using checks such as `modelId.includes("gemma")`. Phase 1 will persist an explicit `modelId -> adapterId` selection when LM Studio metadata cannot identify the adapter reliably.

### 5. Implement Translation Service and Tauri Commands

The translation service selects the configured runtime and adapter, builds a runtime request, executes it, and returns a normalized result.

Expose commands equivalent to:

- `get_runtime_status`;
- `list_models`;
- `translate`;
- `get_settings`;
- `update_settings`.

Validate every IPC input at the Rust boundary.

### 6. Build the Initial Desktop UI

- Two-panel source and translation layout.
- Manual source-language selection.
- Target-language selection and language swapping.
- Model selection and explicit adapter selection where needed.
- Translate button and `Ctrl+Enter` shortcut.
- Clear and copy actions.
- Loading, empty, success, and error states.
- Status display for model, runtime, and latency.

Automatic language detection is not part of Phase 1 and must not be simulated with the translation model.

## Verification

- Unit tests for TranslateGemma request building and response normalization.
- Unit tests for normalized error mapping.
- Mock HTTP tests for the LM Studio runtime.
- React tests for the main UI states and actions.
- `cargo test`, `cargo fmt --check`, and Clippy.
- TypeScript lint, formatting check, type checking, and tests.
- Production desktop build on Windows.
- Manual smoke test against the running LM Studio instance.

## Phase 1 Acceptance Criteria

The following workflow must work reliably:

```text
LingvoLoc Desktop
-> list available models from LM Studio
-> explicitly select a TranslateGemma model and adapter
-> select source and target languages
-> submit text
-> receive a local translation
-> display runtime, model, latency, and useful errors
-> copy the result
```

Switching between `translategemma-4b-it@f16` and `translategemma-4b-it@q8_0` must not require UI or adapter code changes.

## Later Roadmap

1. Model Manager and quantization metadata.
2. Local language detection.
3. SQLite history, favorites, and search.
4. System tray, global hotkeys, and clipboard popup.
5. Lexical MVP using a documented, legally compatible local dataset.
6. Sense-aware and context-aware lexical tools.
7. Authenticated localhost API and Chromium extension.
8. llama.cpp, GGUF discovery, CUDA offload, and CPU fallback.
9. Hardware detection, memory estimates, and benchmarks.
10. Installer, diagnostics, migrations, accessibility, and product hardening.

## Explicitly Deferred

- Translation history and SQLite.
- Language auto-detection.
- Dictionary and lexical datasets.
- Browser extension and localhost integration API.
- llama.cpp and direct GGUF loading.
- Hardware management and benchmarking.
- Global hotkeys, clipboard monitoring, and system tray.
- Installer and update mechanism.
