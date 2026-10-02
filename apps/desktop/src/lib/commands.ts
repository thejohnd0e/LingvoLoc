import { invoke } from '@tauri-apps/api/core';
import type { Settings, TranslationStyle } from './settings';

const STATE_RETRY_DELAY_MS = 100;
const STATE_RETRY_ATTEMPTS = 8;

function isStateNotManagedError(reason: unknown): boolean {
  const message =
    reason instanceof Error
      ? reason.message
      : typeof reason === 'string'
        ? reason
        : String(JSON.stringify(reason));
  return message.includes('state not managed');
}

async function invokeNative<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await invoke<T>(command, args);
    } catch (reason) {
      if (attempt >= STATE_RETRY_ATTEMPTS || !isStateNotManagedError(reason)) {
        throw reason;
      }
      await new Promise((resolve) =>
        window.setTimeout(resolve, STATE_RETRY_DELAY_MS),
      );
    }
  }
}

export interface LocalModel {
  id: string;
  owned_by?: string;
  quantization?: string;
}

export interface RuntimeStatus {
  available: boolean;
  endpoint: string;
  detail: string;
}

export interface TranslationResult {
  text: string;
  model_id: string;
  adapter_id: string;
  latency_ms: number;
  prompt_tokens?: number | null;
  completion_tokens?: number | null;
  total_tokens?: number | null;
  provider_id?: string | null;
  billed_characters?: number | null;
}

export type ProviderId =
  | 'llamaCpp'
  | 'lmStudio'
  | 'openAi'
  | 'anthropic'
  | 'gemini'
  | 'deepL'
  | 'openAiCompatible'
  | 'deepSeek'
  | 'openRouter';

export interface CredentialStatus {
  configured: boolean;
  hint: string | null;
}

export interface BackendCapabilities {
  model_list: boolean;
  custom_model_id: boolean;
  token_usage: boolean;
  billed_characters: boolean;
  translation_styles: boolean;
}

/** Formats only exact provider usage; missing usage is reported instead of estimated. */
export function formatTiming(
  result: Pick<
    TranslationResult,
    'latency_ms' | 'prompt_tokens' | 'completion_tokens' | 'total_tokens'
  >,
) {
  const {
    latency_ms: latency,
    prompt_tokens: input,
    completion_tokens: output,
    total_tokens: total,
  } = result;
  if (input == null || output == null || total == null) {
    return `${latency} ms · token usage unavailable`;
  }
  return `${latency} ms · ${latency > 0 ? ((output * 1000) / latency).toFixed(1) : '0.0'} tok/s · Used tokens: ${input} in · ${output} out · ${total} total`;
}

export interface DetectedLanguage {
  code: string;
  confidence: number;
}

export interface HistoryEntry {
  id: number;
  source_text: string;
  translated_text: string;
  source_language: string;
  target_language: string;
  model_id: string;
  created_at: number;
  favorite: boolean;
  provider_id?: ProviderId | null;
}

export interface SessionUsageEntry {
  providerId: ProviderId;
  modelId: string;
  requests: number;
  failedRequests: number;
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
  billedCharacters: number | null;
}

export interface TranslationRequest {
  model_id: string;
  adapter_id: string;
  source_language: string;
  target_language: string;
  text: string;
  translation_style: TranslationStyle;
}

export const getRuntimeStatus = () =>
  invokeNative<RuntimeStatus>('get_runtime_status');
export const checkLlamaServer = (path: string) =>
  invokeNative<string>('check_llama_server', { path });
export interface DownloadedLlama {
  path: string;
  version: string;
  variant: string;
  upToDate: boolean;
}
export interface LlamaDownloadProgress {
  percent: number;
  stage: string;
}
export interface GpuInfo {
  names: string[];
  backend: 'CUDA' | 'Vulkan' | 'CPU';
}
export interface LlamaDevice {
  id: string;
  name: string;
  memory_mib: number;
}
export interface LlamaDevices {
  devices: LlamaDevice[];
  active: LlamaDevice | null;
}
export const getLlamaDevices = (path: string) =>
  invokeNative<LlamaDevices>('get_llama_devices', { path });
export const llamaPathStatus = (path: string) =>
  invokeNative<'present' | 'absent'>('llama_path_status', { path });
export const addLlamaToPath = (path: string) =>
  invokeNative<'present' | 'added'>('add_llama_to_path', { path });
export const getGpuInfo = () => invokeNative<GpuInfo>('get_gpu_info');
export const downloadLlamaCpp = () =>
  invokeNative<DownloadedLlama>('download_llama_cpp');
