# LingvoLoc

**English** · [Русский](README.ru.md)

![License MIT](https://img.shields.io/badge/license-MIT-blue)
![Platform Windows](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d4?logo=windows&logoColor=white)
![Local or cloud](https://img.shields.io/badge/models-local%20%7C%20cloud%20%7C%20subscription-2ea44f)
![Tauri 2](https://img.shields.io/badge/Tauri-2-24c8db?logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-b7410e?logo=rust&logoColor=white)
![React](https://img.shields.io/badge/React-TypeScript-3178c6?logo=react&logoColor=white)
![llama.cpp](https://img.shields.io/badge/engine-llama.cpp-6e5494)
![Documents](https://img.shields.io/badge/files-TXT%20%7C%20DOCX%20%7C%20EPUB%20%7C%20FB2%20%7C%20PDF-8a5cf5)
![Chrome extension](https://img.shields.io/badge/extension-Chromium%20MV3-4285f4?logo=googlechrome&logoColor=white)

LingvoLoc is a Windows app for translation with the model you choose. It works both with **local models** - completely private, offline, with no account or subscription - and with **any cloud AI provider**:

- OpenAI
- Anthropic
- Google Gemini
- DeepL
- DeepSeek
- OpenRouter
- xAI Grok
- any OpenAI-compatible endpoint
- ChatGPT Plus/Pro and SuperGrok subscriptions (sign in through the browser, no API key)

With a local model everything - text, documents, dictionary lookups - is processed on your computer and nothing is sent to the internet.

![LingvoLoc Text mode](docs/screenshot-text.png)

![LingvoLoc Files mode](docs/screenshot-files.png)

## What LingvoLoc can do

- **Translate text.** Type or paste text, pick the languages (automatic source detection for 12 languages), and get a translation with the response time, exact provider token usage, and speed. The result is copied to the clipboard.
- **Translate whole documents.** Drop TXT, DOCX, EPUB, FB2 and **PDF** files on the window. Translation of a book keeps going in the background, survives restarts, and can be paused, resumed, or cancelled. Several files can be translated in a row.
- **Translate PDF books and keep their layout.** Headings, columns, tables, tables of contents, coloured panels, lists and pictures stay where they are; only the text is replaced (details below).
- **Translate from the clipboard.** Press `Ctrl+Shift+T` anywhere in Windows and a small window shows the translation of what you copied.
- **Translate in the browser.** A Chromium extension translates selected text on any web page through the desktop app.
- **Look up words in your own dictionaries.** Point LingvoLoc at a folder of StarDict dictionaries; articles are shown as separate coloured cards with pronunciation, examples, and audio.
- **Use the model you prefer - local or cloud.** LingvoLoc runs `.gguf` models (TranslateGemma, Gemma 3, Hunyuan-MT, Qwen and others) through its own built-in llama.cpp, or connects to LM Studio. Or switch to a cloud provider in one click: OpenAI, Anthropic, Gemini, DeepL, DeepSeek, OpenRouter, xAI Grok, or any OpenAI-compatible endpoint.
- **Use your subscription.** Sign in with your **ChatGPT Plus/Pro** or **SuperGrok** account in the browser and translate with the models included in your plan, without an API key (see Cloud providers and subscriptions below).
- **Stay in the background.** Closing the window hides LingvoLoc in the tray; it can start with Windows.

## System requirements

Cloud providers and subscriptions need only Windows and an internet connection; the disk, memory and graphics card items below are for local models.

- **Windows 10 or 11, 64-bit.** The installer adds the WebView2 runtime if it is missing (an internet connection is needed for that one step).
- **Disk space:** under 100 MB for the app, 0.2-1 GB for llama.cpp (downloaded from Settings), and 4-9 GB per translation model (`.gguf` file; see the table below).
- **Memory:** the model must fit in video memory (GPU) or in RAM (CPU). Count the file size plus 1-2 GB. A 12 GB video card runs the recommended 12B models comfortably (tested on an RTX 3060 12 GB); 6-8 GB cards fit TranslateGemma 4B or smaller quantizations of the 12B models; without a GPU, 16 GB of RAM or more is advisable and translation is several times slower.
- **Graphics card (optional but strongly recommended):** NVIDIA (CUDA 12 build), any other GPU with Vulkan support (AMD, Intel), or no GPU at all (CPU build). LingvoLoc picks the build automatically.
- **Internet:** to download llama.cpp, models, and the installer itself, and whenever you use a cloud provider or a subscription. Translation with a local model works offline.
- **Browser extension:** Chrome, Edge, or another Chromium-based browser (Manifest V3).
- **PDF:** text PDFs of up to 3000 pages per file; scanned (image-only) PDFs are not supported.

## Install

Download the latest `LingvoLoc_x.y.z_x64-setup.exe` from the [Releases](../../releases) page and run it. The installer checks for the components LingvoLoc needs (for example the WebView2 runtime) and installs anything that is missing. You can install for the current user or for all users.

## First run

LingvoLoc works out of the box in **Standalone** mode and needs no other software:

1. Open **Settings** (gear icon, top right) and press **Download llama.cpp**. LingvoLoc picks the build that fits your computer (CUDA for NVIDIA, Vulkan for other GPUs, CPU otherwise), downloads it, and checks it.
2. Choose the **Models folder** containing your `.gguf` files.
3. Pick a model in the main window and translate.

The **GPU support** row shows your video card(s), which llama.cpp build will be downloaded, and, once it is installed, which device is actually used. With several GPUs (for example a discrete card and an integrated one) LingvoLoc pins the strongest one. Pressing **Update llama.cpp** later replaces the installed copy in place and tells you when the newest release is already installed.

The first translation is slower while the model loads. **Add to PATH** puts the llama.cpp folder into your user PATH (no administrator rights needed) so you can also run it from a terminal. **Advanced** lets you point to an existing llama.cpp folder or find one already in PATH.

Prefer LM Studio? Switch **Mode** to `LM Studio` and start its OpenAI-compatible server at `http://127.0.0.1:1234/v1`.

Prefer the cloud? Switch **Mode** to a provider and paste an API key, or sign in with your ChatGPT Plus/Pro or SuperGrok account - no download and no GPU needed. See the next section.

## Cloud providers and subscriptions

Besides local models, LingvoLoc can translate with cloud services. Choose one in **Settings → Model runtime → Mode**. Local and cloud modes are interchangeable: the same text, files, clipboard and browser-extension translation work with whichever mode is selected, and you can switch at any time.

| Mode                 | How you connect                                          | Notes                                                                                          |
| -------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| OpenAI               | API key                                                  | Models are loaded from your account.                                                           |
| Anthropic            | API key                                                  | Claude models.                                                                                 |
| Google Gemini        | API key                                                  | Text models only; Flash models are listed first.                                               |
| DeepL                | API key (Free or Pro plan)                               | Reports billed characters instead of tokens.                                                   |
| DeepSeek             | API key                                                  | `deepseek-chat` and `deepseek-reasoner`.                                                       |
| OpenRouter           | API key                                                  | Hundreds of models; those with `free` in the id are highlighted with ★.                        |
| xAI Grok             | API key from the xAI console                             | Grok models.                                                                                   |
| OpenAI-compatible    | API key and **Endpoint** (for example `https://host/v1`) | Any service with an OpenAI-style API. Plain `http://` is accepted only for local addresses.    |
| **ChatGPT Plus/Pro** | **Sign in with your ChatGPT account in the browser**     | Uses your plan allowance instead of an API key. Official OpenAI sign-in for open-source apps.  |
| **SuperGrok**        | **Sign in with your xAI account (device code)**          | Uses your SuperGrok / X Premium+ subscription. Unofficial: xAI may refuse some accounts (403). |

**Connecting with an API key.** Choose the provider, paste the key and press **Save key**. Keys are stored in Windows Credential Manager, never in settings files; once saved, the field shows a masked value and **Saved securely**. Press **Refresh models**, pick a model (or type its id), and tick the consent notice. Models whose id contains `free` are highlighted with ★ in every model list.

**Using a subscription.** There is no key to paste:

- **ChatGPT Plus/Pro.** Press **Sign in with ChatGPT**; your browser opens OpenAI's consent page ("Use your ChatGPT plan"). After you approve, LingvoLoc shows **Signed in as your@email**, loads the models available to your plan, and translates with them. Requests count against your ChatGPT plan limits; set a weekly cap for LingvoLoc in ChatGPT under **Settings → Usage**. The sign-in uses OpenAI's official "Sign in with ChatGPT" for open-source apps and is meant for personal use. It listens on `127.0.0.1:47835` for the return from the browser, so that port must be free. **Sign out** revokes the access.
- **SuperGrok.** Press **Sign in with SuperGrok**; LingvoLoc opens xAI's page and shows a short code - approve it there. xAI does not document subscription sign-in for third-party apps, so this is unofficial: it may stop working, or xAI may refuse your account (HTTP 403). The consent screen may name the shared Grok client.

In both cases only a small session record is kept in Credential Manager; access tokens live in memory and are refreshed automatically.

**Proxy.** Every cloud provider has its own optional **Proxy** field (`http://host:port`, `https://…` or `socks5://host:port`). It is used for that provider's requests and sign-in, which helps where a service is blocked in your region. Leave it empty to connect directly.

**What to expect.** Press **Cancel** while a translation is running to stop waiting. If a provider does not start answering within 30 seconds the request fails instead of hanging. For OpenAI-style gateways (DeepSeek, OpenRouter, xAI Grok, OpenAI-compatible) the provider's own message is shown with the error, for example "No endpoints found for this model". Token usage is shown when the provider reports it, and Settings aggregates usage for the current session. Cloud providers receive the text you translate; use a local model when the text must not leave your computer.

## Choosing a model

This section is about local `.gguf` models; for cloud models see the section above.

Names containing `gemma` use the TranslateGemma prompt, `hunyuan` uses Hunyuan-MT's official prompt, and any other instruction-tuned model (for example Qwen) uses a generic translator prompt, so the file name matters. After each translation the status line shows the response time and generation speed (`3779 ms · 30.7 tok/s`); the first request also includes model loading, so its speed is lower.

Benchmark on an RTX 3060 12 GB (llama.cpp, CUDA, 8K context, all layers on the GPU; 12 short and long texts covering idioms, technical, medical and legal wording, and a five-paragraph news article; en, ru, de and zh). Quality was judged by reading the output, without a reference metric, so treat it as a guide rather than a formal benchmark.

| Model                     | Size   | Speed     | Notes                                                                                                                    |
| ------------------------- | ------ | --------- | ------------------------------------------------------------------------------------------------------------------------ |
| TranslateGemma 4B Q8_0    | 3.9 GB | ~55 tok/s | Fastest; good for short text, occasional wording errors.                                                                 |
| TranslateGemma 12B Q4_K_S | 6.5 GB | ~34 tok/s | Good quality and speed.                                                                                                  |
| TranslateGemma 12B Q4_K_M | 6.8 GB | ~33 tok/s | No better than Q4_K_S; not worth it.                                                                                     |
| TranslateGemma 12B Q5_K_M | 7.9 GB | ~26 tok/s | **Best balance:** close to Q6_K, about 25% faster.                                                                       |
| TranslateGemma 12B Q6_K   | 9.0 GB | ~20 tok/s | Best wording, but 70% slower than Q4_K_S.                                                                                |
| Gemma 3 12B QAT Q4_0      | 6.4 GB | ~34 tok/s | Quality on par with TranslateGemma 12B; the QAT 4-bit build loses almost nothing, so larger quantizations are pointless. |
| Hunyuan-MT-7B Q6_K        | 5.7 GB | ~42 tok/s | Wordy for Russian, merged paragraphs and added details in a long text; better suited to Asian languages.                 |
| Qwen3-14B Q4_K_M          | 8.4 GB | ~32 tok/s | Weakest for Russian: invented idioms and stray non-Cyrillic characters.                                                  |

Recommendations:

- **Russian and other European languages:** TranslateGemma 12B Q5_K_M, or Gemma 3 12B QAT Q4_0 if you want a smaller and faster model.
- **Maximum speed on short text:** TranslateGemma 4B Q8_0.
- Avoid "abliterated" fine-tunes: they drop trailing paragraphs of long texts.

Exact provider usage is shown alongside response time when available: `214 in · 118 out · 332 total`. DeepL reports billed characters instead. If the provider does not report usage, LingvoLoc says `token usage unavailable` rather than estimating tokens. Cloud usage is aggregated for the current app session in Settings; keys are stored in Windows Credential Manager and never in settings files.

## Translating files

Switch to **Files** in the header (or press `Ctrl+2`; `Ctrl+1` returns to text). Both views stay loaded, so a file keeps translating while you use Text mode, and the Files tab shows its progress.

- **Adding files.** Press **Choose…** (several files at once are fine) or drop files anywhere on the window. Supported formats: TXT, DOCX, EPUB, plain FB2, and text PDF. Each file becomes its own job; the source language is detected automatically and the target follows your main language pair.
- **Jobs and queue.** **Recent jobs** lists your latest jobs with status and progress. Open a job to see it, remove it, or use **Clear finished**. When two or more files are ready, **Translate N ready files in a row** translates them one after another and saves each result next to its source as `<name>.translated.<lang>.<ext>`. The queue never overwrites an existing file; a pause, cancel, or error stops it.
- **Progress and saving.** The card shows the blocks translated, a time estimate, and finally "Translated N / N blocks in <time>". Single files are saved with **Export translated…** to a place you choose; if you pick an existing file in the save dialog and confirm, it is replaced. Jobs survive restarts and can be resumed.
- **Warnings.** Content that cannot be translated safely (for example SVG with text, unresolved FB2 entities) is left unchanged and listed under the job. Paragraphs where italics or links may shift are summarized in one line.
- **Formats.** DOCX and EPUB keep their package structure, styles, pictures, and links; only text of supported paragraphs, headings, list items, and table cells is replaced. FB2 headings, paragraphs, epigraphs, and notes are translated while metadata, links, images, and embedded resources are preserved (archived `.fb2.zip` is not supported). TXT supports UTF-8 and UTF-16 with a BOM.
- **A stalled model.** A model request that is silent for 45 seconds is treated as stalled; in Standalone mode the local server is restarted and the request is retried once. Diagnostic events are written to `lingvoloc-document-worker.log` in your temporary folder (`%TEMP%`).

### Translating PDF books

LingvoLoc translates **text PDFs in place**: the original text is removed and the translation is drawn in the same place, so the translated book looks like the original.

- **What is kept.** Pictures, backgrounds, coloured panels (tips, warnings, case studies), table borders and shading, lines, page decorations, and code blocks stay untouched. Headings keep their size and weight; centred and right-aligned text stays centred or right-aligned; list bullets and numbers move together with their item; table cells, table-of-contents rows, captions, and several-column pages are translated cell by cell and column by column, never mixed together.
- **Fitting longer text.** Translations are usually longer than the original. LingvoLoc uses free space below a paragraph first, then tighter paragraph spacing and line spacing, and only then a smaller font. Similar cells and rows (for example a whole table or table of contents) share one font size so they look even. A paragraph never grows over a picture, a panel edge, a table line, or the next paragraph.
- **Paragraphs split across pages** are joined and translated as one piece, so sentences are not cut in half.
- **Fonts.** The translation uses Noto Sans and Noto Serif (bundled, SIL Open Font License) with full Latin and Cyrillic coverage, so the result does not depend on fonts installed on your computer.
- **Page range (recommended for books).** The **PDF pages** field accepts ranges such as `5-12, 20, 30-`. It is read **when the file is added**: only those pages are translated and saved, to `<name>.translated.<lang>.p5-12.pdf`. This is the quick way to check the result on a few pages before translating a whole book. If you change the range afterwards, press **Start translation** once: LingvoLoc analyzes the file again with the new range and asks you to press **Start translation** again, so the wrong range is never translated by accident.
- **Left unchanged.** Code, page numbers, short all-caps labels, scanned or image-only pages, text that is part of a picture, and rotated pages are not translated; they are listed as notes. Pages where the translation had to be shrunk a lot are listed so you can review them.
- **Saving.** **Export translated…** writes a new PDF; choosing an existing file in the save dialog and confirming replaces it. The original PDF is never changed.
- **Requirements.** PDF support uses `pdfium.dll` (bundled in the installer, in the `pdfium` folder next to the program).

## Text translation and the clipboard

The status pill in the header shows the runtime and the selected model. LingvoLoc saves your endpoint, model, language selections, two independently chosen main languages, and text-size settings. Automatic source detection is on by default; successful translations are copied to the clipboard when Windows allows it. Double-click a word in the source or the result to highlight its counterpart in the other text and open a dictionary lookup.

The **Style** selector applies one of four translation presets globally: **Neutral**, **Literary**, **Technical**, or **Conversational**. The choice is used for Text, Files, clipboard translation, and the browser extension. Word translation used for dictionary highlighting and reverse alignment always stays neutral. A document job keeps the style selected when it was analyzed, even if you change the global setting while it is paused or running; analyze the file again to use another style. The result still depends on the selected model, so presets do not guarantee terminology or a particular quality level.

The clipboard popup (`Ctrl+Shift+T`) has its own text-size setting, can be resized, and fits its height to its content.

## Dictionaries

In **Settings → Dictionaries** choose the folder that contains your StarDict dictionary folders and enable the ones to search. Extract each dictionary archive into its own subfolder and keep `.ifo`, `.idx` or `.idx.gz`, and `.dict` or `.dict.dz`; media files (`.wav`, `.mp3`, `.ogg`, `.flac`, `.m4a`, `.aac`, images) can lie beside the dictionary or inside its `res.zip`. LingvoLoc ships no dictionaries of its own and never replaces a missing entry with generated text. Articles from different dictionaries are shown as separate cards labelled with the dictionary name, colour-coded for the dark theme (pronunciation, part of speech, labels, examples, cross references); the repeated headword and sense markers such as `< I >` are hidden. Audio and images load on demand.

## Settings

The **Settings** window (gear icon) has runtime, cloud provider, dictionary, and browser extension sections:

- **Model runtime**: Standalone or LM Studio mode, models folder, GPU support, and llama.cpp.
- **Cloud provider**: the provider, its model list (with free models highlighted), API key or **Sign in** button for ChatGPT Plus/Pro and SuperGrok, the **Endpoint** for OpenAI-compatible mode, an optional per-provider **Proxy**, the consent notice, and usage of the current session. See Cloud providers and subscriptions.
- **Dictionaries**: the StarDict folder and the dictionaries to search.
- **Browser extension**: **Copy token** copies the pairing token for the browser extension.

## Tray icon

Left-click the tray icon to show or hide the main window. Right-click opens the menu:

- **Open LingvoLoc** (`Ctrl+Shift+L`)
- **Translate clipboard** (`Ctrl+Shift+T`): translates the clipboard text and shows it in a small popup window.
- **Settings…**: opens the main window with the settings.
- **Start with Windows**: starts LingvoLoc hidden in the tray when you sign in.
- **Quit**

## Browser extension

The Chromium extension (`LingvoLoc-extension-x.y.z.zip` on the Releases page) translates selected text through the desktop app. It uses whatever runtime, provider and model is selected in the desktop app, local or cloud. Load it from `chrome://extensions` with Developer mode enabled, press **Copy token** in LingvoLoc Settings, and paste the token into the extension. After the desktop app restarts, pair it again.

Select text and use the toolbar button or the right-click menu **Translate selection with LingvoLoc**. The toolbar button opens and fills the in-page window without translating; the context-menu action opens it and translates automatically using the saved target language. While a translation is running, the result area shows a clear `Translating...` indicator. Translation paragraphs use a first-line indent for easier reading. Click **Original text** to collapse or expand the source field; that state is remembered. Use **Clear** to remove both fields, or drag the divider between them to change and remember their relative heights. The window can be dragged by its header and resized by its right edge, bottom edge, or corner; the size is remembered. The **⧉** button opens the content in a separate browser window that stays open when you switch tabs; on pages where extensions cannot inject content (such as `chrome://` pages) that window opens instead. See `docs/BROWSER_EXTENSION.md` for details.

## Privacy

With a local model (Standalone or LM Studio), translation, documents, dictionaries, and history stay on your computer; the only network access is the optional download of llama.cpp from GitHub. Nothing is sent to a cloud provider or a subscription service until you select such a mode and accept the notice in Settings. API keys and sign-in sessions are kept in Windows Credential Manager, never in settings files. The extension talks only to the desktop app on `127.0.0.1` with a pairing token.

The project is licensed under MIT; see `LICENSE`.

---

# Technical description

The sections below are for contributors and for readers who want to know how LingvoLoc works.

## Architecture

- **Desktop app** (`apps/desktop`): React, Vite and strict TypeScript frontend (`src`), Tauri 2 shell and Rust backend (`src-tauri/src`) with native commands, runtimes, model adapters, history, language detection, lexical services, and the document subsystem. npm workspaces, Windows first.
- **Browser extension** (`apps/extension`): a thin Chromium MV3 client. It owns no model or data and uses the authenticated loopback API at `127.0.0.1:47831` (per-process bearer token, restricted CORS); see `docs/LOCAL_API.md`.
- **Runtimes.** `ModelRuntime` implementations: `standalone` (spawns `llama-server.exe` on a free loopback port with a `.gguf` chosen from the models folder; llama.cpp is downloaded by `runtimes/llama_download.rs`, picking the CUDA 12, Vulkan or CPU archive from the detected GPU) and `lmStudio` (OpenAI-compatible API at `http://127.0.0.1:1234/v1`). One server process is kept and restarted when the model changes; it is killed on exit.
- **Adapters.** `TranslationModelAdapter` is chosen from the model file name (`adapters::Family`): `gemma` → TranslateGemma, `hunyuan` → Hunyuan-MT, otherwise a generic chat translator (Qwen3 gets `/no_think` and `enable_thinking:false`). `llama-server` receives matching template flags.
- **Cloud backends.** `TranslationBackend` implementations in `backends/`: `openai`, `anthropic`, `gemini`, `deepl`, `openai_compatible` (also serves DeepSeek, OpenRouter and xAI Grok through `for_provider`, with fixed endpoints) and `chatgpt` (the Responses API, used for ChatGPT Plus/Pro and for SuperGrok against `api.x.ai`). The OpenAI-compatible family and Gemini use blocking non-streamed requests on a helper thread that the Cancel button can abandon; OpenAI and Anthropic still stream with an idle timeout; the Responses API backends read the SSE stream with the blocking client. Waiting for response headers is capped at 30 s, and for OpenAI-style gateways the provider's `error.message` is appended to HTTP errors. `HttpTransport` (`backends/http.rs`) can route each provider through its own `http(s)`/`socks5` proxy (`cloud.<provider>.proxyUrl`). API keys live in Windows Credential Manager (`ProviderId::service_name`).
- **Subscription sign-in.** `services/chatgpt_auth.rs` implements OpenAI's "Sign in with ChatGPT" plan usage for open-source apps (authorization code + PKCE at `auth.openai.com`, loopback redirect on `127.0.0.1:47835`, dynamic client, persistent host id, `resource=https://api.openai.com/v1`); `services/supergrok_auth.rs` implements the unofficial xAI device-code flow at `auth.x.ai`. Both keep only a small session record (refresh token, e-mail) in Credential Manager and cache access tokens in memory, refreshed under a lock because refresh tokens rotate. See `DECISIONS.md` for the exact scopes and constraints.
- **Inference coordinator.** All entry points (UI, API, clipboard, document blocks) share one in-process coordinator: one blocking model request at a time, interactive requests first, document callers acquire the slot per block. Lifecycle changes quiesce it. Local-runtime chat completions stream (`stream: true`) on a dedicated thread with a 45 s idle timeout and a 900 s cap; Standalone restarts the server and retries once. Events go to `%TEMP%\lingvoloc-document-worker.log`.
- **History** is SQLite (`lingoloc.sqlite`, schema versioned with `PRAGMA user_version`, 1,000 non-favourite rows kept). Settings live in `lingoloc.settings`. These legacy names and the bundle identifier `com.lingoloc.desktop` are intentionally unchanged so existing installs keep their data.
- **Lexical lookup** reads only user-selected StarDict folders (parsed folders are cached by path and file fingerprint); there is no bundled or app-data dictionary fallback. `scripts/build-lexical-index.mjs` is an offline converter and is not part of runtime lookup.

## Document jobs

Documents run as recoverable jobs stored in `document-jobs.sqlite` (app data): SHA-256 source hash, parser/configuration version, runtime snapshot, persisted blocks (idempotent per-block transactions), diagnostics. States: ready, translating, paused, interrupted (set at startup for active jobs), failed, cancelled, exporting, completed, completed_with_warnings. Resuming requires matching runtime and configuration snapshots. Oversized blocks are split conservatively (sentence, whitespace, then scalar boundaries; stable `parent::part-NNNN` ids). The worker pauses or cancels between blocks; Pause, Cancel and Clear also interrupt `llama-server` so an in-flight request fails fast.

Format modules (`documents/`): `txt.rs`, `docx.rs` (quick-xml, package-preserving, raw copy of untouched ZIP entries), `epub.rs` (spine XHTML), `fb2.rs`, and `pdf/`. Outputs are written through a same-directory temporary file and renamed; the source is never replaced, and only a path confirmed in the save dialog may replace an existing file. The Files UI (`DocumentsPanel.tsx`) polls `get_document_progress`; `get_document_job` loads all blocks only when a job is opened.

## PDF translation engine

`documents/pdf/` (PDFium is bound once per process behind a mutex, loaded at runtime from `pdfium.dll`; `LINGVOLOC_PDFIUM` overrides the path):

1. **Analysis** (`mod.rs::read_page`, `layout.rs`, `regions.rs`). PDFium page objects become fragments, lines, and paragraphs. Paragraphs are classified (heading, body, code, label), regions and reading order are detected (columns, panels, table cells, captions), and cross-page continuations are joined. Blocks get the ids `pdf#pNNNN-KKK`; no geometry is stored in the job database. The parser/configuration version is `pdf-v2` (page ranges are stored as `pdf-v2;pages=…`). Any change that alters grouping, skip rules, or reading order renumbers blocks, so existing jobs would have to be cleared and added again.
2. **Placement** (`slots.rs`, `reflow.rs`). Export repeats the same analysis on the hash-checked source. Every translated paragraph gets a _slot_ that starts at its source position and may grow only into free space: down to the nearest paragraph, rule, panel edge, image, or page margin; right up to neighbouring columns, cells, vertical rules, and panel padding. Panels drawn as tiles of same-coloured strips are merged. Paragraphs stacked in one column with nothing between them form a _run_ that is laid out together: the run is packed to see what fits, then free space moves items back towards their source positions. Search order: scale (1.0 down to 0.55) with paragraph gaps kept until the font is below 0.88, then tighter gaps and leading. Centred and right-aligned source text keeps its alignment, list markers follow their item, similar single-paragraph cells share one scale (not below 0.6), and a panel running to the page edge is treated as cut by a page break and does not grow.
3. **Output** (`mod.rs::export_with`, `fonts.rs`). Original text objects of translated paragraphs are removed (never covered with white boxes), the translation is drawn with embedded Noto fonts copying the original colour, and everything else on the page is kept. Page-range output keeps only the selected pages (`<name>.translated.<lang>.pN-M.pdf`).

Developer loop without the UI: build `cargo build --release --example pdf_try` in `apps/desktop/src-tauri`, then run `pdf_try.exe <pdf> <out.pdf> "" "1-8"` with `LINGVOLOC_PDFIUM`, optionally with `PDF_JOB_DB` and `PDF_JOB_ID` to use real translations of a finished job (without them a fake Cyrillic translation is used). Render pages with `tools/pdf-feasibility/examples/render.rs`. Layout rules are implemented in `apps/desktop/src-tauri/src/documents/pdf/`.

## Development

Requirements: Node.js 22+, Rust, WebView2, and the Windows C++ build tools (Visual Studio provides them). Then run `npm ci`.

- `npm run dev` runs the web UI; `npm run desktop:dev` runs the native shell.
- `npm run check` is the full local gate (formatting, lint, typechecks, extension tests, desktop tests, Rust format, Rust tests). Focused checks: `npm run lint`, `npm run typecheck`, `npm test`, `npm run extension:typecheck`, `npm run extension:test`, `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`. The strict CI-style lint is `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings`.

### Build

Before `npm run desktop:build` run `powershell -File scripts/fetch-pdfium.ps1` once: it downloads the pinned, hash-verified PDFium build into `apps/desktop/src-tauri/resources/pdfium/` (gitignored), which the installer bundles. `npm run desktop:build` compiles the Tauri executable and creates the x64 NSIS installer at `apps/desktop/src-tauri/target/release/bundle/nsis/`; `npm run desktop:verify-bundle` checks that PDFium and the font notices are staged. The installer offers per-user (Local AppData) or all-users (Program Files) installation, links the C runtime statically, and downloads the WebView2 runtime when it is missing. Quit the app before rebuilding (the running executable is locked). `npm run build` builds only the frontend; `npm run extension:build` and `npm run extension:package` build and zip the extension.

The release version is duplicated in the root, desktop, and extension `package.json`, the extension `manifest.json`, `tauri.conf.json`, and `Cargo.toml`; update them together (the extension ZIP name comes from `apps/extension/package.json`, and a published version's ZIP must not be repackaged).

### Lexical index converter

`npm run lexical:index -- --stardict-dir <directory> --output <index.json>` converts FreeDict StarDict directories, Kaikki JSONL (`--kaikki-language <code>`), and tab-separated morphology files (`--morphology-language <code>`) to a compact index. Keep each upstream dataset's license and attribution beside the generated output.

## Project documentation

- `docs/IMPLEMENTATION_PLAN.md`, `docs/FILE_TRANSLATION_PLAN.md`: implementation plans.
- `docs/LOCAL_API.md`: the authenticated loopback API.
- `docs/BROWSER_EXTENSION.md`: extension details.
- Third-party notices: PDFium and Noto fonts (`apps/desktop/src-tauri/resources/`), WordNet (`docs/WORDNET_NOTICE.txt`).
