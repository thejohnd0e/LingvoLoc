import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import { highlightMatches } from './highlight';

vi.mock('../lib/commands', () => ({
  getRuntimeStatus: vi.fn().mockRejectedValue(new Error('offline')),
  getGpuInfo: vi.fn().mockResolvedValue({ names: [], backend: 'CPU' }),
  getLlamaDevices: vi.fn().mockResolvedValue({ devices: [], active: null }),
  llamaPathStatus: vi.fn().mockResolvedValue('absent'),
  addLlamaToPath: vi.fn().mockResolvedValue('added'),
  getUserDictionaryDirectory: vi.fn().mockResolvedValue('user-dictionaries'),
  listUserDictionaries: vi.fn().mockResolvedValue([]),
  lookupLexicon: vi.fn().mockResolvedValue([]),
  listModels: vi.fn().mockRejectedValue(new Error('offline')),
  listHistory: vi.fn().mockResolvedValue([]),
  clearHistory: vi.fn().mockResolvedValue(undefined),
  setHistoryFavorite: vi.fn().mockResolvedValue(undefined),
  exportHistory: vi.fn().mockResolvedValue('history.csv'),
  updateSettings: vi.fn().mockResolvedValue(undefined),
  translate: vi.fn(),
}));

describe('translation workspace', () => {
  afterEach(cleanup);

  it('highlights literal search text without treating it as a pattern', () => {
    render(<div>{highlightMatches('Список (последний)', 'список')}</div>);
    expect(screen.getByText('Список').tagName).toBe('MARK');
  });

  it('renders the empty translation state', () => {
    render(<App />);
    expect(
      screen.getByRole('heading', { name: 'LingvoLoc' }),
    ).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText('Write something to translate…'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Swap languages' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Refresh models' }),
    ).toBeInTheDocument();
  });

  it('sends on Enter but keeps Shift+Enter for a new line', () => {
    render(<App />);
    const input = screen.getByPlaceholderText('Write something to translate…');

    expect(fireEvent.keyDown(input, { key: 'Enter' })).toBe(false);
    expect(fireEvent.keyDown(input, { key: 'Enter', shiftKey: true })).toBe(
      true,
    );
  });

  it('opens the runtime settings from the header icon', () => {
    render(<App />);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Close settings' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens the dictionary settings from the header icon', () => {
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: 'Dictionaries' }));
    expect(screen.getByRole('dialog', { name: 'Dictionaries' })).toBeVisible();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('opens the Additional section when a dictionary lookup starts', () => {
    const { container } = render(<App />);
    const section = container.querySelector('details.additional-options')!;
    expect(section).not.toHaveAttribute('open');
    expect(screen.getByText('Additional')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Dictionary word'), {
      target: { value: 'good' },
    });
    fireEvent.keyDown(screen.getByLabelText('Dictionary word'), {
      key: 'Enter',
    });
    expect(section).toHaveAttribute('open');
  });

  it('shows Auto as the target while source detection is automatic', () => {
    render(<App />);
    const targetSelect = screen.getAllByLabelText('To').at(-1)!;

    expect(targetSelect).toHaveValue('auto');
    expect(targetSelect).toBeDisabled();
  });
});
