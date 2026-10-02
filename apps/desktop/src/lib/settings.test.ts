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
  cloud: {
    consentAccepted: false,
    openAi: {
      modelId: 'gpt-4o-mini',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
    },
    anthropic: {
      modelId: 'claude-3-5-haiku-latest',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
    },
    gemini: {
      modelId: 'gemini-2.0-flash',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
    },
    deepL: {
      plan: 'free',
      proxyUrl: '',
      availableLanguages: [],
      languagesRefreshedAt: null,
    },
    openAiCompatible: {
      modelId: '',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
      endpoint: '',
    },
    deepSeek: {
      modelId: 'deepseek-chat',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
    },
    openRouter: {
      modelId: '',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
    },
  },
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

  it('loads old local settings with cloud defaults', () => {
    localStorage.setItem(
      'lingoloc.settings',
      JSON.stringify({ runtimeMode: 'standalone', modelId: 'old.gguf' }),
    );
    expect(loadSettings(fallback)).toMatchObject({
      runtimeMode: 'standalone',
      modelId: 'old.gguf',
      cloud: { consentAccepted: false },
    });
  });

  it.each([
    'lmStudio',
    'standalone',
    'openAi',
    'anthropic',
    'gemini',
    'deepL',
    'openAiCompatible',
    'deepSeek',
    'openRouter',
  ] as const)('accepts runtime mode %s', (runtimeMode) => {
    localStorage.setItem('lingvoloc.settings', JSON.stringify({ runtimeMode }));
    expect(loadSettings(fallback).runtimeMode).toBe(runtimeMode);
  });

  it('keeps cloud defaults when stored cloud fields are invalid', () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({ cloud: { consentAccepted: 'yes', openAi: null } }),
    );
    expect(loadSettings(fallback).cloud).toEqual(fallback.cloud);
  });

  it('fills missing nested cloud model fields from defaults', () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({
        cloud: {
          ...fallback.cloud,
          openAi: { modelId: 'gpt-4.1' },
        },
      }),
    );

    expect(loadSettings(fallback).cloud.openAi).toEqual({
      modelId: 'gpt-4.1',
      proxyUrl: '',
      availableModels: [],
      modelsRefreshedAt: null,
    });
  });

  it('persists and clamps the text scale', () => {
    saveTextScale(2);
    expect(loadTextScale()).toBe(1);
    saveTextScale(0.5);
    expect(loadTextScale()).toBe(0.5);
  });
});
