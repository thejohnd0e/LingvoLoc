import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const buildNumberPath = path.join(root, 'BUILD_NUMBER');
const current = Number.parseInt(
  (await readFile(buildNumberPath, 'utf8')).trim(),
  10,
);

if (!Number.isInteger(current) || current < 0) {
  throw new Error('BUILD_NUMBER must contain a non-negative integer');
}

const next = current + 1;
await writeFile(buildNumberPath, `${next}\n`, 'utf8');
console.log(`LingvoLoc build ${next}`);
