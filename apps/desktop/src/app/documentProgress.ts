export function formatEta(
  translatedBlocks: number,
  totalBlocks: number,
  startedAt: number,
  now = Date.now(),
): string | null {
  if (translatedBlocks <= 0 || totalBlocks <= translatedBlocks) return null;
  const elapsedSeconds = (now - startedAt) / 1000;
  if (elapsedSeconds <= 0) return null;
  const remainingSeconds = Math.ceil(
    ((totalBlocks - translatedBlocks) * elapsedSeconds) / translatedBlocks,
  );
  const minutes = Math.floor(remainingSeconds / 60);
  const seconds = remainingSeconds % 60;
  return minutes > 0 ? `~${minutes}m ${seconds}s` : `~${seconds}s`;
}

export function formatDuration(milliseconds: number): string {
  const totalSeconds = Math.max(0, Math.round(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${minutes}m ${seconds}s`;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

const formattingBoundary = /formatting boundaries/i;

/**
 * Collapses repeated identical diagnostics into one counted line and reports all
 * inline-formatting notices as a single summary. The native list is capped, so the
 * count is a lower bound when the list says that diagnostics were omitted.
 */
export function groupDiagnostics(diagnostics: string[]): string[] {
  const counts = new Map<string, number>();
  let formatted = 0;
  const truncated = diagnostics.some((diagnostic) =>
    /omitted/i.test(diagnostic),
  );
  for (const diagnostic of diagnostics) {
    if (formattingBoundary.test(diagnostic)) {
      formatted += 1;
    } else {
      counts.set(diagnostic, (counts.get(diagnostic) ?? 0) + 1);
    }
  }
  const lines = [...counts].map(([text, count]) =>
    count > 1 ? `${text} (×${count})` : text,
  );
  if (formatted > 0) {
    const single = formatted === 1 && !truncated;
    const amount = `${truncated ? 'At least ' : ''}${formatted}`;
    lines.unshift(
      single
        ? `${amount} paragraph contains italics, links, or other inline formatting; its position in the translation may shift.`
        : `${amount} paragraphs contain italics, links, or other inline formatting; their position in the translation may shift.`,
    );
  }
  return lines;
}
