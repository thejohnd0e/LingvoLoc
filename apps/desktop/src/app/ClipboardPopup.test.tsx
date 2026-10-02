import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ClipboardPopup from './ClipboardPopup';

const commandMocks = vi.hoisted(() => ({
  detectLanguage: vi.fn(),
  getNativeSettings: vi.fn(),
  takeClipboardRequest: vi.fn(),
  translate: vi.fn(),
  writeClipboard: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    scaleFactor: vi.fn().mockResolvedValue(1),
    innerSize: vi.fn().mockResolvedValue({ width: 400 }),
    setSize: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('../lib/commands', () => ({
  ...commandMocks,
  formatTiming: () => '10 ms',
}));

describe('clipboard popup translation', () => {
  beforeEach(() => {
    localStorage.clear();
    commandMocks.takeClipboardRequest.mockResolvedValue('Hello');
    commandMocks.detectLanguage.mockResolvedValue({
      code: 'en',
      confidence: 1,
    });
    commandMocks.translate.mockResolvedValue({
      text: 'Привет',
      model_id: 'model',
      adapter_id: 'adapter',
      latency_ms: 10,
      prompt_tokens: null,
      completion_tokens: null,
      total_tokens: null,
    });
    commandMocks.writeClipboard.mockResolvedValue(undefined);
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({ modelId: 'model', translationStyle: 'literary' }),
    );
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('passes the saved style to translation', async () => {
    render(<ClipboardPopup />);
    await waitFor(() => expect(commandMocks.translate).toHaveBeenCalled());
    expect(commandMocks.translate).toHaveBeenCalledWith(
      expect.objectContaining({ translation_style: 'literary' }),
    );
  });
});
