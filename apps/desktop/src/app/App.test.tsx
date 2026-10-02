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
import { formatEta } from './documentProgress';
import { highlightMatches } from './highlight';

const dialogMocks = vi.hoisted(() => ({
  open: vi.fn(),
  save: vi.fn(),
}));
const openerMocks = vi.hoisted(() => ({
  openPath: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/plugin-dialog', () => dialogMocks);
vi.mock('@tauri-apps/plugin-opener', () => openerMocks);

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
  detectLanguage: vi.fn().mockResolvedValue({ code: 'en', confidence: 1 }),
  updateSettings: vi.fn().mockResolvedValue(undefined),
  getProviderCredentialStatus: vi.fn().mockResolvedValue({
    configured: false,
    hint: null,
  }),
  saveProviderCredential: vi.fn().mockResolvedValue({
    configured: true,
    hint: '••••1234',
  }),
  deleteProviderCredential: vi.fn().mockResolvedValue(undefined),
  getSessionUsage: vi.fn().mockResolvedValue([
    {
      providerId: 'openAi',
      modelId: 'gpt-4o-mini',
      requests: 2,
      failedRequests: 1,
      inputTokens: 120,
      outputTokens: 80,
      totalTokens: 200,
      billedCharacters: null,
    },
    {
      providerId: 'deepL',
      modelId: 'deepL',
      requests: 1,
      failedRequests: 0,
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      billedCharacters: 450,
    },
  ]),
  translate: vi.fn(),
  translateWord: vi.fn(),
  analyzeTxt: vi.fn(),
  analyzeDocx: vi.fn(),
  analyzeEpub: vi.fn(),
  analyzeFb2: vi.fn(),
  analyzePdf: vi.fn(),
  startTxtJob: vi.fn(),
  startDocxJob: vi.fn(),
  startEpubJob: vi.fn(),
  startFb2Job: vi.fn(),
  startPdfJob: vi.fn(),
  resumeTxtJob: vi.fn(),
  resumeDocxJob: vi.fn(),
  resumeEpubJob: vi.fn(),
  resumeFb2Job: vi.fn(),
  resumePdfJob: vi.fn(),
  listDocumentJobs: vi.fn().mockResolvedValue([]),
  getDocumentProgress: vi.fn().mockRejectedValue(new Error('no progress')),
  getDocumentJob: vi.fn().mockRejectedValue(new Error('no document job')),
  pauseDocumentJob: vi.fn(),
  cancelDocumentJob: vi.fn(),
  clearDocumentJob: vi.fn().mockResolvedValue(undefined),
  exportTxtJob: vi.fn(),
  exportDocxJob: vi.fn(),
  exportEpubJob: vi.fn(),
  exportFb2Job: vi.fn(),
  exportPdfJob: vi.fn(),
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
    translation_style: 'neutral' as const,
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

const documentCases = [
  {
    label: 'TXT',
    extension: 'txt',
    format: 'txt',
    analyze: commands.analyzeTxt,
    start: commands.startTxtJob,
    resume: commands.resumeTxtJob,
    export: commands.exportTxtJob,
  },
  {
    label: 'DOCX',
    extension: 'docx',
    format: 'docx',
    analyze: commands.analyzeDocx,
    start: commands.startDocxJob,
    resume: commands.resumeDocxJob,
    export: commands.exportDocxJob,
  },
  {
    label: 'EPUB',
    extension: 'epub',
    format: 'epub',
    analyze: commands.analyzeEpub,
    start: commands.startEpubJob,
    resume: commands.resumeEpubJob,
    export: commands.exportEpubJob,
  },
  {
    label: 'FB2',
    extension: 'fb2',
    format: 'fb2',
    analyze: commands.analyzeFb2,
    start: commands.startFb2Job,
    resume: commands.resumeFb2Job,
    export: commands.exportFb2Job,
  },
] as const;

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

  it('shows cloud provider settings and aggregated session usage', async () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({
        runtimeMode: 'openAi',
        cloud: {
          consentAccepted: true,
          openAi: {
            modelId: 'gpt-4o-mini',
            availableModels: [],
            modelsRefreshedAt: null,
          },
        },
      }),
    );
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    expect(
      screen.getByRole('combobox', { name: 'Cloud provider' }),
    ).toHaveValue('openAi');
    expect(screen.getByText('OpenAI API key')).toBeInTheDocument();
    expect(await screen.findByText('Session usage')).toBeInTheDocument();
    expect(screen.getByText('200 tokens')).toBeInTheDocument();
    expect(screen.getByText('450 characters')).toBeInTheDocument();
  });

  it('uses the selected cloud model for the main translation action', async () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({
        runtimeMode: 'openAi',
        cloud: {
          consentAccepted: true,
          openAi: {
            modelId: 'gpt-4o-mini',
            availableModels: [],
            modelsRefreshedAt: null,
          },
        },
      }),
    );
    vi.mocked(commands.translate).mockResolvedValue({
      text: 'Translated',
      model_id: 'gpt-4o-mini',
      adapter_id: 'generic',
      latency_ms: 10,
    });
    render(<App />);
    const input = screen.getByPlaceholderText('Write something to translate…');
    fireEvent.change(input, { target: { value: 'Hello' } });
    const button = screen.getByRole('button', { name: /Translate/ });
    expect(button).not.toBeDisabled();
    fireEvent.click(button);
    await waitFor(() =>
      expect(commands.translate).toHaveBeenCalledWith(
        expect.objectContaining({ model_id: 'gpt-4o-mini' }),
      ),
    );
  });

  it('sends the saved translation style and neutral word alignment requests', async () => {
    localStorage.setItem(
      'lingvoloc.settings',
      JSON.stringify({ translationStyle: 'technical', modelId: 'model' }),
    );
    vi.mocked(commands.translate).mockResolvedValue({
      text: 'Привет',
      model_id: 'model',
      adapter_id: 'translategemma',
      latency_ms: 10,
      prompt_tokens: null,
      completion_tokens: null,
      total_tokens: null,
    });
    vi.mocked(commands.detectLanguage).mockResolvedValue({
      code: 'en',
      confidence: 1,
    });
    vi.mocked(commands.translateWord).mockResolvedValue({
      text: 'Привет',
      model_id: 'model',
      adapter_id: 'translategemma',
      latency_ms: 10,
      prompt_tokens: null,
      completion_tokens: null,
      total_tokens: null,
    });
    render(<App />);
    fireEvent.change(
      screen.getByPlaceholderText('Write something to translate…'),
      {
        target: { value: 'Hello' },
      },
    );
    fireEvent.click(screen.getByRole('button', { name: /Translate/ }));
    await waitFor(() => expect(commands.translate).toHaveBeenCalled());
    expect(commands.translate).toHaveBeenCalledWith(
      expect.objectContaining({ translation_style: 'technical' }),
    );
  });

  it('uses neutral style when aligning a selected source word', async () => {
    vi.mocked(commands.lookupLexicon).mockResolvedValue([]);
    vi.mocked(commands.detectLanguage).mockResolvedValue({
      code: 'en',
      confidence: 1,
    });
    vi.mocked(commands.translate).mockResolvedValue({
      text: 'Привет',
      model_id: 'model',
      adapter_id: 'translategemma',
      latency_ms: 10,
      prompt_tokens: null,
      completion_tokens: null,
      total_tokens: null,
    });
    render(<App />);
    const input = screen.getByPlaceholderText(
      'Write something to translate…',
    ) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: 'Hello' } });
    input.focus();
    Object.defineProperty(input, 'selectionStart', {
      configurable: true,
      value: 0,
    });
    Object.defineProperty(input, 'selectionEnd', {
      configurable: true,
      value: 5,
    });
    fireEvent.doubleClick(input);

    await waitFor(() =>
      expect(commands.lookupLexicon).toHaveBeenCalledWith(
        'Hello',
        undefined,
        [],
        '',
      ),
    );
    await waitFor(() =>
      expect(commands.translateWord).toHaveBeenCalledWith(
        expect.objectContaining({
          text: 'Hello',
          translation_style: 'neutral',
        }),
      ),
    );
  });

  it('switches between text and file modes and remembers the choice', () => {
    render(<App />);
    expect(
      screen.getByPlaceholderText('Write something to translate…'),
    ).toBeVisible();
    expect(screen.getByLabelText('Documents')).not.toBeVisible();

    fireEvent.click(screen.getByRole('tab', { name: /Files/ }));
    expect(screen.getByLabelText('Documents')).toBeVisible();
    expect(
      screen.getByPlaceholderText('Write something to translate…'),
    ).not.toBeVisible();
    expect(localStorage.getItem('lingvoloc.mode')).toBe('files');

    fireEvent.keyDown(window, { key: '1', ctrlKey: true });
    expect(
      screen.getByPlaceholderText('Write something to translate…'),
    ).toBeVisible();
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

  it('persists the global translation style and keeps it visible in Files mode', () => {
    render(<App />);
    const selector = screen.getByLabelText('Translation style');
    expect(selector).toHaveValue('neutral');
    fireEvent.change(selector, { target: { value: 'technical' } });
    expect(commands.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({ translationStyle: 'technical' }),
    );
    expect(
      JSON.parse(localStorage.getItem('lingvoloc.settings')!).translationStyle,
    ).toBe('technical');
    fireEvent.click(screen.getByRole('tab', { name: /Files/ }));
    expect(screen.getByLabelText('Translation style')).toBeVisible();
  });

  it.each(documentCases)(
    'preserves $label picker, job dispatch, and output behavior',
    async ({
      label,
      extension,
      format,
      analyze,
      start,
      resume,
      export: exportJob,
    }) => {
      const sourcePath = `C:\\books\\story.${extension}`;
      const outputPath = `C:\\books\\story.translated.en.${extension}`;
      const view = (state = 'ready', translatedBlocks = 0) => ({
        ...epubView(state, translatedBlocks),
        job: {
          ...epubView(state, translatedBlocks).job,
          format,
          source_path: sourcePath,
        },
      });

      dialogMocks.open.mockResolvedValue(sourcePath);
      vi.mocked(analyze).mockResolvedValue(view());
      render(
        <DocumentsPanel
          sourceLanguage="ru"
          targetLanguage="en"
          modelId="model"
        />,
      );
      fireEvent.click(screen.getByRole('button', { name: /choose/i }));
      await screen.findByRole('button', { name: 'Start translation' });
      expect(analyze).toHaveBeenCalledWith(sourcePath, 'ru', 'en', 'neutral');
      expect(dialogMocks.open).toHaveBeenCalledWith(
        expect.objectContaining({
          filters: [
            expect.objectContaining({
              extensions: ['txt', 'docx', 'epub', 'fb2', 'pdf'],
            }),
          ],
        }),
      );

      vi.mocked(start).mockResolvedValue(view('translating', 1));
      fireEvent.click(
        screen.getByRole('button', { name: 'Start translation' }),
      );
      await waitFor(() => expect(start).toHaveBeenCalledWith('epub-job'));

      cleanup();
      localStorage.clear();
      localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
      vi.mocked(commands.getDocumentJob).mockResolvedValue(view('interrupted'));
      render(
        <DocumentsPanel
          sourceLanguage="ru"
          targetLanguage="en"
          modelId="model"
        />,
      );
      await screen.findByRole('button', { name: 'Resume' });
      vi.mocked(resume).mockResolvedValue(view('translating', 1));
      fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
      await waitFor(() => expect(resume).toHaveBeenCalledWith('epub-job'));

      cleanup();
      localStorage.clear();
      localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
      vi.mocked(commands.getDocumentJob).mockResolvedValue(
        view('translating', 1),
      );
      dialogMocks.save.mockResolvedValue(outputPath);
      vi.mocked(exportJob).mockResolvedValue({
        output_path: outputPath,
        job: view('completed', 1),
      });
      render(
        <DocumentsPanel
          sourceLanguage="ru"
          targetLanguage="en"
          modelId="model"
        />,
      );
      await screen.findByRole('button', { name: `Export translated ${label}` });
      fireEvent.click(
        screen.getByRole('button', { name: `Export translated ${label}` }),
      );
      await waitFor(() =>
        expect(exportJob).toHaveBeenCalledWith('epub-job', outputPath),
      );
      expect(dialogMocks.save).toHaveBeenCalledWith(
        expect.objectContaining({
          defaultPath: outputPath,
          filters: [expect.objectContaining({ extensions: [extension] })],
        }),
      );
    },
  );

  it('sends the PDF page range and offers to analyze again when it changes', async () => {
    const sourcePath = 'C:/books/manual.pdf';
    const view = (pages: string) => ({
      ...epubView('ready'),
      job: {
        ...epubView('ready').job,
        format: 'pdf',
        configuration_version: pages ? `pdf-v1;pages=${pages}` : 'pdf-v1',
        source_path: sourcePath,
      },
    });
    dialogMocks.open.mockResolvedValue(sourcePath);
    vi.mocked(commands.analyzePdf).mockResolvedValue(view('1-6'));
    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );
    fireEvent.change(screen.getByLabelText('PDF page ranges'), {
      target: { value: '1-6' },
    });
    fireEvent.click(screen.getByRole('button', { name: /choose/i }));
    await screen.findByRole('button', { name: 'Start translation' });
    expect(commands.analyzePdf).toHaveBeenCalledWith(
      sourcePath,
      'ru',
      'en',
      'neutral',
      '1-6',
    );
    expect(screen.getByText('Pages: 1-6')).toBeTruthy();
    expect(screen.queryByText(/Analyze again/)).toBeNull();

    fireEvent.change(screen.getByLabelText('PDF page ranges'), {
      target: { value: '7-9' },
    });
    expect(await screen.findByText(/Analyze again with pages/)).toBeTruthy();
  });

  it('keeps the full document filename available when displaying a long FB2 basename', async () => {
    const basename = `${'verylongfilename'.repeat(10)}.fb2`;
    const fb2Job = {
      ...epubView('ready'),
      job: {
        ...epubView('ready').job,
        format: 'fb2',
        parser_version: 'fb2-v1',
        configuration_version: 'fb2-v1',
        source_path: `C:\\books\\${basename}`,
      },
    };
    localStorage.setItem('lingvoloc.document-job-id', 'fb2-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue(fb2Job);

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    expect(await screen.findByText(basename)).toHaveAttribute(
      'title',
      basename,
    );
  });

  it('collapses duplicate diagnostics into one counted list item', async () => {
    const diagnostics = [
      'Unsupported script content',
      'Unsupported script content',
    ];
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue({
      ...epubView('ready'),
      diagnostics,
    });
    const consoleError = vi
      .spyOn(console, 'error')
      .mockImplementation(() => {});

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    const list = await screen.findByRole('list', {
      name: 'Document diagnostics',
    });
    expect(list.querySelectorAll('li')).toHaveLength(1);
    expect(
      screen.getByText('Unsupported script content (×2)'),
    ).toBeInTheDocument();
    expect(consoleError).not.toHaveBeenCalled();
    consoleError.mockRestore();
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
        'neutral',
      ),
    );
    expect(dialogMocks.open).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [
          expect.objectContaining({
            extensions: ['txt', 'docx', 'epub', 'fb2', 'pdf'],
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

  it('keeps Cancel enabled while a document translation is still running', async () => {
    let resolveStart: ((view: ReturnType<typeof epubView>) => void) | undefined;
    vi.mocked(commands.getDocumentJob).mockResolvedValue(
      epubView('translating'),
    );
    vi.mocked(commands.analyzeEpub).mockResolvedValue(epubView());
    vi.mocked(commands.startEpubJob).mockReturnValue(
      new Promise((resolve) => {
        resolveStart = resolve;
      }),
    );
    dialogMocks.open.mockResolvedValue('C:\\books\\story.epub');

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /choose/i }));
    await screen.findByRole('button', { name: 'Start translation' });
    fireEvent.click(screen.getByRole('button', { name: 'Start translation' }));

    const cancel = await screen.findByRole('button', { name: 'Cancel' });
    expect(cancel).toBeEnabled();

    resolveStart?.(epubView('cancelled'));
    await screen.findByRole('button', { name: 'Clear job' });
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

    fireEvent.click(await screen.findByRole('button', { name: 'Open output' }));
    expect(openerMocks.openPath).toHaveBeenCalledWith(
      'C:\\books\\story.translated.en.epub',
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Open folder' }));
    expect(openerMocks.openPath).toHaveBeenCalledWith('C:\\books');

    openerMocks.openPath.mockRejectedValueOnce(new Error('access denied'));
    fireEvent.click(screen.getByRole('button', { name: 'Open output' }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Open output failed · access denied',
    );
  });

  it('allows clearing a completed EPUB job with warnings', async () => {
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue(
      epubView('completed_with_warnings', 1),
    );

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Clear job' }));
    expect(screen.queryByText('story.epub')).not.toBeInTheDocument();
    expect(localStorage.getItem('lingvoloc.document-job-id')).toBeNull();
  });

  it('translates ready files one after another and exports each', async () => {
    const view = (id: string, state: string, done: number) => {
      const base = epubView(state, done);
      return {
        ...base,
        job: { ...base.job, id, source_path: `C:\\books\\${id}.epub` },
      };
    };
    const summary = (id: string) => ({
      job: view(id, 'ready', 0).job,
      total_blocks: 1,
      translated_blocks: 0,
    });
    vi.mocked(commands.listDocumentJobs).mockResolvedValue([
      summary('b'),
      summary('a'),
    ]);
    vi.mocked(commands.getDocumentJob).mockImplementation(async (id) =>
      view(id, 'ready', 0),
    );
    vi.mocked(commands.startEpubJob).mockImplementation(async (id) =>
      view(id, 'translating', 1),
    );
    vi.mocked(commands.exportEpubJob).mockImplementation(async (id) => ({
      output_path: `C:\\books\\${id}.translated.en.epub`,
      job: view(id, 'completed', 1),
    }));

    try {
      render(
        <DocumentsPanel
          sourceLanguage="ru"
          targetLanguage="en"
          modelId="model"
        />,
      );

      fireEvent.click(
        await screen.findByRole('button', {
          name: 'Translate 2 ready files in a row',
        }),
      );

      await waitFor(() =>
        expect(commands.exportEpubJob).toHaveBeenCalledTimes(2),
      );
      expect(
        vi.mocked(commands.startEpubJob).mock.calls.map((call) => call[0]),
      ).toEqual(['a', 'b']);
    } finally {
      vi.mocked(commands.listDocumentJobs).mockResolvedValue([]);
      vi.mocked(commands.getDocumentJob).mockRejectedValue(
        new Error('no document job'),
      );
    }
  });

  it('lets the user choose a new file after cancelling and clearing a running job', async () => {
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue(epubView('ready'));
    // The native start call keeps running after the job is cancelled and cleared.
    vi.mocked(commands.startEpubJob).mockReturnValue(new Promise(() => {}));
    vi.mocked(commands.cancelDocumentJob).mockResolvedValue(
      epubView('cancelled'),
    );
    vi.mocked(commands.clearDocumentJob).mockResolvedValue(undefined);

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    fireEvent.click(
      await screen.findByRole('button', { name: 'Start translation' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Clear job' }));

    await waitFor(() =>
      expect(screen.getByRole('button', { name: /choose/i })).toBeEnabled(),
    );
  });

  it('shows a failed job error and allows resuming its translated blocks', async () => {
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue({
      ...epubView('failed', 56),
      job: {
        ...epubView('failed', 56).job,
        error: 'Timeout contacting the local model',
      },
    });
    vi.mocked(commands.resumeEpubJob).mockResolvedValue(
      epubView('translating', 56),
    );

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    expect(
      await screen.findByText('Timeout contacting the local model'),
    ).toBeInTheDocument();
    fireEvent.click(await screen.findByRole('button', { name: 'Resume' }));
    await waitFor(() =>
      expect(commands.resumeEpubJob).toHaveBeenCalledWith('epub-job'),
    );
  });

  it('allows clearing an interrupted document job', async () => {
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue(
      epubView('interrupted', 54),
    );

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Clear job' }));
    await waitFor(() => {
      expect(commands.clearDocumentJob).toHaveBeenCalledWith('epub-job');
      expect(screen.queryByText('story.epub')).not.toBeInTheDocument();
      expect(localStorage.getItem('lingvoloc.document-job-id')).toBeNull();
    });
  });

  it('does not restore a cleared job from pending translation polling', async () => {
    localStorage.setItem('lingvoloc.document-job-id', 'epub-job');
    vi.mocked(commands.getDocumentJob).mockResolvedValue(
      epubView('failed', 54),
    );
    vi.mocked(commands.resumeEpubJob).mockReturnValue(new Promise(() => {}));

    render(
      <DocumentsPanel
        sourceLanguage="ru"
        targetLanguage="en"
        modelId="model"
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Resume' }));
    fireEvent.click(screen.getByRole('button', { name: 'Clear job' }));

    await waitFor(() =>
      expect(commands.clearDocumentJob).toHaveBeenCalledWith('epub-job'),
    );
    await new Promise((resolve) => window.setTimeout(resolve, 900));
    expect(screen.queryByText('story.epub')).not.toBeInTheDocument();
  });

  it('formats document ETA from elapsed progress', () => {
    expect(formatEta(15, 33, 10_000, 70_000)).toBe('~1m 12s');
    expect(formatEta(0, 33, 10_000, 70_000)).toBeNull();
  });
});
