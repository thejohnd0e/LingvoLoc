import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import path from 'node:path';

const buildNumber = readFileSync(
  path.resolve(process.cwd(), '../../BUILD_NUMBER'),
  'utf8',
).trim();

export default defineConfig({
  plugins: [react()],
  define: {
    'import.meta.env.VITE_BUILD_NUMBER': JSON.stringify(buildNumber),
  },
  test: {
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
});
