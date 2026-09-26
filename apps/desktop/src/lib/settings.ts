export interface Settings {
  endpoint: string;
  modelId: string;
  adapterId: string;
  sourceLanguage: string;
  targetLanguage: string;
  primaryLanguage: string;
  secondaryLanguage: string;
}

const storageKey = 'lingvoloc.settings';
const legacyStorageKey = 'lingoloc.settings';
const textScaleStorageKey = 'lingvoloc.textScale';

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
    const parsed = { ...fallback, ...stored };
    if (stored.languagePair && !stored.primaryLanguage) {
      const [primaryLanguage, secondaryLanguage] =
        stored.languagePair.split('-');
      if (primaryLanguage && secondaryLanguage) {
        return { ...parsed, primaryLanguage, secondaryLanguage };
      }
    }
    return parsed;
  } catch {
    return fallback;
  }
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

export function clampTextScale(value: number): number {
  return Math.min(textScaleMax, Math.max(textScaleMin, value));
}
