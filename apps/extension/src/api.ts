const API_BASE = 'http://127.0.0.1:47831/api/v1';

export interface TranslationResult {
  text: string;
  model_id: string;
  adapter_id: string;
  latency_ms: number;
  prompt_tokens?: number | null;
  completion_tokens?: number | null;
  total_tokens?: number | null;
}

export interface RuntimeStatus {
  available: boolean;
  endpoint: string;
  detail: string;
}

interface ApiError {
  error?: string;
}

export async function translate(
  token: string,
  text: string,
  sourceLanguage: string,
  targetLanguage: string,
): Promise<TranslationResult> {
  const response = await fetch(`${API_BASE}/translate`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      text,
      sourceLanguage,
      targetLanguage,
    }),
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as ApiError;
    throw new Error(
      body.error ?? `LingvoLoc API returned HTTP ${response.status}`,
    );
  }
  return (await response.json()) as TranslationResult;
}

export async function getStatus(token: string): Promise<RuntimeStatus> {
  const response = await fetch(`${API_BASE}/status`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as ApiError;
    throw new Error(
      body.error ?? `LingvoLoc API returned HTTP ${response.status}`,
    );
  }
  return (await response.json()) as RuntimeStatus;
}

export async function getSelectedText(): Promise<string> {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab.id) {
    return '';
  }
  const results = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: () => {
      const selection = window.getSelection();
      if (!selection?.rangeCount) return '';
      const fragment = selection.getRangeAt(0).cloneContents();
      const blockTags = new Set([
        'ADDRESS',
        'ARTICLE',
        'ASIDE',
        'BLOCKQUOTE',
        'DIV',
        'FIGCAPTION',
        'H1',
        'H2',
        'H3',
        'H4',
        'H5',
        'H6',
        'HEADER',
        'LI',
        'P',
        'PRE',
        'SECTION',
        'TR',
      ]);
      const read = (node: Node): string => {
        if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? '';
        if (node.nodeType === Node.DOCUMENT_FRAGMENT_NODE) {
          return Array.from(node.childNodes, read).join('');
        }
        if (node.nodeType !== Node.ELEMENT_NODE) return '';
        const element = node as HTMLElement;
        if (element.tagName === 'BR') return '\n';
        const content = Array.from(element.childNodes, read).join('');
        return blockTags.has(element.tagName) ? `\n\n${content}\n\n` : content;
      };
      return read(fragment)
        .replace(/[ \t]*\n[ \t]*/g, '\n')
        .replace(/\n{3,}/g, '\n\n')
        .trim();
    },
  });
  return results[0]?.result?.trim() ?? '';
}
