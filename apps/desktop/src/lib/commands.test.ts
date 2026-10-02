import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

import {
  analyzeDocx,
  analyzeEpub,
  analyzeFb2,
  analyzeTxt,
  clearDocumentJob,
  exportDocxJob,
  exportEpubJob,
  exportFb2Job,
  exportTxtJob,
  resumeDocxJob,
  resumeEpubJob,
  resumeFb2Job,
  resumeTxtJob,
  startDocxJob,
  startEpubJob,
  startFb2Job,
  startTxtJob,
  formatTiming,
} from './commands';

describe('translation timing formatter', () => {
  it('formats exact provider usage and output speed', () => {
    expect(
      formatTiming({
        latency_ms: 3779,
        prompt_tokens: 214,
        completion_tokens: 118,
        total_tokens: 332,
      }),
    ).toBe('3779 ms · 31.2 tok/s · 214 in · 118 out · 332 total');
  });

  it('reports unavailable usage without estimating tokens', () => {
    expect(
      formatTiming({
        latency_ms: 3779,
        prompt_tokens: null,
        completion_tokens: null,
        total_tokens: null,
      }),
    ).toBe('3779 ms · token usage unavailable');
  });
});

const commandCases = [
  {
    format: 'txt',
    analyze: analyzeTxt,
    start: startTxtJob,
    resume: resumeTxtJob,
    export: exportTxtJob,
  },
  {
    format: 'docx',
    analyze: analyzeDocx,
    start: startDocxJob,
    resume: resumeDocxJob,
    export: exportDocxJob,
  },
  {
    format: 'epub',
    analyze: analyzeEpub,
    start: startEpubJob,
    resume: resumeEpubJob,
    export: exportEpubJob,
  },
  {
    format: 'fb2',
    analyze: analyzeFb2,
    start: startFb2Job,
    resume: resumeFb2Job,
    export: exportFb2Job,
  },
] as const;

describe('document command wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({});
  });

  it.each(commandCases)(
    'maps $format wrappers to native command names and payloads',
    async (entry) => {
      await entry.analyze('C:\\books\\story.' + entry.format, 'ru', 'en');
      expect(invokeMock).toHaveBeenLastCalledWith(`analyze_${entry.format}`, {
        sourcePath: `C:\\books\\story.${entry.format}`,
        sourceLanguage: 'ru',
        targetLanguage: 'en',
        translationStyle: 'neutral',
      });

      await entry.start('job-1');
      expect(invokeMock).toHaveBeenLastCalledWith(`start_${entry.format}_job`, {
        jobId: 'job-1',
      });

      await entry.resume('job-1');
      expect(invokeMock).toHaveBeenLastCalledWith(
        `resume_${entry.format}_job`,
        {
          jobId: 'job-1',
        },
      );

      await entry.export('job-1', `C:\\out\\story.${entry.format}`);
      expect(invokeMock).toHaveBeenLastCalledWith(
        `export_${entry.format}_job`,
        {
          jobId: 'job-1',
          outputPath: `C:\\out\\story.${entry.format}`,
        },
      );
    },
  );

  it('maps persistent document cleanup to the native command', async () => {
    await clearDocumentJob('job-1');
    expect(invokeMock).toHaveBeenLastCalledWith('clear_document_job', {
      jobId: 'job-1',
    });
  });
});
