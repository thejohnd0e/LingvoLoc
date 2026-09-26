use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LexicalEntry {
    pub lemma: String,
    pub language: String,
    pub part_of_speech: String,
    pub translations: Vec<String>,
    pub definitions: Vec<String>,
    pub forms: Vec<String>,
    pub synonyms: Vec<String>,
    pub antonyms: Vec<String>,
    pub related_words: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserDictionary {
    pub id: String,
    pub name: String,
    pub entry_count: usize,
}

#[allow(dead_code)]
static FREEDICT_ENTRIES: OnceLock<Vec<LexicalEntry>> = OnceLock::new();

#[allow(dead_code)]
fn freedict_entries() -> &'static [LexicalEntry] {
    FREEDICT_ENTRIES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../data/lexical/freedict-en-ru.json"))
                .expect("bundled FreeDict index must be valid JSON")
        })
        .as_slice()
}

#[allow(dead_code)]
fn seed_entries() -> Vec<LexicalEntry> {
    vec![
        LexicalEntry {
            lemma: "learn".into(),
            language: "en".into(),
            part_of_speech: "verb".into(),
            translations: vec!["учиться".into(), "узнавать".into()],
            definitions: vec!["To gain knowledge or a skill through study or experience.".into()],
            forms: vec![
                "learns".into(),
                "learned".into(),
                "learnt".into(),
                "learning".into(),
            ],
            synonyms: vec!["study".into(), "discover".into()],
            antonyms: vec!["forget".into()],
            related_words: vec!["learner".into(), "lesson".into(), "knowledge".into()],
            examples: vec!["She learns quickly from experience.".into()],
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "translate".into(),
            language: "en".into(),
            part_of_speech: "verb".into(),
            translations: vec!["переводить".into()],
            definitions: vec!["To express the meaning of words in another language.".into()],
            forms: vec![
                "translates".into(),
                "translated".into(),
                "translating".into(),
            ],
            synonyms: vec!["interpret".into(), "render".into()],
            antonyms: vec![],
            related_words: vec!["translation".into(), "translator".into(), "language".into()],
            examples: vec!["Please translate this sentence into Russian.".into()],
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "language".into(),
            language: "en".into(),
            part_of_speech: "noun".into(),
            translations: vec!["язык".into(), "речь".into()],
            definitions: vec!["A system of words and rules used for communication.".into()],
            forms: vec!["languages".into()],
            synonyms: vec!["speech".into(), "tongue".into()],
            antonyms: vec![],
            related_words: vec!["word".into(), "grammar".into(), "meaning".into()],
            examples: vec!["Language changes over time.".into()],
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "good".into(),
            language: "en".into(),
            part_of_speech: "adjective".into(),
            translations: vec!["хороший".into(), "добрый".into()],
            definitions: vec!["Having the qualities that are wanted or valued.".into()],
            forms: vec!["better".into(), "best".into()],
            synonyms: vec!["fine".into(), "excellent".into()],
            antonyms: vec!["bad".into()],
            related_words: vec!["goodness".into(), "well".into()],
            examples: vec!["That was a good decision.".into()],
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "word".into(),
            language: "en".into(),
            part_of_speech: "noun".into(),
            translations: vec!["слово".into()],
            definitions: vec!["A single unit of language with meaning.".into()],
            forms: vec!["words".into()],
            synonyms: vec!["term".into(), "expression".into()],
            antonyms: vec![],
            related_words: vec!["sentence".into(), "text".into(), "language".into()],
            examples: vec!["I do not know the meaning of this word.".into()],
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "beyond".into(),
            language: "en".into(),
            part_of_speech: "preposition/adverb".into(),
            translations: vec!["за пределами".into(), "дальше".into()],
            definitions: vec!["At or to the farther side of something.".into()],
            forms: vec![],
            synonyms: vec!["past".into(), "outside".into()],
            antonyms: vec!["within".into()],
            related_words: vec!["farther".into(), "outside".into(), "limit".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "coverage".into(),
            language: "en".into(),
            part_of_speech: "noun".into(),
            translations: vec!["охват".into(), "покрытие".into()],
            definitions: vec!["The extent to which something is included or protected.".into()],
            forms: vec![],
            synonyms: vec!["range".into(), "extent".into()],
            antonyms: vec![],
            related_words: vec!["cover".into(), "scope".into(), "area".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "roughly".into(),
            language: "en".into(),
            part_of_speech: "adverb".into(),
            translations: vec!["примерно".into(), "грубо".into()],
            definitions: vec!["Approximately or without exact detail.".into()],
            forms: vec![],
            synonyms: vec!["approximately".into(), "about".into()],
            antonyms: vec!["exactly".into()],
            related_words: vec!["rough".into(), "estimate".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "read".into(),
            language: "en".into(),
            part_of_speech: "verb".into(),
            translations: vec!["читать".into()],
            definitions: vec!["To look at and understand written words.".into()],
            forms: vec!["reads".into(), "reading".into(), "read".into()],
            synonyms: vec!["study".into(), "scan".into()],
            antonyms: vec!["ignore".into()],
            related_words: vec!["reader".into(), "book".into(), "text".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "write".into(),
            language: "en".into(),
            part_of_speech: "verb".into(),
            translations: vec!["писать".into()],
            definitions: vec!["To form letters or words on a surface or screen.".into()],
            forms: vec![
                "writes".into(),
                "wrote".into(),
                "written".into(),
                "writing".into(),
            ],
            synonyms: vec!["record".into(), "compose".into()],
            antonyms: vec![],
            related_words: vec!["writer".into(), "sentence".into(), "text".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "speak".into(),
            language: "en".into(),
            part_of_speech: "verb".into(),
            translations: vec!["говорить".into()],
            definitions: vec!["To use spoken words to communicate.".into()],
            forms: vec![
                "speaks".into(),
                "spoke".into(),
                "spoken".into(),
                "speaking".into(),
            ],
            synonyms: vec!["talk".into(), "communicate".into()],
            antonyms: vec!["listen".into()],
            related_words: vec!["speaker".into(), "speech".into(), "language".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "перевод".into(),
            language: "ru".into(),
            part_of_speech: "noun".into(),
            translations: vec!["translation".into()],
            definitions: vec![
                "The act or result of expressing meaning in another language.".into(),
            ],
            forms: vec!["переводы".into()],
            synonyms: vec!["переложение".into()],
            antonyms: vec![],
            related_words: vec!["переводить".into(), "язык".into(), "текст".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "слово".into(),
            language: "ru".into(),
            part_of_speech: "noun".into(),
            translations: vec!["word".into()],
            definitions: vec!["A unit of language that carries meaning.".into()],
            forms: vec!["слова".into(), "слов".into()],
            synonyms: vec!["термин".into(), "выражение".into()],
            antonyms: vec![],
            related_words: vec!["текст".into(), "предложение".into(), "язык".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "язык".into(),
            language: "ru".into(),
            part_of_speech: "noun".into(),
            translations: vec!["language".into(), "tongue".into()],
            definitions: vec!["A system of communication used by a community.".into()],
            forms: vec!["языки".into(), "языка".into()],
            synonyms: vec!["речь".into()],
            antonyms: vec![],
            related_words: vec!["слово".into(), "грамматика".into(), "перевод".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
        LexicalEntry {
            lemma: "несколько".into(),
            language: "ru".into(),
            part_of_speech: "determiner/pronoun".into(),
            translations: vec!["several".into(), "a few".into()],
            definitions: vec!["More than two but not many.".into()],
            forms: vec![],
            synonyms: vec!["немного".into(), "ряд".into()],
            antonyms: vec!["много".into(), "один".into()],
            related_words: vec!["количество".into(), "часть".into()],
            examples: Vec::new(),
            providers: vec!["LingvoLoc seed".into()],
        },
    ]
}

#[cfg(test)]
pub fn lookup(query: &str, language: Option<&str>) -> Vec<LexicalEntry> {
    lookup_with_user_directory(query, language, None, None)
}

pub fn lookup_with_user_directory(
    query: &str,
    language: Option<&str>,
    user_directory: Option<&Path>,
    enabled_user_dictionaries: Option<&[String]>,
) -> Vec<LexicalEntry> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }

    let matches: Vec<LexicalEntry> = user_entries(user_directory, enabled_user_dictionaries)
        .into_iter()
        .filter(|entry| {
            language.is_none_or(|value| entry.language == value)
                && (entry.lemma == normalized || entry.forms.iter().any(|form| form == &normalized))
        })
        .collect();
    if !matches.is_empty() {
        return merge_entries(matches);
    }

    merge_entries(matches)
}

pub fn list_user_dictionaries(user_directory: &Path) -> Vec<UserDictionary> {
    let Ok(files) = fs::read_dir(user_directory) else {
        return Vec::new();
    };
    let mut dictionaries = Vec::new();
    let root_entries = user_stardict_entries(user_directory);
    if let Some(entry) = root_entries.first() {
        dictionaries.push(UserDictionary {
            id: ".".into(),
            name: entry
                .providers
                .first()
                .cloned()
                .unwrap_or_else(|| "User StarDict".into()),
            entry_count: root_entries.len(),
        });
    }
    dictionaries.extend(
        files
            .filter_map(Result::ok)
            .filter(|file| file.path().is_dir())
            .filter_map(|file| {
                let entries = user_stardict_entries(&file.path());
                entries.first().map(|entry| UserDictionary {
                    id: file.file_name().to_string_lossy().to_string(),
                    name: entry
                        .providers
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "User StarDict".into()),
                    entry_count: entries.len(),
                })
            }),
    );
    dictionaries
}

fn user_entries(
    user_directory: Option<&Path>,
    enabled_user_dictionaries: Option<&[String]>,
) -> Vec<LexicalEntry> {
    let Some(directory) = user_directory else {
        return Vec::new();
    };
    let Ok(files) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut entries = user_stardict_entries(directory);
    entries.extend(
        files
            .filter_map(Result::ok)
            .flat_map(|file| {
                let path = file.path();
                let id = file.file_name().to_string_lossy().to_string();
                if path.is_dir()
                    && enabled_user_dictionaries
                        .is_none_or(|enabled| enabled.iter().any(|value| value == &id))
                {
                    user_stardict_entries(&path)
                } else {
                    Vec::new()
                }
            })
            .collect::<Vec<_>>(),
    );
    entries
}

fn user_stardict_entries(directory: &Path) -> Vec<LexicalEntry> {
    let Some(ifo) = fs::read_dir(directory).ok().and_then(|files| {
        files
            .filter_map(Result::ok)
            .find(|file| file.path().extension().is_some_and(|value| value == "ifo"))
    }) else {
        return Vec::new();
    };
    let basename = ifo.path().with_extension("");
    let idx_path = basename.with_extension("idx");
    let idx_gz_path = basename.with_extension("idx.gz");
    let dict_path = basename.with_extension("dict");
    let dict_dz_path = basename.with_extension("dict.dz");
    let index = read_maybe_gzip(&idx_path, &idx_gz_path).ok();
    let dictionary = read_dictionary(&dict_path, &dict_dz_path).ok();
    let (Some(index), Some(dictionary)) = (index, dictionary) else {
        return Vec::new();
    };
    let provider = fs::read_to_string(ifo.path())
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find_map(|line| line.strip_prefix("bookname="))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "User StarDict".into())
        .trim()
        .to_string();
    parse_stardict_index(&index, &dictionary, provider)
}

fn read_maybe_gzip(path: &Path, compressed_path: &Path) -> Result<Vec<u8>, std::io::Error> {
    if path.exists() {
        fs::read(path)
    } else {
        let bytes = fs::read(compressed_path)?;
        let mut decoder = GzDecoder::new(Cursor::new(bytes));
        let mut output = Vec::new();
        decoder.read_to_end(&mut output)?;
        Ok(output)
    }
}

fn read_dictionary(path: &Path, compressed_path: &Path) -> Result<Vec<u8>, std::io::Error> {
    if path.exists() {
        fs::read(path)
    } else {
        let bytes = fs::read(compressed_path)?;
        let mut decoder = GzDecoder::new(Cursor::new(bytes));
        let mut output = Vec::new();
        decoder.read_to_end(&mut output)?;
        Ok(output)
    }
}

fn parse_stardict_index(index: &[u8], dictionary: &[u8], provider: String) -> Vec<LexicalEntry> {
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset < index.len() {
        let Some(word_end) = index[offset..].iter().position(|byte| *byte == 0) else {
            break;
        };
        let word_end = offset + word_end;
        if word_end + 9 > index.len() {
            break;
        }
        let lemma = String::from_utf8_lossy(&index[offset..word_end])
            .trim()
            .to_lowercase();
        let content_offset =
            u32::from_be_bytes(index[word_end + 1..word_end + 5].try_into().unwrap()) as usize;
        let content_length =
            u32::from_be_bytes(index[word_end + 5..word_end + 9].try_into().unwrap()) as usize;
        let end = content_offset
            .saturating_add(content_length)
            .min(dictionary.len());
        if !lemma.is_empty() && content_offset < end {
            let definition =
                clean_user_definition(&String::from_utf8_lossy(&dictionary[content_offset..end]));
            if !definition.is_empty() {
                entries.push(LexicalEntry {
                    lemma,
                    language: "und".into(),
                    part_of_speech: "User dictionary".into(),
                    translations: Vec::new(),
                    definitions: vec![
                        String::from_utf8_lossy(&dictionary[content_offset..end]).into_owned()
                    ],
                    forms: Vec::new(),
                    synonyms: Vec::new(),
                    antonyms: Vec::new(),
                    related_words: Vec::new(),
                    examples: Vec::new(),
                    providers: vec![provider.clone()],
                });
            }
        }
        offset = word_end + 9;
    }
    entries
}

fn clean_user_definition(value: &str) -> String {
    let mut plain = String::new();
    let mut in_tag = false;
    for character in value.replace("\\n", "\n").chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                plain.push(' ');
            }
            _ if !in_tag => plain.push(character),
            _ => {}
        }
    }
    plain
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(2400)
        .collect()
}

fn merge_entries(entries: Vec<LexicalEntry>) -> Vec<LexicalEntry> {
    let mut results: Vec<LexicalEntry> = Vec::new();
    for entry in entries {
        if let Some(existing) = results.iter_mut().find(|existing| {
            existing.lemma == entry.lemma
                && existing.language == entry.language
                && existing.part_of_speech == entry.part_of_speech
        }) {
            merge_entry(existing, entry);
        } else {
            results.push(entry);
        }
    }
    results
}

fn merge_entry(result: &mut LexicalEntry, entry: LexicalEntry) {
    for value in entry.translations {
        add_unique(&mut result.translations, value);
    }
    for value in entry.definitions {
        add_unique(&mut result.definitions, value);
    }
    for value in entry.forms {
        add_unique(&mut result.forms, value);
    }
    for value in entry.synonyms {
        add_unique(&mut result.synonyms, value);
    }
    for value in entry.antonyms {
        add_unique(&mut result.antonyms, value);
    }
    for value in entry.related_words {
        add_unique(&mut result.related_words, value);
    }
    for value in entry.examples {
        add_unique(&mut result.examples, value);
    }
    for value in entry.providers {
        add_unique(&mut result.providers, value);
    }
}

fn add_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{lookup, merge_entries, LexicalEntry};

    fn test_entry(part_of_speech: &str, translation: &str) -> LexicalEntry {
        LexicalEntry {
            lemma: "test".into(),
            language: "en".into(),
            part_of_speech: part_of_speech.into(),
            translations: vec![translation.into()],
            definitions: Vec::new(),
            forms: Vec::new(),
            synonyms: Vec::new(),
            antonyms: Vec::new(),
            related_words: Vec::new(),
            examples: Vec::new(),
            providers: vec!["test".into()],
        }
    }

    #[test]
    fn bundled_dictionaries_are_not_used() {
        assert!(lookup("learn", Some("en")).is_empty());
        assert!(lookup("дом", Some("ru")).is_empty());
    }

    #[test]
    fn keeps_different_parts_of_speech_in_separate_cards() {
        let entries = merge_entries(vec![
            test_entry("noun", "a noun"),
            test_entry("verb", "a verb"),
            test_entry("noun", "another noun"),
        ]);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].translations, vec!["a noun", "another noun"]);
        assert_eq!(entries[1].translations, vec!["a verb"]);
    }

    #[test]
    fn parses_user_stardict_records() {
        let mut index = b"house\0".to_vec();
        index.extend_from_slice(&0_u32.to_be_bytes());
        index.extend_from_slice(&100_u32.to_be_bytes());
        let entries =
            super::parse_stardict_index(&index, b"<b>Home</b> &amp; garden\n", "Example".into());
        assert_eq!(entries[0].lemma, "house");
        assert!(entries[0].definitions[0].contains("<b>Home</b>"));
        assert_eq!(entries[0].providers, vec!["Example"]);
    }
}
