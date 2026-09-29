import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import DocumentsPanel from './DocumentsPanel';
import { highlightMatches } from './highlight';

const dialogMocks = vi.hoisted(() => ({
  open: vi.fn(),
  save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => dialogMocks);

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
  analyzeTxt: vi.fn(),
  analyzeDocx: vi.fn(),
  analyzeEpub: vi.fn(),
  startTxtJob: vi.fn(),
  startDocxJob: vi.fn(),
  startEpubJob: vi.fn(),
  resumeTxtJob: vi.fn(),
  resumeDocxJob: vi.fn(),
  resumeEpubJob: vi.fn(),
  getDocumentJob: vi.fn().mockRejectedValue(new Error('no document job')),
  pauseDocumentJob: vi.fn(),
  cancelDocumentJob: vi.fn(),
  exportTxtJob: vi.fn(),
  exportDocxJob: vi.fn(),
  exportEpubJob: vi.fn(),
}));

import * as commands from '../lib/commands';

const epubView = (state = 'ready', translatedBlocks = 0) => ({
  job: {
    id: 'epub-job',
    source_path: 'C:\\books\\story.epub',
    source_hash: 'hash',
    format: 'epub',
    parser_version: 'epub-v1',
    source_language: 'ru',
    target_language: 'en',
    runtime_snapshot: 'runtime',
    configuration_version: 'epub-v1',
    state,
  },
  blocks: [],
  diagnostics: [],
  translated_blocks: translatedBlocks,
  total_blocks: 1,
  paused: state === 'paused',
  cancelled: false,
});

describe('translation workspace', () => {
  afterEach(() => {
    cleanup();
    localStorage.clear();
    vi.clearAllMocks();
  });

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

  it('keeps dictionary and extension settings inside the settings window', () => {
    render(<App />);
    expect(
      screen.queryByRole('button', { name: 'Dictionaries' }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(screen.getByLabelText('Dictionaries')).toBeVisible();
    expect(screen.getByLabelText('Browser extension')).toBeVisible();
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

  it('accepts EPUB files and dispatches EPUB analysis', async () => {
    dialogMocks.open.mockResolvedValue('C:\\books\\story.epub');
    vi.mocked(commands.analyzeEpub).mockResolvedValue(epubView());

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /choose/i }));

    await waitFor(() =>
      expect(commands.analyzeEpub).toHaveBeenCalledWith(
        'C:\\books\\story.epub',
        'ru',
        'en',
      ),
    );
    expect(dialogMocks.open).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [
          expect.objectContaining({
            extensions: ['txt', 'docx', 'epub'],
          }),
        ],
      }),
    );
  });

  it('dispatches EPUB start and resume commands by job format', async () => {
    vi.mocked(commands.getDocumentJob).mockRejectedValue(new Error('no job'));
    vi.mocked(commands.startEpubJob).mockResolvedValue(
      epubView('translating', 1),
    );
    vi.mocked(commands.resumeEpubJob).mockResolvedValue(
      epubView('translating', 1),
    );

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );
    dialogMocks.open.mockResolvedValueOnce('C:\\books\\story.epub');
    vi.mocked(commands.analyzeEpub).mockResolvedValue(epubView());
    fireEvent.click(screen.getByRole('button', { name: /choose/i }));
    await screen.findByRole('button', { name: 'Start translation' });
    fireEvent.click(screen.getByRole('button', { name: 'Start translation' }));
    await waitFor(() =>
      expect(commands.startEpubJob).toHaveBeenCalledWith('epub-job'),
    );

    vi.mocked(commands.getDocumentJob).mockResolvedValue(
      epubView('interrupted'),
    );
    cleanup();
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );
    await screen.findByRole('button', { name: 'Resume' });
    fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
    await waitFor(() =>
      expect(commands.resumeEpubJob).toHaveBeenCalledWith('epub-job'),
    );
  });

  it('selects an EPUB output and dispatches EPUB export', async () => {
    vi.mocked(commands.getDocumentJob).mockResolvedValue(
      epubView('translating', 1),
    );
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    dialogMocks.save.mockResolvedValue('C:\\books\\story.translated.en.epub');
    vi.mocked(commands.exportEpubJob).mockResolvedValue({
      output_path: 'C:\\books\\story.translated.en.epub',
      job: epubView('completed', 1),
    });

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );
    await screen.findByRole('button', { name: 'Export translated EPUB' });
    fireEvent.click(
      screen.getByRole('button', { name: 'Export translated EPUB' }),
    );

    await waitFor(() =>
      expect(commands.exportEpubJob).toHaveBeenCalledWith(
        'epub-job',
        'C:\\books\\story.translated.en.epub',
      ),
    );
    expect(dialogMocks.save).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: 'C:\\books\\story.translated.en.epub',
        filters: [expect.objectContaining({ extensions: ['epub'] })],
      }),
    );
  });
});
