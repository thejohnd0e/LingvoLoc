import { describe, expect, it } from 'vitest';
import {
  formatDuration,
  groupDiagnostics,
  groupPdfDiagnostics,
} from './documentProgress';

describe('documentProgress', () => {
  it('collapses repeated diagnostics with a count', () => {
    expect(groupDiagnostics(['a', 'b', 'a', 'a'])).toEqual(['a (×3)', 'b']);
    expect(groupDiagnostics([])).toEqual([]);
  });

  it('summarizes inline-formatting notices in one line', () => {
    const notice =
      'FB2 translation may cross inline formatting boundaries in paragraph';
    expect(groupDiagnostics([notice, 'x', notice])).toEqual([
      '2 paragraphs contain italics, links, or other inline formatting; their position in the translation may shift.',
      'x',
    ]);
    expect(groupDiagnostics([notice])[0]).toMatch(/^1 paragraph contains/);
    expect(
      groupDiagnostics([notice, 'additional FB2 diagnostics omitted'])[0],
    ).toMatch(/^At least 1 paragraphs/);
  });

  it('formats durations', () => {
    expect(formatDuration(46_000)).toBe('46s');
    expect(formatDuration(166_000)).toBe('2m 46s');
    expect(formatDuration(3_725_000)).toBe('1h 2m 5s');
  });

  it('groups PDF layout warnings by page', () => {
    expect(
      groupPdfDiagnostics([
        'Translated PDF page 2 block pdf#p0002-001 did not fit',
        'Translated PDF page 2 block pdf#p0002-002 did not fit',
        'Pages without extractable text stay unchanged: 4',
      ]),
    ).toEqual([
      'Translated PDF page 2 blocks did not fit (×2)',
      'Pages without extractable text stay unchanged: 4',
    ]);
  });
});
