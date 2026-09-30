import { open, save } from '@tauri-apps/plugin-dialog';
import { openPath } from '@tauri-apps/plugin-opener';
import { useEffect, useRef, useState } from 'react';
import {
  analyzeDocx,
  analyzeEpub,
  analyzeFb2,
  analyzeTxt,
  cancelDocumentJob,
  clearDocumentJob,
  exportDocxJob,
  exportEpubJob,
  exportFb2Job,
  exportTxtJob,
  getDocumentJob,
  getDocumentProgress,
  listDocumentJobs,
  pauseDocumentJob,
  resumeDocxJob,
  resumeEpubJob,
  resumeFb2Job,
  resumeTxtJob,
  startDocxJob,
  startEpubJob,
  startFb2Job,
  startTxtJob,
  type DocumentJobSummary,
  type DocumentJobView,
} from '../lib/commands';
import { errorDetail } from '../lib/errors';
import {
  formatDuration,
  formatEta,
  groupDiagnostics,
} from './documentProgress';

const storedJobKey = 'lingvoloc.document-job-id';
const supportedExtensions = ['txt', 'docx', 'epub', 'fb2'];

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function summaryLabel(summary: DocumentJobSummary): string {
  const { state } = summary.job;
  if (state === 'translating') {
    return summary.total_blocks > 0 &&
      summary.translated_blocks === summary.total_blocks
      ? 'translated'
      : 'translating';
  }
  return state.replace(/_/g, ' ');
}

type DocumentFormat = 'txt' | 'docx' | 'epub' | 'fb2';

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
  fb2: {
    label: 'FB2',
    extension: 'fb2',
    analyze: analyzeFb2,
    start: startFb2Job,
    resume: resumeFb2Job,
    export: exportFb2Job,
  },
};

function documentFormat(value: string): DocumentFormat {
  const format = value.toLowerCase();
  return format === 'docx' || format === 'epub' || format === 'fb2'
    ? format
    : 'txt';
}

interface DocumentsPanelProps {
  sourceLanguage: string;
  targetLanguage: string;
  modelId: string;
  hidden?: boolean;
  /** Percent while a job is translating, otherwise null. */
  onActivity?: (percent: number | null) => void;
  /** Called when a file is dropped on the window so the host can show this panel. */
  onFileDrop?: () => void;
}

const generatedTranslation =
  /\.translated\.[a-z]{2,3}(-[a-z0-9]+)?\.(txt|docx|epub|fb2)$/i;

