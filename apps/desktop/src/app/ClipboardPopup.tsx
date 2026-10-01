import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import {
  detectLanguage,
  getNativeSettings,
  takeClipboardRequest,
  translate,
  writeClipboard,
  formatTiming,
  type TranslationResult,
} from '../lib/commands';
import {
  loadClipboardTextScale,
  loadSettings,
  saveClipboardTextScale,
  type Settings,
} from '../lib/settings';
import { errorDetail } from '../lib/errors';
import { targetForDetectedLanguage } from '../lib/languagePair';
import TextSizeControls from './TextSizeControls';

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
  translationStyle: 'neutral',
};

export default function ClipboardPopup() {
  const [textScale, setTextScale] = useState(loadClipboardTextScale);
  const [source, setSource] = useState('Waiting for clipboard…');
  const [result, setResult] = useState<TranslationResult | null>(null);
  const [status, setStatus] = useState('Ready');

  useEffect(() => {
    let active = true;
    const pollClipboardRequest = async () => {
      try {
        const text = await takeClipboardRequest();
        if (!active || !text) return;
        if (!text.trim()) {
          setStatus('Clipboard is empty.');
          return;
        }
        setSource(text);
        setResult(null);
        setStatus('Translating…');
        // The popup is created at startup, before the main window has restored or chosen
        // a model, so settings are read fresh for every request.
        let settings = loadSettings(defaultSettings);
        if (!settings.modelId) {
          settings = {
            ...settings,
            modelId: (await getNativeSettings()).modelId,
          };
        }
        const sourceLanguage =
          settings.sourceLanguage === 'auto'
            ? (await detectLanguage(text)).code
            : settings.sourceLanguage;
        const targetLanguage =
          settings.sourceLanguage === 'auto'
            ? targetForDetectedLanguage(
                sourceLanguage,
                settings.primaryLanguage,
                settings.secondaryLanguage,
              )
            : settings.targetLanguage;
        const translated = await translate({
          model_id: settings.modelId,
          adapter_id: settings.adapterId,
          source_language: sourceLanguage,
          target_language: targetLanguage,
          text,
          translation_style: settings.translationStyle,
        });
        setResult(translated);
        await writeClipboard(translated.text);
        setStatus(`Copied · ${formatTiming(translated)}`);
      } catch (reason) {
        setStatus(`Translation failed · ${errorDetail(reason)}`);
      }
    };
    void pollClipboardRequest();
    const timer = window.setInterval(() => void pollClipboardRequest(), 250);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    const resizeToContent = async () => {
      try {
        const currentWindow = getCurrentWindow();
        const [scaleFactor, currentSize] = await Promise.all([
          currentWindow.scaleFactor(),
          currentWindow.innerSize(),
        ]);
        const contentHeight =
          Math.max(
            document.documentElement.scrollHeight,
            document.body.scrollHeight,
          ) + 24;
        const height = Math.min(
          Math.max(240, contentHeight),
          Math.max(window.screen.availHeight - 40, 480),
        );
        await currentWindow.setSize(
          new LogicalSize(currentSize.width / scaleFactor, height),
        );
      } catch {
        // Browser preview does not expose a native window.
      }
    };
    const timer = window.setTimeout(() => void resizeToContent(), 0);
    return () => window.clearTimeout(timer);
  }, [source, result, status, textScale]);

  useEffect(() => {
    document.documentElement.style.setProperty(
      '--text-scale',
      String(textScale),
    );
  }, [textScale]);

  return (
    <main className="clipboard-popup">
      <div className="clipboard-popup-heading">
        <span className="eyebrow">Clipboard translation</span>
        <TextSizeControls
          value={textScale}
          onChange={setTextScale}
          onSave={saveClipboardTextScale}
        />
        <span className="status-pill">{status}</span>
      </div>
      <p className="clipboard-popup-source">{source}</p>
      <div className="clipboard-popup-result">
        {result?.text ?? 'Copy text, then press Ctrl+Shift+T.'}
      </div>
    </main>
  );
}
