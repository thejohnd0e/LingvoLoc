import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import App from './App';
import { highlightMatches } from './highlight';

vi.mock('../lib/commands', () => ({
  getRuntimeStatus: vi.fn().mockRejectedValue(new Error('offline')),
  getUserDictionaryDirectory: vi.fn().mockResolvedValue('user-dictionaries'),
  listUserDictionaries: vi.fn().mockResolvedValue([]),
  listModels: vi.fn().mockRejectedValue(new Error('offline')),
  listHistory: vi.fn().mockResolvedValue([]),
  clearHistory: vi.fn().mockResolvedValue(undefined),
  setHistoryFavorite: vi.fn().mockResolvedValue(undefined),
  exportHistory: vi.fn().mockResolvedValue('history.csv'),
  updateSettings: vi.fn().mockResolvedValue(undefined),
  translate: vi.fn(),
}));

describe('translation workspace', () => {
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

  it('shows Auto as the target while source detection is automatic', () => {
    render(<App />);
    const targetSelect = screen.getAllByLabelText('To').at(-1)!;

    expect(targetSelect).toHaveValue('auto');
    expect(targetSelect).toBeDisabled();
  });
});
