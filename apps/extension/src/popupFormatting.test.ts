import { describe, expect, it } from 'vitest';
import {
  formatParagraphIndents,
  splitParagraphs,
  stripParagraphIndents,
  translateParagraphs,
  formatTokenUsage,
} from './popupFormatting';

describe('popup paragraph formatting', () => {
  it('formats every source paragraph without changing the API text', () => {
    const source = 'First paragraph.\nSecond paragraph.';
    const formatted = formatParagraphIndents(source);

    expect(formatted).toContain('\u00a0\u00a0\u00a0\u00a0First paragraph.');
    expect(formatted).toContain('\n\u00a0\u00a0\u00a0\u00a0Second paragraph.');
    expect(stripParagraphIndents(formatted)).toBe(source);
  });

  it('treats line-separated model output as separate paragraphs', () => {
    expect(splitParagraphs('Первый абзац.\nВторой абзац.')).toEqual([
      'Первый абзац.',
      'Второй абзац.',
    ]);
  });

  it('translates each source paragraph independently', async () => {
    const translated = await translateParagraphs(
      ['First paragraph.', 'Second paragraph.'],
      async (paragraph) => `${paragraph} translated`,
    );

    expect(translated).toBe(
      'First paragraph. translated\n\nSecond paragraph. translated',
    );
  });

  it('formats complete, partial, and unavailable usage honestly', () => {
    expect(formatTokenUsage(30, 12, 2, 2)).toBe('30 in · 12 out · 42 total');
    expect(formatTokenUsage(30, 12, 1, 2)).toBe(
      '30 in · 12 out · 42 total · partial',
    );
    expect(formatTokenUsage(0, 0, 0, 2)).toBe('Token usage unavailable');
  });
});
