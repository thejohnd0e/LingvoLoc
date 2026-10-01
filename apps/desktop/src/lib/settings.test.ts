import { beforeEach, describe, expect, it } from 'vitest';
import {
  loadSettings,
  loadTextScale,
  saveSettings,
  saveTextScale,
  type Settings,
} from './settings';

const fallback: Settings = {
  runtimeMode: 'lmStudio',
  modelsDirectory: '',
  llamaServerPath: '',
  endpoint: 'http://localhost',
  modelId: '',
  adapterId: 'translategemma',
  sourceLanguage: 'auto',
  targetLanguage: 'en',
  primaryLanguage: 'en',
  secondaryLanguage: 'ru',
  translationStyle: 'neutral',
};

beforeEach(() => localStorage.clear());

describe('settings persistence', () => {
  it('returns fallback settings when storage is empty', () =>
    expect(loadSettings(fallback)).toEqual(fallback));
  it('merges persisted settings with defaults', () => {
    saveSettings({ ...fallback, targetLanguage: 'ru' });
    expect(loadSettings(fallback).targetLanguage).toBe('ru');
  });

  it('keeps automatic detection as the default source language', () => {
    expect(loadSettings(fallback).sourceLanguage).toBe('auto');
  });

  it('defaults an old settings object to neutral translation style', () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({ ...fallback, translationStyle: undefined }),
    );
    expect(loadSettings(fallback).translationStyle).toBe('neutral');
  });

  it.each(['neutral', 'literary', 'technical', 'conversational'] as const)(
    'persists the %s translation style',
    (translationStyle) => {
      saveSettings({ ...fallback, translationStyle });
      expect(loadSettings(fallback).translationStyle).toBe(translationStyle);
    },
  );

  it('falls back to neutral for an invalid stored translation style', () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({ ...fallback, translationStyle: 'custom' }),
    );
    expect(loadSettings(fallback).translationStyle).toBe('neutral');
  });

  it('persists and clamps the text scale', () => {
    saveTextScale(2);
    expect(loadTextScale()).toBe(1);
    saveTextScale(0.5);
    expect(loadTextScale()).toBe(0.5);
  });
});
