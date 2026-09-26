import { describe, expect, it } from 'vitest';
import { targetForDetectedLanguage } from './languagePair';

describe('automatic language pair target', () => {
  it('selects the opposite language for either pair member', () => {
    expect(targetForDetectedLanguage('en', 'en', 'ru')).toBe('ru');
    expect(targetForDetectedLanguage('ru', 'en', 'ru')).toBe('en');
  });

  it('uses the first pair language for an outside language', () => {
    expect(targetForDetectedLanguage('de', 'en', 'ru')).toBe('en');
  });
});
