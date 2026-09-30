import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useEffect, useRef, useState } from 'react';
import {
  getRuntimeStatus,
  getApiToken,
  checkLlamaServer,
  downloadLlamaCpp,
  getGpuInfo,
  getLlamaDevices,
  llamaPathStatus,
  addLlamaToPath,
  type GpuInfo,
  type LlamaDevices,
  findLlamaServer,
  type LlamaDownloadProgress,
  locateLlamaServer,
  listUserDictionaries,
  readDictionaryMedia,
  detectLanguage,
  formatTiming,
  clearHistory,
  exportHistory,
  listModels,
  listHistory,
  lookupLexicon,
  setHistoryFavorite,
  translate,
  translateWord,
  writeClipboard,
  updateSettings as updateNativeSettings,
  type HistoryEntry,
  type LexicalEntry,
  type LocalModel,
  type UserDictionary,
} from '../lib/commands';
import {
  loadSettings,
  loadTextScale,
  saveSettings,
  type Settings,
} from '../lib/settings';
import { errorDetail } from '../lib/errors';
import { targetForDetectedLanguage } from '../lib/languagePair';
import { highlightMatches } from './highlight';
import Modal from './Modal';
import Spinner from './Spinner';
import TextSizeControls from './TextSizeControls';
import DocumentsPanel from './DocumentsPanel';
import { sanitizeDictionaryHtml } from '../lib/dictionaryHtml';

const modeKey = 'lingvoloc.mode';

const defaultSettings: Settings = {
  runtimeMode: 'standalone',
  modelsDirectory: '',
  llamaServerPath: '',
  endpoint: 'http://127.0.0.1:1234/v1',
  modelId: '',
  adapterId: 'translategemma',
  sourceLanguage: 'auto',
  targetLanguage: 'en',
  primaryLanguage: 'en',
  secondaryLanguage: 'ru',
};

const enabledDictionariesKey = 'lingvoloc.enabled-dictionaries';
const dictionaryPathKey = 'lingvoloc.dictionary-path';

function loadEnabledDictionaries(): string[] {
  try {
    const value = JSON.parse(
      localStorage.getItem(enabledDictionariesKey) ?? '[]',
    );
    return Array.isArray(value)
      ? value.filter((item) => typeof item === 'string')
      : [];
  } catch {
    return [];
  }
}

function loadDictionaryPath(): string {
  return localStorage.getItem(dictionaryPathKey) ?? '';
}

async function fitWindowToContent() {
  try {
    const currentWindow = getCurrentWindow();
    const [scaleFactor, currentSize] = await Promise.all([
      currentWindow.scaleFactor(),
      currentWindow.innerSize(),
    ]);
    const availableHeight = Math.max(window.screen.availHeight, 720);
    const shell = document.querySelector<HTMLElement>('.shell');
    const contentHeight = (shell?.getBoundingClientRect().height ?? 0) + 48;
    const height = Math.min(contentHeight, availableHeight - 24);
    await currentWindow.setSize(
      new LogicalSize(currentSize.width / scaleFactor, Math.max(600, height)),
    );
  } catch {
    // The browser dev server does not expose a native window to resize.
  }
}

