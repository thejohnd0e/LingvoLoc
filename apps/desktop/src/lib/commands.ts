import { invoke } from '@tauri-apps/api/core';
import type { Settings } from './settings';

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
}

export interface TranslationRequest {
  model_id: string;
  adapter_id: string;
  source_language: string;
  target_language: string;
  text: string;
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
export const getNativeSettings = () => invokeNative<Settings>('get_settings');
export const updateSettings = (next: Settings) =>
  invokeNative<Settings>('update_settings', { next });
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
