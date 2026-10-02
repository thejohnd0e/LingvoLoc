export type RuntimeMode =
  | 'lmStudio'
  | 'standalone'
  | 'openAi'
  | 'anthropic'
  | 'gemini'
  | 'deepL'
  | 'openAiCompatible';
export type TranslationStyle =
  'neutral' | 'literary' | 'technical' | 'conversational';

export interface LocalModel {
  id: string;
  owned_by?: string;
  quantization?: string;
}

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

const translationStyles: readonly TranslationStyle[] = [
  'neutral',
  'literary',
  'technical',
  'conversational',
];

export interface Settings {
  runtimeMode: RuntimeMode;
  modelsDirectory: string;
  llamaServerPath: string;
  endpoint: string;
  modelId: string;
  adapterId: string;
  sourceLanguage: string;
  targetLanguage: string;
  primaryLanguage: string;
  secondaryLanguage: string;
  translationStyle: TranslationStyle;
  cloud: CloudSettings;
}

export const defaultCloudSettings: CloudSettings = {
  consentAccepted: false,
  openAi: {
    modelId: 'gpt-4o-mini',
    availableModels: [],
    modelsRefreshedAt: null,
  },
  anthropic: {
    modelId: 'claude-3-5-haiku-latest',
    availableModels: [],
    modelsRefreshedAt: null,
  },
  gemini: {
    modelId: 'gemini-2.0-flash',
    availableModels: [],
    modelsRefreshedAt: null,
  },
  deepL: { plan: 'free', availableLanguages: [], languagesRefreshedAt: null },
  openAiCompatible: {
    modelId: '',
    availableModels: [],
    modelsRefreshedAt: null,
    endpoint: '',
  },
};

const storageKey = 'lingvoloc.settings';
const legacyStorageKey = 'lingoloc.settings';
const textScaleStorageKey = 'lingvoloc.textScale';
const clipboardTextScaleStorageKey = 'lingvoloc.clipboardTextScale';

export const textScaleMin = 0.5;
export const textScaleMax = 1;
export const textScaleStep = 0.1;

export function loadSettings(fallback: Settings): Settings {
  try {
    const value =
      localStorage.getItem(storageKey) ??
      localStorage.getItem(legacyStorageKey);
    if (!value) return fallback;
    const stored = JSON.parse(value) as Partial<Settings> & {
      languagePair?: string;
    };
    const parsed = {
      ...fallback,
      ...stored,
      cloud: normalizeCloudSettings(stored.cloud, fallback.cloud),
      translationStyle: translationStyles.includes(
        stored.translationStyle as TranslationStyle,
      )
        ? (stored.translationStyle as TranslationStyle)
        : 'neutral',
    };
    if (stored.languagePair && !stored.primaryLanguage) {
      const [primaryLanguage, secondaryLanguage] =
        stored.languagePair.split('-');
      if (primaryLanguage && secondaryLanguage) {
        return { ...parsed, primaryLanguage, secondaryLanguage };
      }
    }
    return parsed;
  } catch {
    return {
      ...fallback,
      translationStyle: fallback.translationStyle ?? 'neutral',
    };
  }
}

function normalizeCloudSettings(
  value: unknown,
  fallbackCloud: CloudSettings,
): CloudSettings {
  if (!value || typeof value !== 'object') return fallbackCloud;
  const candidate = value as Partial<CloudSettings>;
  if (typeof candidate.consentAccepted !== 'boolean') return fallbackCloud;
  if (!candidate.openAi || !candidate.anthropic || !candidate.gemini) {
    return fallbackCloud;
  }
  return {
    ...fallbackCloud,
    ...candidate,
    deepL: { ...fallbackCloud.deepL, ...(candidate.deepL ?? {}) },
    openAiCompatible: {
      ...fallbackCloud.openAiCompatible,
      ...(candidate.openAiCompatible ?? {}),
    },
  };
}

export function saveSettings(settings: Settings): void {
  localStorage.setItem(storageKey, JSON.stringify(settings));
}

export function loadTextScale(): number {
  const stored = Number(localStorage.getItem(textScaleStorageKey));
  return Number.isFinite(stored) ? clampTextScale(stored) : 1;
}

export function saveTextScale(value: number): void {
  localStorage.setItem(textScaleStorageKey, String(clampTextScale(value)));
}

export function loadClipboardTextScale(): number {
  const stored = Number(localStorage.getItem(clipboardTextScaleStorageKey));
  return Number.isFinite(stored) ? clampTextScale(stored) : 1;
}

export function saveClipboardTextScale(value: number): void {
  localStorage.setItem(
    clipboardTextScaleStorageKey,
    String(clampTextScale(value)),
  );
}

export function clampTextScale(value: number): number {
  return Math.min(textScaleMax, Math.max(textScaleMin, value));
}
