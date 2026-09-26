#!/usr/bin/env node

import {
  createReadStream,
  readdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { createInterface } from 'node:readline';
import { gunzipSync } from 'node:zlib';

const args = parseArgs(process.argv.slice(2));
if (!args.kaikki && !args.morphology && !args['stardict-dir']) {
  throw new Error(
    'Provide --kaikki <jsonl>, --morphology <tsv>, or --stardict-dir <directory>',
  );
}
if (!args.output) {
  throw new Error('Provide --output <json>');
}

const entries = new Map();

if (args.kaikki) {
  await readLines(args.kaikki, (line) =>
    addKaikkiRecord(JSON.parse(line), args['kaikki-language'] || 'ru'),
  );
}
if (args.morphology) {
  await readLines(args.morphology, (line) =>
    addMorphologyRecord(line, args['morphology-language'] || 'ru'),
  );
}
for (const directory of asList(args['stardict-dir'])) {
  addStarDictDirectory(directory);
}

const output = [...entries.values()]
  .map((entry) => ({
    ...entry,
    forms: [...entry.forms].sort(),
    translations: [...entry.translations].sort(),
    definitions: [...entry.definitions],
    synonyms: [...entry.synonyms].sort(),
    antonyms: [...entry.antonyms].sort(),
    related_words: [...entry.related_words].sort(),
    examples: [...entry.examples],
    providers: [...entry.providers].sort(),
  }))
  .sort((left, right) => left.lemma.localeCompare(right.lemma, 'ru'));

writeFileSync(args.output, `${JSON.stringify(output)}\n`, 'utf8');

function parseArgs(values) {
  const result = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith('--')) {
      throw new Error(`Unexpected argument: ${value}`);
    }
    const key = value.slice(2);
    const argument = values[++index];
    result[key] = result[key] ? [...asList(result[key]), argument] : argument;
  }
  return result;
}

function asList(value) {
  return value === undefined ? [] : Array.isArray(value) ? value : [value];
}

async function readLines(path, onLine) {
  const input = createInterface({
    input: createReadStream(path, { encoding: 'utf8' }),
    crlfDelay: Infinity,
  });
  for await (const line of input) {
    if (line.trim()) {
      onLine(line);
    }
  }
}

function addKaikkiRecord(record, language) {
  const lemma = normalize(record.word);
  if (!lemma) {
    return;
  }
  const sourceLanguage = normalizeLanguage(record.lang_code || language);
  const entry = getEntry(lemma, sourceLanguage);
  if (entry.part_of_speech === 'unknown') {
    entry.part_of_speech = record.pos || 'unknown';
  }
  for (const form of record.forms || []) {
    add(entry.forms, form.form);
  }
  for (const sense of record.senses || []) {
    for (const gloss of sense.glosses || []) {
      add(entry.definitions, gloss);
    }
    for (const translation of sense.translations || []) {
      if (
        translation.lang_code !== sourceLanguage &&
        translation.lang?.toLowerCase() !== languageName(sourceLanguage)
      ) {
        add(entry.translations, translation.word);
      }
    }
    for (const synonym of sense.synonyms || []) {
      add(entry.synonyms, synonym.word);
    }
    for (const example of sense.examples || []) {
      add(entry.examples, example.text || example.example);
    }
  }
}

function addMorphologyRecord(line, language) {
  const [lemmaValue, formValue] = line.split('\t');
  const lemma = normalize(lemmaValue);
  const form = normalize(formValue);
  if (!lemma || !form || line.startsWith('#')) {
    return;
  }
  add(getEntry(lemma, normalizeLanguage(language)).forms, form);
}

function addStarDictDirectory(directory) {
  for (const file of readdirSync(directory)) {
    if (!file.endsWith('.idx.gz')) {
      continue;
    }
    const basename = file.slice(0, -'.idx.gz'.length);
    const [sourceLanguage, targetLanguage] = basename.split('-');
    if (!sourceLanguage || !targetLanguage) {
      continue;
    }
    const dictionary = readFileSync(`${directory}/${basename}.dict`);
    const index = gunzipSync(readFileSync(`${directory}/${file}`));
    for (let offset = 0; offset < index.length;) {
      const wordEnd = index.indexOf(0, offset);
      if (wordEnd < 0 || wordEnd + 9 > index.length) {
        break;
      }
      const word = index.toString('utf8', offset, wordEnd);
      const contentOffset = index.readUInt32BE(wordEnd + 1);
      const contentLength = index.readUInt32BE(wordEnd + 5);
      const content = cleanDictionaryText(
        dictionary
          .subarray(contentOffset, contentOffset + contentLength)
          .toString('utf8'),
      );
      if (word && content) {
        const entry = getEntry(
          normalize(word),
          normalizeLanguage(sourceLanguage),
        );
        entry.part_of_speech = `FreeDict ${sourceLanguage.toUpperCase()}-${targetLanguage.toUpperCase()}`;
        add(entry.translations, content);
        addLabel(
          entry.providers,
          `FreeDict ${sourceLanguage.toUpperCase()}-${targetLanguage.toUpperCase()}`,
        );
      }
      offset = wordEnd + 9;
    }
  }
}

function cleanDictionaryText(value) {
  return value
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/\s+/g, ' ')
    .replaceAll('\uFFFD', '')
    .trim()
    .slice(0, 1200);
}

function getEntry(lemma, language) {
  const key = `${language}:${lemma}`;
  if (!entries.has(key)) {
    entries.set(key, {
      lemma,
      language,
      part_of_speech: 'unknown',
      translations: new Set(),
      definitions: new Set(),
      forms: new Set(),
      synonyms: new Set(),
      antonyms: new Set(),
      related_words: new Set(),
      examples: new Set(),
      providers: new Set(),
    });
  }
  return entries.get(key);
}

function add(set, value) {
  const normalized = normalize(value);
  if (normalized) {
    set.add(normalized);
  }
}

function addLabel(set, value) {
  if (typeof value === 'string' && value.trim()) {
    set.add(value.trim());
  }
}

function normalize(value) {
  return typeof value === 'string' ? value.trim().toLowerCase() : '';
}

function normalizeLanguage(value) {
  return { eng: 'en', rus: 'ru' }[value] || value;
}

function languageName(value) {
  return (
    {
      de: 'german',
      en: 'english',
      es: 'spanish',
      fr: 'french',
      it: 'italian',
      pl: 'polish',
      pt: 'portuguese',
      ru: 'russian',
      uk: 'ukrainian',
    }[value] || value
  );
}
