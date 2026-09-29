import { open, save } from '@tauri-apps/plugin-dialog';
import { openPath } from '@tauri-apps/plugin-opener';
import { useEffect, useState } from 'react';
import {
  analyzeDocx,
  analyzeEpub,
  analyzeTxt,
  cancelDocumentJob,
  exportDocxJob,
  exportEpubJob,
  exportTxtJob,
  getDocumentJob,
  pauseDocumentJob,
  resumeDocxJob,
  resumeEpubJob,
  resumeTxtJob,
  startDocxJob,
  startEpubJob,
  startTxtJob,
  type DocumentJobView,
} from '../lib/commands';
import { errorDetail } from '../lib/errors';

const storedJobKey = 'lingvoloc.document-job-id';

type DocumentFormat = 'txt' | 'docx' | 'epub';

const documentFormats: Record<
  DocumentFormat,
  {
    label: string;
    extension: string;
    analyze: (
      sourcePath: string,
      sourceLanguage: string,
      targetLanguage: string,
    ) => Promise<DocumentJobView>;
    start: (jobId: string) => Promise<DocumentJobView>;
    resume: (jobId: string) => Promise<DocumentJobView>;
    export: (
      jobId: string,
      outputPath?: string,
    ) => ReturnType<typeof exportTxtJob>;
  }
> = {
  txt: {
    label: 'TXT',
    extension: 'txt',
    analyze: analyzeTxt,
    start: startTxtJob,
    resume: resumeTxtJob,
    export: exportTxtJob,
  },
  docx: {
    label: 'DOCX',
    extension: 'docx',
    analyze: analyzeDocx,
    start: startDocxJob,
    resume: resumeDocxJob,
    export: exportDocxJob,
  },
  epub: {
    label: 'EPUB',
    extension: 'epub',
    analyze: analyzeEpub,
    start: startEpubJob,
    resume: resumeEpubJob,
    export: exportEpubJob,
  },
};

function documentFormat(value: string): DocumentFormat {
  const format = value.toLowerCase();
  return format === 'docx' || format === 'epub' ? format : 'txt';
}

interface DocumentsPanelProps {
  sourceLanguage: string;
  targetLanguage: string;
  modelId: string;
}