export const locateLlamaServer = (directory: string) =>
  invokeNative<string>('locate_llama_server', { directory });
export const findLlamaServer = () =>
  invokeNative<string | null>('find_llama_server');
export const getApiToken = () => invokeNative<string>('get_api_token');
export const writeClipboard = (text: string) =>
  invokeNative<void>('write_clipboard', { text });
export const listModels = () => invokeNative<LocalModel[]>('list_models');
export const getBackendCapabilities = () =>
  invokeNative<BackendCapabilities>('get_backend_capabilities');
export const refreshProviderModels = (providerId: ProviderId) =>
  invokeNative<LocalModel[]>('refresh_provider_models', { providerId });
export const refreshDeepLLanguages = () =>
  invokeNative<string[]>('refresh_deepl_languages');
export const testProviderConnection = (providerId: ProviderId) =>
  invokeNative<RuntimeStatus>('test_provider_connection', { providerId });
export const getSessionUsage = () =>
  invokeNative<SessionUsageEntry[]>('get_session_usage');
export const getNativeSettings = () => invokeNative<Settings>('get_settings');
export const updateSettings = (next: Settings) =>
  invokeNative<Settings>('update_settings', { next });
export const getProviderCredentialStatus = (providerId: ProviderId) =>
  invokeNative<CredentialStatus>('get_provider_credential_status', {
    providerId,
  });
export const saveProviderCredential = (
  providerId: ProviderId,
  secret: string,
) =>
  invokeNative<CredentialStatus>('save_provider_credential', {
    providerId,
    secret,
  });
export const deleteProviderCredential = (providerId: ProviderId) =>
  invokeNative<void>('delete_provider_credential', { providerId });
export const cancelTranslation = () =>
  invokeNative<number>('cancel_translation');

export const translate = (request: TranslationRequest) =>
  invokeNative<TranslationResult>('translate', { request });
export const detectLanguage = (text: string) =>
  invokeNative<DetectedLanguage>('detect_language', { text });
export const listHistory = (query?: string, offset = 0) =>
  invokeNative<HistoryEntry[]>('list_history', {
    query: query?.trim() || null,
    offset,
  });
export const clearHistory = () => invokeNative<void>('clear_history');
export const setHistoryFavorite = (id: number, favorite: boolean) =>
  invokeNative<void>('set_history_favorite', { id, favorite });
export const exportHistory = () => invokeNative<string>('export_history');
export const takeClipboardRequest = () =>
  invokeNative<string | null>('take_clipboard_request');

export interface LexicalEntry {
  lemma: string;
  language: string;
  part_of_speech: string;
  translations: string[];
  definitions: string[];
  forms: string[];
  synonyms: string[];
  antonyms: string[];
  related_words: string[];
  examples: string[];
  providers: string[];
}

export interface UserDictionary {
  id: string;
  name: string;
  entry_count: number;
}

export const lookupLexicon = (
  query: string,
  language?: string,
  enabledDictionaries?: string[],
  directory?: string,
) =>
  invokeNative<LexicalEntry[]>('lookup_lexicon', {
    query,
    language,
    enabledDictionaries,
    directory,
  });

export const listUserDictionaries = (directory?: string) =>
  invokeNative<UserDictionary[]>('list_user_dictionaries', { directory });

export const readDictionaryMedia = (directory: string, resource: string) =>
  invokeNative<string>('read_dictionary_media', { directory, resource });

export const translateWord = (request: TranslationRequest) =>
  invokeNative<TranslationResult>('translate_word', { request });

export interface DocumentBlock {
  id: string;
  ordinal: number;
  block_type: string;
  source_text: string;
  translated_text?: string | null;
}

export interface DocumentJob {
  id: string;
  source_path: string;
  source_hash: string;
  format: string;
  parser_version: string;
  source_language: string;
  target_language: string;
  translation_style: TranslationStyle;
  runtime_snapshot: string;
  configuration_version: string;
  state: string;
  error?: string | null;
}

export interface DocumentJobView {
  job: DocumentJob;
  blocks: DocumentBlock[];
  diagnostics: string[];
  translated_blocks: number;
  total_blocks: number;
  paused: boolean;
  cancelled: boolean;
}

export interface DocumentJobSummary {
  job: DocumentJob;
  total_blocks: number;
  translated_blocks: number;
  last_request_usage?: {
    input_tokens?: number | null;
    output_tokens?: number | null;
    total_tokens?: number | null;
  } | null;
}

export interface DocumentExport {
  output_path: string;
  job: DocumentJobView;
}

