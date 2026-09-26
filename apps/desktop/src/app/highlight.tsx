import type { ReactNode } from 'react';

export function highlightMatches(text: string, query: string): ReactNode {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) return text;
  const escapedQuery = trimmedQuery.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const parts = text.split(new RegExp(`(${escapedQuery})`, 'giu'));
  return parts.map((part, index) =>
    index % 2 === 1 ? <mark key={`${part}-${index}`}>{part}</mark> : part,
  );
}
