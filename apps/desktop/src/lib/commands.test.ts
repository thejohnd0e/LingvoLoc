import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

import {
  analyzeDocx,
  analyzeEpub,
  analyzeTxt,
  exportDocxJob,
  exportEpubJob,
  exportTxtJob,
  resumeDocxJob,
  resumeEpubJob,
  resumeTxtJob,
  startDocxJob,
  startEpubJob,
  startTxtJob,
} from './commands';

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
});
