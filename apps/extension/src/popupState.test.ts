import { describe, expect, it } from 'vitest';
import {
  DEFAULT_TEXT_SPLIT,
  clampTextSplit,
  DEFAULT_ORIGINAL_TEXT_EXPANDED,
  isOriginalTextExpanded,
  isAutoTranslateRequest,
} from './popupState';

describe('popup state', () => {
  it('clamps divider ratios and defaults invalid values', () => {
    expect(clampTextSplit(0.1)).toBe(0.25);
    expect(clampTextSplit(0.25)).toBe(0.25);
    expect(clampTextSplit(0.75)).toBe(0.75);
    expect(clampTextSplit(0.8)).toBe(0.75);
    expect(clampTextSplit('bad')).toBe(DEFAULT_TEXT_SPLIT);
  });

  it('recognizes only a one-shot auto-translate flag', () => {
    expect(isAutoTranslateRequest(true)).toBe(true);
    expect(isAutoTranslateRequest(false)).toBe(false);
    expect(isAutoTranslateRequest('true')).toBe(false);
    expect(isAutoTranslateRequest({ value: true })).toBe(false);
  });

  it('restores only boolean Original text spoiler states', () => {
    expect(isOriginalTextExpanded(false)).toBe(false);
    expect(isOriginalTextExpanded(true)).toBe(true);
    expect(isOriginalTextExpanded('false')).toBe(
      DEFAULT_ORIGINAL_TEXT_EXPANDED,
    );
  });
});
