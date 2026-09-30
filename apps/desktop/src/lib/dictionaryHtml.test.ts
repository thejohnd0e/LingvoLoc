import { describe, expect, it } from 'vitest';
import { sanitizeDictionaryHtml } from './dictionaryHtml';

const cambridge =
  '<k>mom</k>\n<b>mom</b> <abr>UK</abr> <c c="darkcyan">[mɒm]</c> <c c="orange"> noun </c>' +
  '<blockquote> <c c="lightgreen"><b>&lt;</b></c><abr>I</abr><c c="lightgreen"> <b>&gt;</b></c>' +
  ' <c c="rosybrown">US </c> mother </blockquote>';

describe('sanitizeDictionaryHtml', () => {
  it('drops the repeated headword key and the < I > sense marker', () => {
    const html = sanitizeDictionaryHtml(cambridge);
    expect(html).not.toContain('<k>');
    expect(html).not.toContain('&lt;');
    expect(html).not.toContain('&gt;');
    const text = new DOMParser().parseFromString(html, 'text/html').body
      .textContent;
    expect(text?.replace(/\s+/g, ' ').trim()).toBe(
      'mom UK [mɒm] noun US mother',
    );
  });

  it('keeps ordinary angle brackets and removes unsafe markup', () => {
    const html = sanitizeDictionaryHtml(
      '<b>a &lt; b</b><script>alert(1)</script><span onclick="x()">ok</span>',
    );
    expect(html).toContain('a &lt; b');
    expect(html).not.toContain('script');
    expect(html).not.toContain('onclick');
  });

  it('turns colour names into theme classes without inline styles', () => {
    const html = sanitizeDictionaryHtml(
      '<c c="darkcyan">[mɒm]</c><c c="red;background:url(x)">x</c>',
    );
    expect(html).toContain('dc-darkcyan');
    expect(html).not.toContain('style');
    expect(html).not.toContain('dc-red;');
  });
});
