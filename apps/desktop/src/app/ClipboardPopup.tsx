import { useEffect, useState } from 'react';
import {
  detectLanguage,
  takeClipboardRequest,
  translate,
  writeClipboard,
  type TranslationResult,
} from '../lib/commands';
import { loadSettings, loadTextScale, type Settings } from '../lib/settings';
import { targetForDetectedLanguage } from '../lib/languagePair';
import TextSizeControls from './TextSizeControls';

const defaultSettings: Settings = {
  endpoint: 'http://127.0.0.1:1234/v1',
  modelId: '',
  adapterId: 'translategemma',
  sourceLanguage: 'auto',
  targetLanguage: 'en',
  primaryLanguage: 'en',
  secondaryLanguage: 'ru',
};

export default function ClipboardPopup() {
  const [settings] = useState(() => loadSettings(defaultSettings));
  const [textScale, setTextScale] = useState(loadTextScale);
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
        });
        setResult(translated);
        await writeClipboard(translated.text);
        setStatus(`Copied · ${translated.latency_ms} ms`);
      } catch {
        setStatus('Translation failed. Check LM Studio and clipboard access.');
      }
    };
    void pollClipboardRequest();
    const timer = window.setInterval(() => void pollClipboardRequest(), 250);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [settings]);

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
        <TextSizeControls value={textScale} onChange={setTextScale} />
        <span className="status-pill">{status}</span>
      </div>
      <p className="clipboard-popup-source">{source}</p>
      <div className="clipboard-popup-result">
        {result?.text ?? 'Copy text, then press Ctrl+Shift+T.'}
      </div>
    </main>
  );
}