export default function DocumentsPanel({
  sourceLanguage,
  targetLanguage,
  modelId,
}: DocumentsPanelProps) {
  const [job, setJob] = useState<DocumentJobView | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [outputPath, setOutputPath] = useState('');

  useEffect(() => {
    const jobId = localStorage.getItem(storedJobKey);
    if (!jobId) return;
    void getDocumentJob(jobId)
      .then(setJob)
      .catch(() => localStorage.removeItem(storedJobKey));
  }, []);

  async function chooseAndAnalyze() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Documents', extensions: ['txt', 'docx', 'epub'] }],
    });
    if (typeof selected !== 'string') return;
    setBusy(true);
    const format =
      documentFormats[documentFormat(selected.split('.').pop() ?? '')];
    setMessage(`Analyzing ${format.label}…`);
    try {
      const next = await format.analyze(
        selected,
        sourceLanguage,
        targetLanguage,
      );
      localStorage.setItem(storedJobKey, next.job.id);
      setJob(next);
      setMessage(
        `${next.total_blocks} paragraph${next.total_blocks === 1 ? '' : 's'} ready.`,
      );
    } catch (reason) {
      setMessage(`Analysis failed · ${errorDetail(reason)}`);
    } finally {
      setBusy(false);
    }
  }

  async function updateJob(
    action: () => Promise<DocumentJobView>,
    label: string,
  ) {
    if (!job) return;
    setBusy(true);
    setMessage(label);
    try {
      setJob(await action());
      setMessage('');
    } catch (reason) {
      setMessage(`${label} failed · ${errorDetail(reason)}`);
      try {
        setJob(await getDocumentJob(job.job.id));
      } catch {
        // Keep the last known snapshot when the native call failed too.
      }
    } finally {
      setBusy(false);
    }
  }

  async function exportJob() {
    if (!job) return;
    const format = documentFormats[documentFormat(job.job.format)];
    const selected = await save({
      defaultPath: `${job.job.source_path.replace(/\.[^.\\/]+$/, '')}.translated.${job.job.target_language}.${format.extension}`,
      filters: [{ name: 'Documents', extensions: [format.extension] }],
    });
    if (!selected) return;
    setBusy(true);
    setMessage(`Exporting ${format.label}…`);
    try {
      const result = await format.export(job.job.id, selected);
      setJob(result.job);
      setOutputPath(result.output_path);
      setMessage(`Exported to ${result.output_path}`);
    } catch (reason) {
      setMessage(`Export failed · ${errorDetail(reason)}`);
    } finally {
      setBusy(false);
    }
  }

  const state = job?.job.state;
  const canStart = state === 'ready' && Boolean(modelId);
  const canResume = state === 'paused' || state === 'interrupted';
  const canPause = state === 'translating';
  const canCancel =
    state === 'ready' || state === 'translating' || state === 'paused';
  const canExport =
    state === 'translating' &&
    job !== null &&
    job.total_blocks > 0 &&
    job.translated_blocks === job.total_blocks;

  return (
    <section className="documents-panel" aria-label="Documents">
      <div className="documents-heading">
        <div>
          <span className="panel-label">DOCUMENTS / TXT + DOCX + EPUB</span>
          <h2>Translate a document</h2>
          <p>
            TXT, DOCX, and EPUB are supported. The original file is never
            replaced.
          </p>
        </div>
        <button
          className="quiet"
          type="button"
          onClick={() => void chooseAndAnalyze()}
          disabled={busy}
        >
          Choose .txt, .docx, or .epub
        </button>
      </div>
      {job && (
        <div className="document-job" aria-live="polite">
          <div className="document-job-meta">
            <strong>{job.job.source_path.split(/[\\/]/).pop()}</strong>
            <span>
              {job.job.source_language} → {job.job.target_language}
            </span>
            <span>Model: {modelId || 'not selected'}</span>
            <span>Status: {job.job.state}</span>
          </div>
          <progress max={job.total_blocks || 1} value={job.translated_blocks} />
          <span className="document-progress">
            {job.translated_blocks} / {job.total_blocks} blocks translated
          </span>
          {job.diagnostics.length > 0 && (
            <ul aria-label="Document diagnostics">
              {job.diagnostics.map((diagnostic) => (
                <li key={diagnostic}>{diagnostic}</li>
              ))}
            </ul>
          )}
          <div className="document-actions">
            {canStart && (
              <button
                className="translate"
                type="button"
                disabled={busy}
                onClick={() =>
                  void updateJob(
                    () =>
                      documentFormats[documentFormat(job.job.format)].start(
                        job.job.id,
                      ),
                    'Translating…',
                  )
                }
              >
                Start translation
              </button>
            )}
            {canResume && (
              <button
                className="translate"
                type="button"
                disabled={busy}
                onClick={() =>
                  void updateJob(
                    () =>
                      documentFormats[documentFormat(job.job.format)].resume(
                        job.job.id,
                      ),
                    'Resuming…',
                  )
                }
              >
                Resume
              </button>
            )}
            {canPause && (
              <button
                className="quiet"
                type="button"
                disabled={busy}
                onClick={() =>
                  void updateJob(() => pauseDocumentJob(job.job.id), 'Pausing…')
                }
              >
                Pause after current block
              </button>
            )}
            {canCancel && (
              <button
                className="quiet danger"
                type="button"
                disabled={busy}
                onClick={() =>
                  void updateJob(
                    () => cancelDocumentJob(job.job.id),
                    'Cancelling…',
                  )
                }
              >
                Cancel
              </button>
            )}
            {canExport && (
              <button
                className="quiet"
                type="button"
                disabled={busy}
                onClick={() => void exportJob()}
              >
                Export translated {job.job.format.toUpperCase()}
              </button>
            )}
            {outputPath && (
              <>
                <button
                  className="quiet"
                  type="button"
                  onClick={() => void openPath(outputPath)}
                >
                  Open output
                </button>
                <button
                  className="quiet"
                  type="button"
                  onClick={() =>
                    void openPath(outputPath.replace(/[\\/][^\\/]+$/, ''))
                  }
                >
                  Open folder
                </button>
              </>
            )}
            {(state === 'completed' || state === 'cancelled') && (
              <button
                className="quiet"
                type="button"
                onClick={() => {
                  localStorage.removeItem(storedJobKey);
                  setJob(null);
                }}
              >
                Clear job
              </button>
            )}
          </div>
        </div>
      )}
      {message && (
        <p className="document-message" role="status">
          {message}
        </p>
      )}
    </section>
  );
}
