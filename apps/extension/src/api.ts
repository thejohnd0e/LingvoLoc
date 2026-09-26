const API_BASE = 'http://127.0.0.1:47831/api/v1';

export interface TranslationResult {
  text: string;
  model_id: string;
  adapter_id: string;
  latency_ms: number;
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
    func: () => window.getSelection()?.toString() ?? '',
  });
  return results[0]?.result?.trim() ?? '';
}