export default function App() {
  const [settings, setSettings] = useState(() => loadSettings(defaultSettings));
  const [textScale, setTextScale] = useState(loadTextScale);
  const [source, setSource] = useState('');
  const [translation, setTranslation] = useState('');
  const [models, setModels] = useState<LocalModel[]>([]);
  const [status, setStatus] = useState('Checking runtime…');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [timing, setTiming] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [detectedLanguage, setDetectedLanguage] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [historyQuery, setHistoryQuery] = useState('');
  const [historyInput, setHistoryInput] = useState('');
  const [hasMoreHistory, setHasMoreHistory] = useState(false);
  const [historyMessage, setHistoryMessage] = useState('');
  const [notice, setNotice] = useState('');
  const [lexicalQuery, setLexicalQuery] = useState('');
  const [lexicalResults, setLexicalResults] = useState<LexicalEntry[]>([]);
  const [lexicalMessage, setLexicalMessage] = useState('');
  const [selectedDictionaryPath, setSelectedDictionaryPath] =
    useState(loadDictionaryPath);
  const [userDictionaries, setUserDictionaries] = useState<UserDictionary[]>(
    [],
  );
  const [enabledDictionaries, setEnabledDictionaries] = useState(
    loadEnabledDictionaries,
  );
  const [translationHighlight, setTranslationHighlight] = useState('');
  const [serverCheck, setServerCheck] = useState('');
  const [gpuInfo, setGpuInfo] = useState<GpuInfo | null>(null);
  const [llamaDevices, setLlamaDevices] = useState<LlamaDevices | null>(null);
  const [inPath, setInPath] = useState<boolean | null>(null);
  const [pathBusy, setPathBusy] = useState(false);
  const [llamaDownload, setLlamaDownload] =
    useState<LlamaDownloadProgress | null>(null);
  const [mode, setMode] = useState<'text' | 'files'>(() => {
    try {
      return localStorage.getItem(modeKey) === 'files' ? 'files' : 'text';
    } catch {
      return 'text';
    }
  });
  const [fileProgress, setFileProgress] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [additionalOpen, setAdditionalOpen] = useState(false);
  const [lexicalBusy, setLexicalBusy] = useState(false);
  const [dictionariesBusy, setDictionariesBusy] = useState(false);
  const sourceInputRef = useRef<HTMLTextAreaElement>(null);
  const settingsRef = useRef(settings);
  const lexicalRequestId = useRef(0);
  const autoSized = useRef(false);
  const translateFromClipboard = useRef<(input: string) => Promise<void>>(
    async () => undefined,
  );
  settingsRef.current = settings;
  const runtimeLabel =
    settings.runtimeMode === 'standalone' ? 'Standalone' : 'LM Studio';
  const runtimeLabelRef = useRef(runtimeLabel);
  runtimeLabelRef.current = runtimeLabel;

  useEffect(() => {
    document.documentElement.style.setProperty(
      '--text-scale',
      String(textScale),
    );
    window.setTimeout(() => void fitWindowToContent(), 0);
  }, [textScale]);

  useEffect(() => {
    // Native settings must be current before the runtime is queried, otherwise a
    // freshly started app would list models for the default LM Studio mode.
    void updateNativeSettings(settingsRef.current)
      .catch(() => undefined)
      .then(() =>
        Promise.allSettled([getRuntimeStatus(), listModels(), listHistory()]),
      )
      .then(([runtimeResult, modelsResult, historyResult]) => {
        if (runtimeResult.status === 'fulfilled') {
          setStatus(runtimeResult.value.detail);
        } else {
          setStatus(
            `${runtimeLabelRef.current} error · ${errorDetail(runtimeResult.reason)}`,
          );
        }
        if (modelsResult.status === 'fulfilled') {
          const availableModels = modelsResult.value;
          setModels(availableModels);
          setError((current) =>
            current.startsWith('Model list error') ? '' : current,
          );
          const savedModel = availableModels.find(
            (model) => model.id === settingsRef.current.modelId,
          )?.id;
          const modelId = savedModel ?? availableModels[0]?.id ?? '';
          if (modelId !== settingsRef.current.modelId) {
            setSettings((current) => {
              const next = { ...current, modelId };
              saveSettings(next);
              void updateNativeSettings(next).catch(() => undefined);
              return next;
            });
          } else if (modelId) {
            void updateNativeSettings(settingsRef.current).catch(
              () => undefined,
            );
          }
        } else {
          setError(`Model list error · ${errorDetail(modelsResult.reason)}`);
        }
        if (historyResult.status === 'fulfilled') {
          setHistory(historyResult.value);
          setHasMoreHistory(historyResult.value.length === 20);
        }
      })
      .finally(() => {
        if (autoSized.current) return;
        autoSized.current = true;
        [0, 300, 1000].forEach((delay) => {
          window.setTimeout(() => void fitWindowToContent(), delay);
        });
      });
  }, [
    settings.modelId,
    settings.runtimeMode,
    settings.modelsDirectory,
    settings.llamaServerPath,
  ]);

  useEffect(() => {
    void refreshUserDictionaries(loadDictionaryPath());
    // This initialization intentionally runs once; the selected folder is persisted locally.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    let active = true;
    const media = document.querySelectorAll<HTMLElement>(
      '.dictionary-definition-html [data-dictionary-media-resource]',
    );
    for (const element of media) {
      const resource = element.getAttribute('data-dictionary-media-resource');
      const directory = element
        .closest('.dictionary-media')
        ?.getAttribute('data-dictionary-media-directory');
      if (!resource || !directory) continue;
      void readDictionaryMedia(directory, resource)
        .then((source) => {
          if (active && element.isConnected)
            element.setAttribute('src', source);
        })
        .catch(() => element.remove());
    }
    return () => {
      active = false;
    };
  }, [lexicalResults]);

  function updateSettings(patch: Partial<Settings>) {
    const next = { ...settings, ...patch };
    setSettings(next);
    saveSettings(next);
    void updateNativeSettings(next).catch(() => undefined);
  }

  async function runTranslation(input = source) {
    if (!input.trim()) return;
    setLoading(true);
    setError('');
    setNotice('');
    setHistoryMessage('');
    try {
      const sourceLanguage =
        settings.sourceLanguage === 'auto'
          ? (await detectLanguage(input)).code
          : settings.sourceLanguage;
      const targetLanguage =
        settings.sourceLanguage === 'auto'
          ? targetForDetectedLanguage(
              sourceLanguage,
              settings.primaryLanguage,
              settings.secondaryLanguage,
            )
          : settings.targetLanguage;
      if (settings.sourceLanguage === 'auto') {
        setDetectedLanguage(sourceLanguage);
      }
      const result = await translate({
        model_id: settings.modelId,
        adapter_id: settings.adapterId,
        source_language: sourceLanguage,
        target_language: targetLanguage,
        text: input,
      });
      setTranslation(result.text);
      setTiming(formatTiming(result));
      void getRuntimeStatus()
        .then((runtime) => setStatus(runtime.detail))
        .catch(() => undefined);
      try {
        await writeClipboard(result.text);
        setNotice(`Translation copied to clipboard · ${formatTiming(result)}`);
      } catch {
        setNotice(
          `Translation completed, but clipboard access is unavailable · ${formatTiming(result)}`,
        );
      }
      try {
        const entries = await listHistory(historyQuery);
        setHistory(entries);
        setHasMoreHistory(entries.length === 20);
      } catch (reason) {
        setNotice(
          `Translation completed, but history could not refresh · ${errorDetail(reason)}`,
        );
      }
    } catch (reason) {
      setError(`Translation failed · ${errorDetail(reason)}`);
    } finally {
      setLoading(false);
    }
  }

  translateFromClipboard.current = runTranslation;

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return;
    let unlisten: (() => void) | undefined;
    void listen('global-translate-clipboard', async () => {
      try {
        const text = await navigator.clipboard.readText();
        if (!text.trim()) {
          setNotice('Clipboard is empty.');
          return;
        }
        setSource(text);
        await translateFromClipboard.current(text);
      } catch {
        setNotice('Clipboard access is unavailable.');
      }
    }).then((remove) => {
      unlisten = remove;
    });
    return () => unlisten?.();
  }, []);

  function swapLanguages() {
    if (settings.sourceLanguage === 'auto') {
      updateSettings({
        primaryLanguage: settings.secondaryLanguage,
        secondaryLanguage: settings.primaryLanguage,
      });
      return;
    }
    updateSettings({
      sourceLanguage: settings.targetLanguage,
      targetLanguage: settings.sourceLanguage,
    });
  }

  async function refreshModels() {
    setRefreshing(true);
    try {
      const availableModels = await listModels();
      setModels(availableModels);
      setStatus(`${runtimeLabel} · ${availableModels.length} models`);
    } catch (reason) {
      setStatus(`${runtimeLabel} unavailable · ${errorDetail(reason)}`);
    } finally {
      setRefreshing(false);
    }
  }

  async function copyExtensionToken() {
    try {
      const token = await getApiToken();
      await navigator.clipboard.writeText(token);
      setNotice('Extension pairing token copied to clipboard.');
    } catch {
      setNotice('Could not copy the extension pairing token.');
    }
  }

  async function removeHistory() {
    try {
      await clearHistory();
      setHistory([]);
      setHasMoreHistory(false);
      setHistoryMessage('History cleared.');
    } catch {
      setHistoryMessage('History clear failed.');
    }
  }

  async function searchHistory() {
    try {
      setHistoryQuery(historyInput);
      const entries = await listHistory(historyInput);
      setHistory(entries);
      setHasMoreHistory(entries.length === 20);
      setStatus(`History search · ${entries.length} matches`);
      setHistoryMessage(`${entries.length} matches found.`);
    } catch {
      const query = historyInput.trim().toLocaleLowerCase();
      const entries = history.filter(
        (entry) =>
          entry.source_text.toLocaleLowerCase().includes(query) ||
          entry.translated_text.toLocaleLowerCase().includes(query),
      );
      setHistoryQuery(historyInput);
      setHistory(entries);
      setHasMoreHistory(false);
      setHistoryMessage(
        `Native search unavailable; ${entries.length} loaded matches found.`,
      );
    }
  }

  async function toggleFavorite(entry: HistoryEntry) {
    try {
      await setHistoryFavorite(entry.id, !entry.favorite);
      setHistory(await listHistory(historyQuery));
    } catch {
      setError('Could not update favorite.');
    }
  }

  async function loadMoreHistory() {
    const entries = await listHistory(historyQuery, history.length);
    setHistory((current) => [...current, ...entries]);
    setHasMoreHistory(entries.length === 20);
  }

  async function exportTranslationHistory() {
    const csvField = (value: string) => `"${value.replaceAll('"', '""')}"`;
    const csv = [
      'id,source_text,translated_text,source_language,target_language,model_id,created_at,favorite',
      ...history.map((entry) =>
        [
          entry.id,
          csvField(entry.source_text),
          csvField(entry.translated_text),
          entry.source_language,
          entry.target_language,
          csvField(entry.model_id),
          entry.created_at,
          entry.favorite,
        ].join(','),
      ),
    ].join('\n');
    try {
      const path = await exportHistory();
      setStatus(`History exported · ${path}`);
      setHistoryMessage('CSV exported to Downloads.');
    } catch {
      setHistoryMessage(
        'Native export unavailable; downloading CSV from the UI.',
      );
    }
    const link = document.createElement('a');
    link.href = URL.createObjectURL(
      new Blob([csv], { type: 'text/csv;charset=utf-8' }),
    );
    link.download = 'LingvoLoc-history.csv';
    link.click();
    URL.revokeObjectURL(link.href);
  }

  async function lookupWord(query = lexicalQuery) {
    const requestId = ++lexicalRequestId.current;
    setLexicalResults([]);
    setLexicalMessage('Looking up…');
    setLexicalBusy(true);
    setAdditionalOpen(true);
    try {
      const entries = await lookupLexicon(
        query,
        undefined,
        enabledDictionaries,
        selectedDictionaryPath,
      );
      if (requestId !== lexicalRequestId.current) return;
      setLexicalBusy(false);
      setLexicalQuery(query);
      setLexicalResults(entries);
      setLexicalMessage(
        entries.length
          ? `${entries.length} dictionary match${entries.length === 1 ? '' : 'es'}.`
          : 'No local dictionary match.',
      );
    } catch {
      if (requestId !== lexicalRequestId.current) return;
      setLexicalResults([]);
      setLexicalMessage('Local dictionary is unavailable.');
      setLexicalBusy(false);
    }
  }

  async function refreshUserDictionaries(
    directory = selectedDictionaryPath,
  ): Promise<UserDictionary[]> {
    setDictionariesBusy(true);
    try {
      const dictionaries = await listUserDictionaries(directory);
      setUserDictionaries(dictionaries);
      setEnabledDictionaries((current) => {
        const available = new Set(
          dictionaries.map((dictionary) => dictionary.id),
        );
        const next = current.filter((id) => available.has(id));
        localStorage.setItem(enabledDictionariesKey, JSON.stringify(next));
        return next;
      });
      setLexicalMessage(
        dictionaries.length
          ? `${dictionaries.length} user dictionary${dictionaries.length === 1 ? '' : 'ies'} found.`
          : 'No user dictionaries found.',
      );
      return dictionaries;
    } catch (reason) {
      setLexicalMessage(`Dictionary refresh failed · ${errorDetail(reason)}`);
      return [];
    } finally {
      setDictionariesBusy(false);
    }
  }

  function changeRuntimeMode(runtimeMode: Settings['runtimeMode']) {
    setModels([]);
    setServerCheck('');
    updateSettings({ runtimeMode, modelId: '' });
  }

  async function chooseModelsFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== 'string') return;
    updateSettings({ modelsDirectory: selected, modelId: '' });
  }

  async function verifyLlamaServer(path: string) {
    setServerCheck('Checking…');
    try {
      setServerCheck(`OK · ${await checkLlamaServer(path)}`);
    } catch (reason) {
      setServerCheck(`Check failed · ${errorDetail(reason)}`);
    }
  }

  useEffect(() => {
    // The tray menu asks the main window to open its settings.
    let active = true;
    let stop: (() => void) | undefined;
    void listen('open-settings', () => setSettingsOpen(true))
      .then((unlisten) => {
        if (active) stop = unlisten;
        else unlisten();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      stop?.();
    };
  }, []);

  useEffect(() => {
    if (!settingsOpen || gpuInfo) return;
    void getGpuInfo()
      .then(setGpuInfo)
      .catch(() => setGpuInfo({ names: [], backend: 'CPU' }));
  }, [settingsOpen, gpuInfo]);

  useEffect(() => {
    const path = settings.llamaServerPath;
    if (!settingsOpen || !path) {
      setLlamaDevices(null);
      setInPath(null);
      return;
    }
    let current = true;
    void getLlamaDevices(path)
      .then((devices) => current && setLlamaDevices(devices))
      .catch(() => current && setLlamaDevices(null));
    void llamaPathStatus(path)
      .then((status) => current && setInPath(status === 'present'))
      .catch(() => current && setInPath(null));
    return () => {
      current = false;
    };
  }, [settingsOpen, settings.llamaServerPath]);

  async function addToPath() {
    setPathBusy(true);
    try {
      const result = await addLlamaToPath(settings.llamaServerPath);
      setInPath(true);
      setServerCheck(
        result === 'added'
          ? 'Added to your user PATH. Programs started from now on will see it.'
          : 'Already in your PATH.',
      );
    } catch (reason) {
      setServerCheck(`Cannot edit PATH · ${errorDetail(reason)}`);
    } finally {
      setPathBusy(false);
    }
  }

  async function installLlamaCpp() {
    setServerCheck('');
    setLlamaDownload({ percent: 0, stage: 'Starting…' });
    const unlisten = await listen<LlamaDownloadProgress>(
      'llama-download-progress',
      (event) => setLlamaDownload(event.payload),
    );
    try {
      const installed = await downloadLlamaCpp();
      updateSettings({ llamaServerPath: installed.path });
      if (installed.upToDate) {
        setServerCheck(
          `The latest version is already installed · ${installed.version} (${installed.variant})`,
        );
      } else {
        await verifyLlamaServer(installed.path);
      }
    } catch (reason) {
      setServerCheck(`Download failed · ${errorDetail(reason)}`);
    } finally {
      unlisten();
      setLlamaDownload(null);
    }
  }

  async function chooseLlamaFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== 'string') return;
    try {
      const executable = await locateLlamaServer(selected);
      updateSettings({ llamaServerPath: executable });
      await verifyLlamaServer(executable);
    } catch (reason) {
      setServerCheck(errorDetail(reason));
    }
  }

  async function detectLlamaServer() {
    const found = await findLlamaServer().catch(() => null);
    if (!found) {
      setServerCheck('llama-server.exe was not found in PATH.');
      return;
    }
    updateSettings({ llamaServerPath: found });
    await verifyLlamaServer(found);
  }

  async function chooseDictionaryFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== 'string') return;
    setAdditionalOpen(true);
    setSelectedDictionaryPath(selected);
    localStorage.setItem(dictionaryPathKey, selected);
    const dictionaries = await refreshUserDictionaries(selected);
    const enabled = dictionaries.map((dictionary) => dictionary.id);
    setEnabledDictionaries(enabled);
    localStorage.setItem(enabledDictionariesKey, JSON.stringify(enabled));
  }

  function toggleUserDictionary(id: string) {
    setEnabledDictionaries((current) => {
      const next = current.includes(id)
        ? current.filter((value) => value !== id)
        : [...current, id];
      localStorage.setItem(enabledDictionariesKey, JSON.stringify(next));
      return next;
    });
  }

  async function selectDictionaryWord(value: string, sourceSelection: boolean) {
    const word = value.trim();
    if (!word) return;
    const requestId = ++lexicalRequestId.current;
    setLexicalResults([]);
    setLexicalMessage('Looking up…');
    setLexicalBusy(true);
    setAdditionalOpen(true);
    let entries: LexicalEntry[];
    try {
      entries = await lookupLexicon(
        word,
        undefined,
        enabledDictionaries,
        selectedDictionaryPath,
      );
    } catch {
      if (requestId !== lexicalRequestId.current) return;
      setLexicalBusy(false);
      setLexicalMessage('Local dictionary is unavailable.');
      return;
    }
    if (requestId !== lexicalRequestId.current) return;
    setLexicalBusy(false);
    setLexicalQuery(word);
    setLexicalResults(entries);
    setLexicalMessage(
      entries.length
        ? `${entries.length} dictionary match.`
        : 'No local dictionary match.',
    );
    if (sourceSelection) {
      try {
        const sourceLanguage =
          settings.sourceLanguage === 'auto'
            ? (await detectLanguage(word)).code
            : settings.sourceLanguage;
        const targetLanguage =
          settings.sourceLanguage === 'auto'
            ? targetForDetectedLanguage(
                sourceLanguage,
                settings.primaryLanguage,
                settings.secondaryLanguage,
              )
            : settings.targetLanguage;
        const result = await translateWord({
          model_id: settings.modelId,
          adapter_id: settings.adapterId,
          source_language: sourceLanguage,
          target_language: targetLanguage,
          text: word,
        });
        if (requestId !== lexicalRequestId.current) return;
        setTranslationHighlight(result.text.trim());
      } catch {
        setTranslationHighlight('');
      }
    } else {
      setTranslationHighlight(word);
      try {
        const selectedLanguage = (await detectLanguage(word)).code;
        const sourceLanguage =
          detectedLanguage ??
          (settings.sourceLanguage === 'auto'
            ? settings.primaryLanguage
            : settings.sourceLanguage);
        const result = await translateWord({
          model_id: settings.modelId,
          adapter_id: settings.adapterId,
          source_language: selectedLanguage,
          target_language: sourceLanguage,
          text: word,
        });
        if (requestId !== lexicalRequestId.current) return;
        const candidate = result.text.trim();
        const match = findWordMatch(source, candidate);
        if (match) {
          window.setTimeout(() => {
            sourceInputRef.current?.focus();
            sourceInputRef.current?.setSelectionRange(match.start, match.end);
          }, 0);
        }
      } catch {
        // The right-side selection remains visible when reverse alignment fails.
      }
    }
  }

  function findWordMatch(text: string, candidate: string) {
    const words = [...text.matchAll(/[\p{L}\p{M}]+/gu)];
    const normalizedCandidate = candidate.toLocaleLowerCase();
    const match = words.find((item) => {
      const value = item[0].toLocaleLowerCase();
      return (
        value === normalizedCandidate ||
        (value.length >= 4 &&
          normalizedCandidate.startsWith(value.slice(0, 4))) ||
        (normalizedCandidate.length >= 4 &&
          value.startsWith(normalizedCandidate.slice(0, 4)))
      );
    });
    return match
      ? { start: match.index!, end: match.index! + match[0].length }
      : null;
  }

  function chooseMode(next: 'text' | 'files') {
    setMode(next);
    try {
      localStorage.setItem(modeKey, next);
    } catch {
      // The mode is only a convenience; ignore blocked storage.
    }
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (!event.ctrlKey || event.shiftKey || event.altKey) return;
      if (event.key === '1') chooseMode('text');
      if (event.key === '2') chooseMode('files');
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  return (
    <main className="shell">
      <header className="masthead">
        <div>
          <p className="eyebrow">LOCAL TRANSLATION WORKBENCH</p>
          <h1>LingvoLoc</h1>
          <p className="build-label">
            v{import.meta.env.VITE_APP_VERSION}{' '}
            <span aria-hidden="true">·</span>{' '}
            <a
              href="https://github.com/thejohnd0e/LingvoLoc"
              onClick={(event) => {
                event.preventDefault();
                void openUrl('https://github.com/thejohnd0e/LingvoLoc');
              }}
            >
              GitHub
            </a>
          </p>
        </div>
        <div className="masthead-zoom">
          <TextSizeControls value={textScale} onChange={setTextScale} />
        </div>
        <div className="masthead-side">
          <div className="masthead-icons">
            <div className="mode-switch" role="tablist" aria-label="Mode">
              <button
                type="button"
                role="tab"
                aria-selected={mode === 'text'}
                title="Text translation (Ctrl+1)"
                onClick={() => chooseMode('text')}
              >
                Text
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={mode === 'files'}
                title="File translation (Ctrl+2)"
                onClick={() => chooseMode('files')}
              >
                Files
                {fileProgress !== null && (
                  <span className="mode-badge">{fileProgress}%</span>
                )}
              </button>
            </div>
            <button
              className="icon-button"
              type="button"
              aria-label="Settings"
              title="Settings"
              onClick={() => setSettingsOpen(true)}
            >
              <svg
                viewBox="0 0 24 24"
                width="18"
                height="18"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
            </button>
          </div>
          <div className="masthead-actions">
            <span className="status-pill">
              {status.startsWith('Checking') && <Spinner />}
              {status}
            </span>
          </div>
        </div>
      </header>
      <section
        className="workspace"
        aria-label="Translation workspace"
        hidden={mode !== 'text'}
      >
        <div className="panel">
          <span className="panel-label">
            Source · {settings.sourceLanguage}
          </span>
          <textarea
            ref={sourceInputRef}
            value={source}
            onChange={(event) => setSource(event.target.value)}
            onDoubleClick={(event) => {
              const target = event.currentTarget;
              void selectDictionaryWord(
                target.value.slice(target.selectionStart, target.selectionEnd),
                true,
              );
            }}
            onKeyDown={(event) => {
              if (
                event.key === 'Enter' &&
                !event.shiftKey &&
                !event.nativeEvent.isComposing
              ) {
                event.preventDefault();
                void runTranslation();
              }
            }}
            placeholder="Write something to translate…"
          />
          <button className="quiet" onClick={() => setSource('')}>
            Clear
          </button>
        </div>
        <div className="panel result-panel">
          <span className="panel-label">
            Translation · {settings.targetLanguage}
          </span>
          {translation ? (
            <div
              className="translation-output"
              onDoubleClick={() => {
                const selection = window.getSelection()?.toString() ?? '';
                if (selection) void selectDictionaryWord(selection, false);
              }}
              role="textbox"
              aria-label="Translation result"
              tabIndex={0}
            >
              {highlightMatches(translation, translationHighlight)}
            </div>
          ) : (
            <textarea
              value={translation}
              readOnly
              placeholder="Your local translation will appear here."
            />
          )}
          <button
            className="quiet"
            onClick={() =>
              void writeClipboard(translation).then(
                () => setNotice('Translation copied to clipboard.'),
                () => setNotice('Clipboard access is unavailable.'),
              )
            }
          >
            Copy
          </button>
        </div>
      </section>
      <section
        className="controls"
        aria-label="Translation settings"
        data-mode={mode}
      >
        <label>
          From
          <select
            value={settings.sourceLanguage}
            onChange={(event) => {
              setDetectedLanguage(null);
              updateSettings({ sourceLanguage: event.target.value });
            }}
          >
            <option value="auto">Detect automatically</option>
            <option value="en">English</option>
            <option value="ru">Russian</option>
            <option value="de">German</option>
            <option value="es">Spanish</option>
            <option value="fr">French</option>
            <option value="it">Italian</option>
            <option value="pt">Portuguese</option>
            <option value="pl">Polish</option>
            <option value="uk">Ukrainian</option>
            <option value="zh">Chinese</option>
            <option value="ko">Korean</option>
            <option value="th">Thai</option>
          </select>
        </label>
        <button
          className="swap"
          aria-label="Swap languages"
          onClick={swapLanguages}
        >
          Swap
        </button>
        <label>
          To
          <select
            value={
              settings.sourceLanguage === 'auto'
                ? 'auto'
                : settings.targetLanguage
            }
            disabled={settings.sourceLanguage === 'auto'}
            onChange={(event) =>
              updateSettings({ targetLanguage: event.target.value })
            }
          >
            <option value="auto">Auto</option>
            <option value="en">English</option>
            <option value="ru">Russian</option>
            <option value="de">German</option>
            <option value="es">Spanish</option>
            <option value="fr">French</option>
            <option value="it">Italian</option>
            <option value="pt">Portuguese</option>
            <option value="pl">Polish</option>
            <option value="uk">Ukrainian</option>
            <option value="zh">Chinese</option>
            <option value="ko">Korean</option>
            <option value="th">Thai</option>
          </select>
        </label>
        <div className="main-pair">
          <span className="main-pair-label">Main pair</span>
          <label>
            <select
              aria-label="Main pair source language"
              value={settings.primaryLanguage}
              onChange={(event) =>
                updateSettings({ primaryLanguage: event.target.value })
              }
            >
              <option value="en">English</option>
              <option value="ru">Russian</option>
              <option value="de">German</option>
              <option value="es">Spanish</option>
              <option value="fr">French</option>
              <option value="it">Italian</option>
              <option value="pt">Portuguese</option>
              <option value="pl">Polish</option>
              <option value="uk">Ukrainian</option>
              <option value="zh">Chinese</option>
              <option value="ko">Korean</option>
              <option value="th">Thai</option>
            </select>
          </label>
          <label>
            <select
              aria-label="Main pair target language"
              value={settings.secondaryLanguage}
              onChange={(event) =>
                updateSettings({ secondaryLanguage: event.target.value })
              }
            >
              <option value="en">English</option>
              <option value="ru">Russian</option>
              <option value="de">German</option>
              <option value="es">Spanish</option>
              <option value="fr">French</option>
              <option value="it">Italian</option>
              <option value="pt">Portuguese</option>
              <option value="pl">Polish</option>
              <option value="uk">Ukrainian</option>
              <option value="zh">Chinese</option>
              <option value="ko">Korean</option>
              <option value="th">Thai</option>
            </select>
          </label>
        </div>
        <div className="model-field">
          <span className="model-label">Model</span>
          <div className="model-row">
            <select
              value={settings.modelId}
              onChange={(event) =>
                updateSettings({ modelId: event.target.value })
              }
            >
              <option value="">
                {settings.runtimeMode === 'standalone'
                  ? 'Select a .gguf model'
                  : 'Select from LM Studio'}
              </option>
              {models.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.id}
                </option>
              ))}
            </select>
            <button
              className="refresh"
              type="button"
              aria-label="Refresh models"
              title="Refresh models"
              disabled={refreshing}
              onClick={() => void refreshModels()}
            >
              <span aria-hidden="true">{refreshing ? <Spinner /> : '↻'}</span>
            </button>
          </div>
        </div>
        <button
          className="translate"
          disabled={loading || !settings.modelId}
          onClick={() => void runTranslation()}
        >
          {loading && <Spinner />}
          {loading ? 'Translating…' : 'Translate'} <span>Enter</span>
        </button>
      </section>
      <div className="feedback" role="status" hidden={mode !== 'text'}>
        {loading && <Spinner />}
        {loading
          ? settings.runtimeMode === 'standalone'
            ? 'Translating… the model may still be loading'
            : 'Translating…'
          : error ||
            notice ||
            historyMessage ||
            (timing === null
              ? 'Local runtime · no request yet'
              : `Local runtime · ${timing} · ${settings.modelId}`)}
        {detectedLanguage ? ` · detected ${detectedLanguage}` : ''}
      </div>
      <DocumentsPanel
        hidden={mode !== 'files'}
        onActivity={setFileProgress}
        onFileDrop={() => chooseMode('files')}
        sourceLanguage={settings.sourceLanguage}
        targetLanguage={
          settings.sourceLanguage === 'auto' ? 'auto' : settings.targetLanguage
        }
        modelId={settings.modelId}
      />
      {settingsOpen && (
        <Modal title="Settings" onClose={() => setSettingsOpen(false)}>
          <section
            className="runtime-panel settings-section"
            aria-label="Model runtime"
          >
            <span className="panel-label">MODEL RUNTIME</span>
            <label className="runtime-mode">
              <b>Mode</b>
              <select
                value={settings.runtimeMode}
                onChange={(event) =>
                  changeRuntimeMode(
                    event.target.value as Settings['runtimeMode'],
                  )
                }
              >
                <option value="lmStudio">LM Studio</option>
                <option value="standalone">Standalone (llama.cpp)</option>
              </select>
            </label>
            {settings.runtimeMode === 'standalone' ? (
              <>
                <div className="runtime-row">
                  <button
                    className="translate"
                    type="button"
                    onClick={() => void chooseModelsFolder()}
                  >
                    Choose folder
                  </button>
                  <div>
                    <b>Models folder</b>
                    <code>
                      {settings.modelsDirectory || 'No folder selected'}
                    </code>
                  </div>
                </div>
                <div className="runtime-row">
                  <div className="runtime-label">GPU support</div>
                  <div>
                    <code>
                      {gpuInfo === null
                        ? 'Detecting…'
                        : gpuInfo.names.length > 0
                          ? gpuInfo.names.join(', ')
                          : 'No GPU detected'}
                    </code>
                    {gpuInfo && (
                      <span className="runtime-note">
                        {gpuInfo.backend === 'CPU'
                          ? 'The CPU build will be used.'
                          : `The ${gpuInfo.backend} build will be used.`}
                      </span>
                    )}
                    {settings.llamaServerPath && (
                      <span className="runtime-note">
                        {llamaDevices === null
                          ? ''
                          : llamaDevices.active
                            ? `In use: ${llamaDevices.active.name} (${llamaDevices.active.id})`
                            : 'In use: CPU'}
                      </span>
                    )}
                  </div>
                </div>
                <div className="runtime-row">
                  <button
                    className="translate"
                    type="button"
                    disabled={llamaDownload !== null}
                    onClick={() => void installLlamaCpp()}
                  >
                    {settings.llamaServerPath
                      ? 'Update llama.cpp'
                      : 'Download llama.cpp'}
                  </button>
                  <div>
                    <b>llama.cpp</b>
                    <code>
                      {llamaDownload
                        ? `${llamaDownload.stage} · ${llamaDownload.percent}%`
                        : settings.llamaServerPath ||
                          'Not installed. Press Download to install it automatically.'}
                    </code>
                    {settings.llamaServerPath && !llamaDownload && (
                      <div className="runtime-buttons">
                        <button
                          className="quiet"
                          type="button"
                          disabled={pathBusy || inPath === true}
                          onClick={() => void addToPath()}
                        >
                          {inPath === true ? 'In PATH' : 'Add to PATH'}
                        </button>
                      </div>
                    )}
                  </div>
                </div>
                {serverCheck && (
                  <p className="runtime-check" role="status">
                    {serverCheck === 'Checking…' && <Spinner />}
                    {serverCheck}
                  </p>
                )}
                <details className="runtime-advanced">
                  <summary>Advanced</summary>
                  <div className="runtime-row">
                    <button
                      className="translate"
                      type="button"
                      onClick={() => void chooseLlamaFolder()}
                    >
                      Choose folder
                    </button>
                    <div>
                      <b>Existing llama.cpp folder</b>
                      <code>{settings.llamaServerPath || 'Not selected'}</code>
                      <div className="runtime-buttons">
                        <button
                          className="quiet"
                          type="button"
                          onClick={() => void detectLlamaServer()}
                        >
                          Find in PATH
                        </button>
                        <button
                          className="quiet"
                          type="button"
                          onClick={() =>
                            void verifyLlamaServer(settings.llamaServerPath)
                          }
                        >
                          Check
                        </button>
                      </div>
                    </div>
                  </div>
                </details>
                <div className="runtime-help">
                  <b>How to set up</b>
                  <ol>
                    <li>Press Download llama.cpp.</li>
                    <li>
                      Choose the folder with your .gguf models and pick one in
                      the Model list. The first translation is slower while the
                      model loads.
                    </li>
                  </ol>
                </div>
              </>
            ) : (
              <p className="runtime-note">
                Translation uses the model loaded in LM Studio at{' '}
                <code>{settings.endpoint}</code>.
              </p>
            )}
          </section>
          <section
            className="runtime-panel settings-section"
            aria-label="Dictionaries"
          >
            <span className="panel-label">DICTIONARIES</span>
            <div className="runtime-row">
              <button
                className="translate"
                type="button"
                onClick={() => void chooseDictionaryFolder()}
              >
                Choose folder
              </button>
              <div>
                <b>Dictionaries folder</b>
                <code>{selectedDictionaryPath || 'No folder selected'}</code>
                <div className="runtime-buttons">
                  <button
                    className="quiet"
                    type="button"
                    disabled={dictionariesBusy}
                    onClick={() => {
                      setAdditionalOpen(true);
                      void refreshUserDictionaries(selectedDictionaryPath);
                    }}
                  >
                    Refresh dictionaries
                  </button>
                </div>
              </div>
            </div>
            <p className="runtime-note">
              Choose the folder where you keep dictionary folders. Each
              dictionary needs its .ifo, .idx and .dict or .dict.dz files.
            </p>
            {dictionariesBusy && (
              <p className="runtime-check" role="status">
                <Spinner />
                Scanning dictionaries…
              </p>
            )}
            <div className="dictionary-list">
              {userDictionaries.length ? (
                userDictionaries.map((dictionary) => (
                  <label key={dictionary.id}>
                    <input
                      type="checkbox"
                      checked={enabledDictionaries.includes(dictionary.id)}
                      onChange={() => toggleUserDictionary(dictionary.id)}
                    />
                    <span>
                      {dictionary.name} ·{' '}
                      {dictionary.entry_count.toLocaleString()} entries
                    </span>
                  </label>
                ))
              ) : (
                <span>No dictionaries detected.</span>
              )}
            </div>
          </section>
          <section
            className="runtime-panel settings-section"
            aria-label="Browser extension"
          >
            <span className="panel-label">BROWSER EXTENSION</span>
            <div className="runtime-row">
              <button
                className="translate"
                type="button"
                onClick={() => void copyExtensionToken()}
              >
                Copy token
              </button>
              <div>
                <b>Pairing token</b>
                <p className="runtime-note">
                  Copy the token and paste it into the LingvoLoc Chrome
                  extension to connect it to this app.
                </p>
              </div>
            </div>
          </section>
        </Modal>
      )}
      <details
        className="additional-options"
        hidden={mode !== 'text'}
        open={additionalOpen}
        onToggle={(event) => setAdditionalOpen(event.currentTarget.open)}
      >
        <summary>Additional</summary>
        <section className="lexical" aria-label="Dictionary lookup">
          <div className="lexical-heading">
            <div>
              <span className="panel-label">LOCAL LEXICON</span>
              <h2>Dictionary lookup</h2>
            </div>
            <div className="lexical-search">
              <input
                aria-label="Dictionary word"
                value={lexicalQuery}
                onChange={(event) => setLexicalQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') void lookupWord();
                }}
                placeholder="learn"
              />
              <button className="translate" onClick={() => void lookupWord()}>
                Look up
              </button>
              <button
                className="quiet"
                type="button"
                onClick={() => {
                  lexicalRequestId.current += 1;
                  setLexicalQuery('');
                  setLexicalResults([]);
                  setLexicalMessage('');
                }}
              >
                Clear
              </button>
            </div>
          </div>
          {(lexicalMessage || lexicalBusy) && (
            <p className="lexical-message" role="status">
              {lexicalBusy && <Spinner />}
              {lexicalMessage}
            </p>
          )}
          {lexicalResults.length > 0 && (
            <details className="lexical-results-disclosure" open>
              <summary>Dictionary results ({lexicalResults.length})</summary>
              <div className="lexical-results">
                {lexicalResults.map((entry) => (
                  <article
                    className={
                      entry.part_of_speech === 'User dictionary'
                        ? 'lexical-entry lexical-entry-user'
                        : 'lexical-entry'
                    }
                    key={`${entry.language}-${entry.lemma}-${entry.part_of_speech}-${entry.providers.join('|')}`}
                  >
                    {entry.part_of_speech === 'User dictionary' && (
                      <div className="lexical-source">
                        <span aria-hidden="true">▤</span>
                        {entry.providers.join(', ') || 'User dictionary'}
                      </div>
                    )}
                    <div className="lexical-entry-title">
                      <strong>{entry.lemma}</strong>
                      {entry.part_of_speech !== 'User dictionary' && (
                        <span>{entry.part_of_speech}</span>
                      )}
                    </div>
                    {(entry.translations.length > 0 ||
                      entry.part_of_speech !== 'User dictionary') && (
                      <p className="lexical-translations">
                        {entry.translations.length
                          ? entry.translations.join(' · ')
                          : 'Translation unavailable until a model is selected and running.'}
                      </p>
                    )}
                    {entry.part_of_speech === 'User dictionary' ? (
                      <div
                        className="dictionary-definition-html"
                        dangerouslySetInnerHTML={{
                          __html: sanitizeDictionaryHtml(
                            entry.definitions.join(' '),
                          ),
                        }}
                      />
                    ) : (
                      <p>{entry.definitions.join(' ')}</p>
                    )}
                    {entry.examples.length > 0 && (
                      <div className="lexical-examples">
                        <b>Examples</b>
                        {entry.examples.map((example) => (
                          <q key={example}>{example}</q>
                        ))}
                      </div>
                    )}
                    <div className="lexical-facts">
                      {entry.forms.length > 0 && (
                        <span>
                          <b>Forms</b> {entry.forms.join(', ')}
                        </span>
                      )}
                      {entry.synonyms.length > 0 && (
                        <span>
                          <b>Synonyms</b> {entry.synonyms.join(', ')}
                        </span>
                      )}
                      {entry.antonyms.length > 0 && (
                        <span>
                          <b>Antonyms</b> {entry.antonyms.join(', ')}
                        </span>
                      )}
                      {entry.related_words.length > 0 && (
                        <span>
                          <b>Related</b> {entry.related_words.join(', ')}
                        </span>
                      )}
                      {entry.providers.length > 0 &&
                        entry.part_of_speech !== 'User dictionary' && (
                          <span>
                            <b>Sources</b> {entry.providers.join(', ')}
                          </span>
                        )}
                    </div>
                  </article>
                ))}
              </div>
            </details>
          )}
        </section>
        <details className="history-disclosure">
          <summary>Recent translations</summary>
          <section className="history" aria-label="Translation history">
            <div className="history-heading">
              <h2>Recent translations</h2>
              <div className="history-tools">
                <input
                  aria-label="Search history"
                  value={historyInput}
                  onChange={(event) => setHistoryInput(event.target.value)}
                  placeholder="Search"
                />
                <button className="quiet" onClick={() => void searchHistory()}>
                  Search
                </button>
                <button
                  className="quiet"
                  onClick={() => void removeHistory()}
                  disabled={!history.length}
                >
                  Clear history
                </button>
                <button
                  className="quiet"
                  onClick={() => void exportTranslationHistory()}
                >
                  Export CSV
                </button>
              </div>
            </div>
            {history.length === 0 ? (
              <p className="history-empty">
                Successful translations will be saved locally.
              </p>
            ) : (
              <div className="history-list">
                {history.map((entry) => (
                  <article className="history-entry" key={entry.id}>
                    <span>
                      {entry.source_language} → {entry.target_language}
                    </span>
                    <strong>
                      {highlightMatches(entry.source_text, historyQuery)}
                    </strong>
                    <p>
                      {highlightMatches(entry.translated_text, historyQuery)}
                    </p>
                    <button
                      className="quiet history-favorite"
                      onClick={() => void toggleFavorite(entry)}
                    >
                      {entry.favorite ? 'Unfavorite' : 'Favorite'}
                    </button>
                  </article>
                ))}
              </div>
            )}
            {hasMoreHistory && (
              <button
                className="quiet history-more"
                onClick={() => void loadMoreHistory()}
              >
                Load more
              </button>
            )}
          </section>
        </details>
      </details>
    </main>
  );
}
