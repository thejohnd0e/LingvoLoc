export const AUTO_TRANSLATE_KEY = 'autoTranslatePending';
export const TEXT_SPLIT_KEY = 'textSplitRatio';
export const ORIGINAL_TEXT_EXPANDED_KEY = 'originalTextExpanded';
export const DEFAULT_TEXT_SPLIT = 0.5;
export const DEFAULT_ORIGINAL_TEXT_EXPANDED = true;

export function clampTextSplit(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(0.75, Math.max(0.25, value))
    : DEFAULT_TEXT_SPLIT;
}

export function isAutoTranslateRequest(value: unknown): boolean {
  return value === true;
}

export function isOriginalTextExpanded(value: unknown): boolean {
  return typeof value === 'boolean' ? value : DEFAULT_ORIGINAL_TEXT_EXPANDED;
}
