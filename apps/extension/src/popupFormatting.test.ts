import { describe, expect, it } from 'vitest';
import {
  formatParagraphIndents,
  splitParagraphs,
  stripParagraphIndents,
  translateParagraphs,
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
});