export const analyzeTxt = (
  sourcePath: string,
  sourceLanguage: string,
  targetLanguage: string,
  translationStyle: TranslationStyle = 'neutral',
) =>
  invokeNative<DocumentJobView>('analyze_txt', {
    sourcePath,
    sourceLanguage,
    targetLanguage,
    translationStyle,
  });
export const analyzeDocx = (
  sourcePath: string,
  sourceLanguage: string,
  targetLanguage: string,
  translationStyle: TranslationStyle = 'neutral',
) =>
  invokeNative<DocumentJobView>('analyze_docx', {
    sourcePath,
    sourceLanguage,
    targetLanguage,
    translationStyle,
  });
export const analyzeEpub = (
  sourcePath: string,
  sourceLanguage: string,
  targetLanguage: string,
  translationStyle: TranslationStyle = 'neutral',
) =>
  invokeNative<DocumentJobView>('analyze_epub', {
    sourcePath,
    sourceLanguage,
    targetLanguage,
    translationStyle,
  });
export const analyzeFb2 = (
  sourcePath: string,
  sourceLanguage: string,
  targetLanguage: string,
  translationStyle: TranslationStyle = 'neutral',
) =>
  invokeNative<DocumentJobView>('analyze_fb2', {
    sourcePath,
    sourceLanguage,
    targetLanguage,
    translationStyle,
  });
export const analyzePdf = (
  sourcePath: string,
  sourceLanguage: string,
  targetLanguage: string,
  translationStyle: TranslationStyle = 'neutral',
  pages?: string,
) =>
  invokeNative<DocumentJobView>('analyze_pdf', {
    sourcePath,
    sourceLanguage,
    targetLanguage,
    translationStyle,
    pages: pages?.trim() || null,
  });
export const startTxtJob = (jobId: string) =>
  invokeNative<DocumentJobView>('start_txt_job', { jobId });
export const startDocxJob = (jobId: string) =>
  invokeNative<DocumentJobView>('start_docx_job', { jobId });
export const startEpubJob = (jobId: string) =>
  invokeNative<DocumentJobView>('start_epub_job', { jobId });
export const startFb2Job = (jobId: string) =>
  invokeNative<DocumentJobView>('start_fb2_job', { jobId });
export const startPdfJob = (jobId: string) =>
  invokeNative<DocumentJobView>('start_pdf_job', { jobId });
export const resumeTxtJob = (jobId: string) =>
  invokeNative<DocumentJobView>('resume_txt_job', { jobId });
export const resumeDocxJob = (jobId: string) =>
  invokeNative<DocumentJobView>('resume_docx_job', { jobId });
export const resumeEpubJob = (jobId: string) =>
  invokeNative<DocumentJobView>('resume_epub_job', { jobId });
export const resumeFb2Job = (jobId: string) =>
  invokeNative<DocumentJobView>('resume_fb2_job', { jobId });
export const resumePdfJob = (jobId: string) =>
  invokeNative<DocumentJobView>('resume_pdf_job', { jobId });
export const getDocumentJob = (jobId: string) =>
  invokeNative<DocumentJobView>('get_document_job', { jobId });
export const getDocumentProgress = (jobId: string) =>
  invokeNative<DocumentJobSummary>('get_document_progress', { jobId });
export const listDocumentJobs = () =>
  invokeNative<DocumentJobSummary[]>('list_document_jobs');
export const pauseDocumentJob = (jobId: string) =>
  invokeNative<DocumentJobView>('pause_document_job', { jobId });
export const cancelDocumentJob = (jobId: string) =>
  invokeNative<DocumentJobView>('cancel_document_job', { jobId });
export const clearDocumentJob = (jobId: string) =>
  invokeNative<void>('clear_document_job', { jobId });
export const exportTxtJob = (jobId: string, outputPath?: string) =>
  invokeNative<DocumentExport>('export_txt_job', {
    jobId,
    outputPath: outputPath ?? null,
  });
export const exportDocxJob = (jobId: string, outputPath?: string) =>
  invokeNative<DocumentExport>('export_docx_job', {
    jobId,
    outputPath: outputPath ?? null,
  });
export const exportEpubJob = (jobId: string, outputPath?: string) =>
  invokeNative<DocumentExport>('export_epub_job', {
    jobId,
    outputPath: outputPath ?? null,
  });
export const exportFb2Job = (jobId: string, outputPath?: string) =>
  invokeNative<DocumentExport>('export_fb2_job', {
    jobId,
    outputPath: outputPath ?? null,
  });
export const exportPdfJob = (jobId: string, outputPath?: string) =>
  invokeNative<DocumentExport>('export_pdf_job', {
    jobId,
    outputPath: outputPath ?? null,
  });