export default function DocumentsPanel({
  sourceLanguage,
  targetLanguage,
  modelId,
  hidden = false,
  onActivity,
  onFileDrop,
}: DocumentsPanelProps) {
  const [job, setJob] = useState<DocumentJobView | null>(null);
  const [busy, setBusy] = useState(false);
  const [controlBusy, setControlBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [outputPath, setOutputPath] = useState('');
  const [progressStartedAt, setProgressStartedAt] = useState<number | null>(
    null,
  );
  const [etaNow, setEtaNow] = useState(() => Date.now());
  const [finishedIn, setFinishedIn] = useState<string | null>(null);
  // Identifies the latest long-running action so a superseded one (its job was
  // cleared while the native call is still winding down) cannot leave the panel busy.
  const actionRun = useRef({ main: 0, control: 0 });
  const clearedJobIds = useRef(new Set<string>());
  const [recent, setRecent] = useState<DocumentJobSummary[]>([]);
  const [queueRunning, setQueueRunning] = useState(false);
  const [queueMessage, setQueueMessage] = useState('');
  const [dragging, setDragging] = useState(false);
  const dropHandler = useRef<(paths: string[]) => void>(() => undefined);

  async function refreshRecent() {
    try {
      setRecent(await listDocumentJobs());
    } catch {
      // The list is a convenience; keep the previous rows when it cannot load.
    }
  }

  useEffect(() => {
    void refreshRecent();
  }, []);

  useEffect(() => {
    const jobId = localStorage.getItem(storedJobKey);
    if (!jobId) return;
    void getDocumentJob(jobId)
      .then((snapshot) => {
        if (!clearedJobIds.current.has(jobId)) setJob(snapshot);
      })
      .catch(() => localStorage.removeItem(storedJobKey));
  }, []);

  useEffect(() => {
    if (job?.job.state !== 'translating') {
      setProgressStartedAt(null);
      return;
    }
    if (progressStartedAt === null) {
      setProgressStartedAt(Date.now());
    }
    const timer = window.setInterval(() => setEtaNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [job?.job.state, progressStartedAt]);

  async function chooseAndAnalyze() {
    const selected = await open({
      multiple: true,
      filters: [{ name: 'Documents', extensions: supportedExtensions }],
    });
    if (selected === null) return;
    await analyzePaths(Array.isArray(selected) ? selected : [selected]);
  }

  /** Analyzes each file into its own job; the first new job becomes the active one. */
  async function analyzePaths(paths: string[]) {
    const supported = paths.filter((path) =>
      supportedExtensions.includes((path.split('.').pop() ?? '').toLowerCase()),
    );
    if (supported.length === 0) {
      setMessage('Unsupported file. Choose .txt, .docx, .epub, or .fb2.');
      return;
    }
    setBusy(true);
    const failures: string[] = [];
    let created = 0;
    let first: DocumentJobView | null = null;
    let lastNote = '';
    for (const [index, path] of supported.entries()) {
      const format =
        documentFormats[documentFormat(path.split('.').pop() ?? '')];
      setMessage(
        `Analyzing ${format.label} ${index + 1} / ${supported.length}: ${fileName(path)}…`,
      );
      try {
        const next = await format.analyze(path, sourceLanguage, targetLanguage);
        clearedJobIds.current.delete(next.job.id);
        created += 1;
        first ??= next;
        lastNote = generatedTranslation.test(path)
          ? ' Note: the file name looks like a LingvoLoc translation; make sure this is the file you want.'
          : '';
      } catch (reason) {
        failures.push(`${fileName(path)}: ${errorDetail(reason)}`);
      }
    }
    if (first) {
      localStorage.setItem(storedJobKey, first.job.id);
      setJob(first);
      setOutputPath('');
    }
    setMessage(
      [
        created === 1 && first
          ? `${first.total_blocks} paragraph${first.total_blocks === 1 ? '' : 's'} ready.${lastNote}`
          : created > 1
            ? `${created} files ready. Start one, or translate them all in a row.`
            : '',
        failures.length > 0 ? `Analysis failed · ${failures.join('; ')}` : '',
      ]
        .filter(Boolean)
        .join(' '),
    );
    setBusy(false);
    void refreshRecent();
  }

  dropHandler.current = (paths) => {
    onFileDrop?.();
    if (busy || queueRunning) {
      setMessage(
        'A translation is running. Drop the files again when it ends.',
      );
      return;
    }
    void analyzePaths(paths);
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let stopped = false;
    void import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          const { type } = event.payload;
          if (type === 'enter' || type === 'over') setDragging(true);
          if (type === 'leave') setDragging(false);
          if (type === 'drop') {
            setDragging(false);
            dropHandler.current(event.payload.paths);
          }
        }),
      )
      .then((remove) => {
        if (stopped) remove();
        else unlisten = remove;
      })
      .catch(() => undefined);
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!busy && !queueRunning) return;
    const timer = window.setInterval(() => void refreshRecent(), 3000);
    return () => window.clearInterval(timer);
  }, [busy, queueRunning]);

  async function openJob(jobId: string) {
    try {
      const snapshot = await getDocumentJob(jobId);
      clearedJobIds.current.delete(jobId);
      localStorage.setItem(storedJobKey, jobId);
      setJob(snapshot);
      setOutputPath('');
      setMessage('');
    } catch (reason) {
      setMessage(`Open failed · ${errorDetail(reason)}`);
      void refreshRecent();
    }
  }

  async function removeJob(jobId: string) {
    if (job?.job.id === jobId) {
      await clearJob();
    } else {
      try {
        await clearDocumentJob(jobId);
      } catch (reason) {
        setMessage(`Clear failed · ${errorDetail(reason)}`);
      }
    }
    void refreshRecent();
  }

  async function clearFinished() {
    const finished = recent.filter((item) =>
      ['completed', 'completed_with_warnings', 'cancelled'].includes(
        item.job.state,
      ),
    );
    for (const item of finished) {
      await removeJob(item.job.id);
    }
  }

  /** Translates every ready job in the order it was added and exports each next to its source. */
  async function runQueue() {
    const queued = recent
      .filter((item) => item.job.state === 'ready')
      .reverse();
    if (queued.length === 0) return;
    setQueueRunning(true);
    let finished = 0;
    for (const [index, item] of queued.entries()) {
      setQueueMessage(
        `Queue ${index + 1} / ${queued.length}: ${fileName(item.job.source_path)}`,
      );
      let view: DocumentJobView;
      try {
        view = await getDocumentJob(item.job.id);
      } catch {
        continue;
      }
      clearedJobIds.current.delete(view.job.id);
      localStorage.setItem(storedJobKey, view.job.id);
      setJob(view);
      setOutputPath('');
      const format = documentFormats[documentFormat(view.job.format)];
      const snapshot = await updateJob(
        () => format.start(view.job.id),
        'Translating…',
        false,
        view,
      );
      if (
        !snapshot ||
        snapshot.job.state !== 'translating' ||
        snapshot.translated_blocks !== snapshot.total_blocks
      ) {
        break;
      }
      try {
        const result = await format.export(view.job.id);
        setJob(result.job);
        setOutputPath(result.output_path);
        setMessage(`Exported to ${result.output_path}`);
        finished += 1;
      } catch (reason) {
        setMessage(`Export failed · ${errorDetail(reason)}`);
        break;
      }
    }
    setQueueRunning(false);
    setQueueMessage(
      finished > 0
        ? `Queue finished: ${finished} of ${queued.length} exported.`
        : '',
    );
    void refreshRecent();
  }

  async function updateJob(
    action: () => Promise<DocumentJobView>,
    label: string,
    isControlAction = false,
    target: DocumentJobView | null = job,
  ): Promise<DocumentJobView | null> {
    if (!target) return null;
    const jobId = target.job.id;
    const kind = isControlAction ? 'control' : 'main';
    const run = ++actionRun.current[kind];
    if (isControlAction) {
      setControlBusy(true);
    } else {
      setBusy(true);
      setProgressStartedAt(Date.now());
      setEtaNow(Date.now());
    }
    setMessage(label);
    let polling = false;
    let pollInFlight = false;
    const progressTimer = window.setInterval(() => {
      if (pollInFlight) return;
      pollInFlight = true;
      void getDocumentProgress(jobId)
        .then((progress) => {
          if (polling && !clearedJobIds.current.has(jobId)) {
            // Blocks and diagnostics do not change while translating, so only
            // the counters and state are refreshed.
            setJob((current) =>
              current && current.job.id === jobId
                ? {
                    ...current,
                    job: progress.job,
                    translated_blocks: progress.translated_blocks,
                    total_blocks: progress.total_blocks,
                    paused: progress.job.state === 'paused',
                    cancelled: progress.job.state === 'cancelled',
                  }
                : current,
            );
          }
        })
        .catch(() => undefined)
        .finally(() => {
          pollInFlight = false;
        });
    }, 750);
    polling = true;
    let result: DocumentJobView | null = null;
    try {
      const snapshot = await action();
      if (!clearedJobIds.current.has(jobId)) {
        setJob(snapshot);
        setMessage('');
        result = snapshot;
      }
    } catch (reason) {
      if (clearedJobIds.current.has(jobId)) return null;
      setMessage(`${label} failed · ${errorDetail(reason)}`);
      try {
        const snapshot = await getDocumentJob(jobId);
        setJob(snapshot);
      } catch {
        // Keep the last known snapshot when the native call failed too.
      }
    } finally {
      polling = false;
      window.clearInterval(progressTimer);
      if (actionRun.current[kind] === run) {
        if (isControlAction) {
          setControlBusy(false);
        } else {
          setBusy(false);
        }
      }
      void refreshRecent();
    }
    return result;
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

  async function openOutput(path: string, label: string) {
    try {
      await openPath(path);
    } catch (reason) {
      setMessage(`${label} failed · ${errorDetail(reason)}`);
    }
  }

  async function clearJob() {
    if (!job) return;
    const jobId = job.job.id;
    clearedJobIds.current.add(jobId);
    actionRun.current.main += 1;
    actionRun.current.control += 1;
    setBusy(false);
    setControlBusy(false);
    localStorage.removeItem(storedJobKey);
    setJob(null);
    setOutputPath('');
    setMessage('Clearing…');
    try {
      await clearDocumentJob(jobId);
      setMessage('');
    } catch (reason) {
      clearedJobIds.current.delete(jobId);
      localStorage.setItem(storedJobKey, jobId);
      setJob(job);
      setMessage(`Clear failed · ${errorDetail(reason)}`);
    }
  }

  const state = job?.job.state;
  const canStart = state === 'ready' && Boolean(modelId);
  const canResume =
    state === 'paused' || state === 'interrupted' || state === 'failed';
  const translationFinished =
    job !== null &&
    job.total_blocks > 0 &&
    job.translated_blocks === job.total_blocks;
  const canCancel =
    state === 'ready' ||
    state === 'paused' ||
    (state === 'translating' && !translationFinished);
  const canPause = state === 'translating' && !translationFinished;
  const canExport = state === 'translating' && translationFinished;
  const translationDone =
    canExport ||
    state === 'completed' ||
    state === 'completed_with_warnings' ||
    state === 'exporting';
  useEffect(() => {
    setFinishedIn(null);
  }, [job?.job.id]);
  useEffect(() => {
    if (state === 'translating' && translationFinished) {
      setFinishedIn(
        (current) =>
          current ??
          (progressStartedAt === null
            ? null
            : formatDuration(Date.now() - progressStartedAt)),
      );
    }
  }, [state, translationFinished, progressStartedAt]);
  const percent =
    state === 'translating' && job && !translationFinished
      ? Math.floor(
          (job.translated_blocks * 100) / Math.max(job.total_blocks, 1),
        )
      : null;
  useEffect(() => {
    onActivity?.(percent);
  }, [percent, onActivity]);
  const readyCount = recent.filter((item) => item.job.state === 'ready').length;
  const finishedCount = recent.filter((item) =>
    ['completed', 'completed_with_warnings', 'cancelled'].includes(
      item.job.state,
    ),
  ).length;
  const groupedDiagnostics = groupDiagnostics(job?.diagnostics ?? []);
  const visibleDiagnostics = groupedDiagnostics.slice(0, 8);
  const hiddenDiagnostics = groupedDiagnostics.slice(8);
  const eta =
    job && progressStartedAt !== null
      ? formatEta(
          job.translated_blocks,
          job.total_blocks,
          progressStartedAt,
          etaNow,
        )
      : null;

  return (
    <section
      className={`documents-panel${dragging ? ' drag-over' : ''}`}
      aria-label="Documents"
      hidden={hidden}
    >
      <div className="documents-heading">
        <div>
          <span className="panel-label">
            DOCUMENTS / TXT + DOCX + EPUB + FB2
          </span>
          <h2>Translate a document</h2>
          <p>
            TXT, DOCX, EPUB, and FB2 are supported. Drop files anywhere on the
            window or choose several at once. The original file is never
            replaced.
          </p>
        </div>
        <button
          className="quiet"
          type="button"
          onClick={() => void chooseAndAnalyze()}
          disabled={busy || queueRunning}
        >
          Choose .txt, .docx, .epub, or .fb2
        </button>
      </div>
      {job && (
        <div className="document-job" aria-live="polite">
          <div className="document-job-meta">
            <strong title={job.job.source_path.split(/[\\/]/).pop()}>
              {job.job.source_path.split(/[\\/]/).pop()}
            </strong>
            <span>
              {job.job.source_language} → {job.job.target_language}
            </span>
            <span>Model: {modelId || 'not selected'}</span>
            <span>
              Status:{' '}
              {state === 'translating' && translationFinished
                ? 'translated, ready to export'
                : job.job.state}
            </span>
          </div>
          <progress max={job.total_blocks || 1} value={job.translated_blocks} />
          <span className="document-progress">
            {translationDone
              ? `✓ Translated ${job.total_blocks} / ${job.total_blocks} blocks${finishedIn ? ` in ${finishedIn}` : ''}${canExport ? ' · export to save the file' : ''}`
              : `${job.translated_blocks} / ${job.total_blocks} blocks translated${eta ? ` · ETA ${eta}` : ''}`}
          </span>
          {job.job.error && (
            <p className="document-error" role="alert">
              {job.job.error}
            </p>
          )}
          {job.diagnostics.length > 0 && (
            <>
              <ul aria-label="Document diagnostics">
                {visibleDiagnostics.map((diagnostic, index) => (
                  <li key={`${diagnostic}-${index}`}>{diagnostic}</li>
                ))}
              </ul>
              {hiddenDiagnostics.length > 0 && (
                <details className="document-diagnostics-more">
                  <summary>
                    Show {hiddenDiagnostics.length} more diagnostics
                  </summary>
                  <ul>
                    {hiddenDiagnostics.map((diagnostic, index) => (
                      <li key={`${diagnostic}-${index + 8}`}>{diagnostic}</li>
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
          <div className="document-actions">
            {canStart && (
              <button
                className="translate"
                type="button"
                disabled={busy || controlBusy}
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
                disabled={busy || controlBusy}
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
                disabled={controlBusy}
                onClick={() =>
                  void updateJob(
                    () => pauseDocumentJob(job.job.id),
                    'Pausing…',
                    true,
                  )
                }
              >
                Pause after current block
              </button>
            )}
            {canCancel && (
              <button
                className="quiet danger"
                type="button"
                disabled={controlBusy}
                onClick={() =>
                  void updateJob(
                    () => cancelDocumentJob(job.job.id),
                    'Cancelling…',
                    true,
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
                disabled={busy || controlBusy}
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
                  onClick={() => void openOutput(outputPath, 'Open output')}
                >
                  Open output
                </button>
                <button
                  className="quiet"
                  type="button"
                  onClick={() =>
                    void openOutput(
                      outputPath.replace(/[\\/][^\\/]+$/, ''),
                      'Open folder',
                    )
                  }
                >
                  Open folder
                </button>
              </>
            )}
            <button
              className="quiet"
              type="button"
              onClick={() => void clearJob()}
            >
              Clear job
            </button>
          </div>
        </div>
      )}
      {message && (
        <p className="document-message" role="status">
          {message}
        </p>
      )}
      {queueMessage && (
        <p className="document-message" role="status">
          {queueMessage}
        </p>
      )}
      {recent.length > 0 && (
        <div className="document-recent">
          <div className="document-recent-head">
            <span className="panel-label">RECENT JOBS</span>
            <span className="document-recent-actions">
              {readyCount >= 2 && (
                <button
                  className="translate"
                  type="button"
                  disabled={busy || controlBusy || queueRunning || !modelId}
                  onClick={() => void runQueue()}
                >
                  Translate {readyCount} ready files in a row
                </button>
              )}
              {finishedCount > 0 && (
                <button
                  className="quiet"
                  type="button"
                  disabled={busy || queueRunning}
                  onClick={() => void clearFinished()}
                >
                  Clear finished
                </button>
              )}
            </span>
          </div>
          <ul aria-label="Recent document jobs">
            {recent.map((item) => {
              const selected = item.job.id === job?.job.id;
              return (
                <li key={item.job.id} className={selected ? 'active' : ''}>
                  <span className="recent-name" title={item.job.source_path}>
                    {fileName(item.job.source_path)}
                  </span>
                  <span>
                    {item.job.source_language} → {item.job.target_language}
                  </span>
                  <span className="recent-state">{summaryLabel(item)}</span>
                  <span className="recent-count">
                    {item.translated_blocks} / {item.total_blocks}
                  </span>
                  {!selected && (
                    <button
                      className="quiet"
                      type="button"
                      disabled={busy || controlBusy || queueRunning}
                      onClick={() => void openJob(item.job.id)}
                    >
                      Open
                    </button>
                  )}
                  <button
                    className="quiet"
                    type="button"
                    disabled={queueRunning && selected}
                    onClick={() => void removeJob(item.job.id)}
                  >
                    Remove
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </section>
  );
}
