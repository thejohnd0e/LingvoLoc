import { beforeEach, describe, expect, it } from 'vitest';
import {
  loadSettings,
  loadTextScale,
  saveSettings,
  saveTextScale,
  type Settings,
} from './settings';

const fallback: Settings = {
  endpoint: 'http://localhost',
  modelId: '',
  adapterId: 'translategemma',
  sourceLanguage: 'auto',
  targetLanguage: 'en',
  primaryLanguage: 'en',
  secondaryLanguage: 'ru',
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

  it('persists and clamps the text scale', () => {
    saveTextScale(2);
    expect(loadTextScale()).toBe(1);
    saveTextScale(0.5);
    expect(loadTextScale()).toBe(0.5);
  });
});
