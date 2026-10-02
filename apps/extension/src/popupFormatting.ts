export const PARAGRAPH_INDENT = '\u00a0\u00a0\u00a0\u00a0';

export function formatTokenUsage(
  input: number,
  output: number,
  translatedParagraphs: number,
  paragraphs: number,
): string {
  if (translatedParagraphs === 0) return 'Token usage unavailable';
  const partial = translatedParagraphs < paragraphs ? ' · partial' : '';
  return `${input} in · ${output} out · ${input + output} total${partial}`;
}

function normalizeLineBreaks(text: string): string {
  return text.replace(/\r\n?/g, '\n');
}

export function formatParagraphIndents(text: string): string {
  return normalizeLineBreaks(text)
    .split('\n')
    .map((paragraph) =>
      paragraph ? `${PARAGRAPH_INDENT}${paragraph}` : paragraph,
    )
    .join('\n');
}

export function stripParagraphIndents(text: string): string {
  return normalizeLineBreaks(text).replace(
    new RegExp(`(^|\\n+)${PARAGRAPH_INDENT}`, 'g'),
    '$1',
  );
}

export function splitParagraphs(text: string): string[] {
  return normalizeLineBreaks(text)
    .split(/\n+/)
    .map((paragraph) => paragraph.trim())
    .filter(Boolean);
}

export async function translateParagraphs(
  paragraphs: string[],
  translateParagraph: (paragraph: string) => Promise<string>,
): Promise<string> {
  const translated: string[] = [];
  for (const paragraph of paragraphs) {
    translated.push(await translateParagraph(paragraph));
  }
  return translated.join('\n\n');
}
