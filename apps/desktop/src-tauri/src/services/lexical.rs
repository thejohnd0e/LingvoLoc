use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;
use zip::ZipArchive;

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
const MAX_CACHED_DICTIONARIES: usize = 16;

type CachedDictionary = (u64, Arc<Vec<LexicalEntry>>);

/// Parsed StarDict folders, so a lookup does not decompress and re-parse every dictionary.
static DICTIONARY_CACHE: Mutex<Option<HashMap<PathBuf, CachedDictionary>>> = Mutex::new(None);

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

    let matches: Vec<LexicalEntry> = user_entry_sets(user_directory, enabled_user_dictionaries)
        .iter()
        .flat_map(|set| set.iter())
        .filter(|entry| {
            language.is_none_or(|value| entry.language == value)
                && (entry.lemma == normalized || entry.forms.iter().any(|form| form == &normalized))
        })
        .cloned()
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
    let root_entries = cached_stardict_entries(user_directory);
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
                let entries = cached_stardict_entries(&file.path());
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

fn user_entry_sets(
    user_directory: Option<&Path>,
    enabled_user_dictionaries: Option<&[String]>,
) -> Vec<Arc<Vec<LexicalEntry>>> {
    let Some(directory) = user_directory else {
        return Vec::new();
    };
    let Ok(files) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut sets = vec![cached_stardict_entries(directory)];
    sets.extend(files.filter_map(Result::ok).filter_map(|file| {
        let path = file.path();
        let id = file.file_name().to_string_lossy().to_string();
        (path.is_dir()
            && enabled_user_dictionaries
                .is_none_or(|enabled| enabled.iter().any(|value| value == &id)))
        .then(|| cached_stardict_entries(&path))
    }));
    sets
}

/// Fingerprint of every file in a dictionary folder (name, size, modification time).
fn dictionary_stamp(directory: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Ok(files) = fs::read_dir(directory) {
        let mut files: Vec<_> = files
            .filter_map(Result::ok)
            .filter(|file| file.path().is_file())
            .collect();
        files.sort_by_key(|file| file.file_name());
        for file in files {
            file.file_name().hash(&mut hasher);
            if let Ok(metadata) = file.metadata() {
                metadata.len().hash(&mut hasher);
                metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos())
                    .hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn cached_stardict_entries(directory: &Path) -> Arc<Vec<LexicalEntry>> {
    let stamp = dictionary_stamp(directory);
    if let Ok(guard) = DICTIONARY_CACHE.lock() {
        if let Some((cached_stamp, entries)) = guard.as_ref().and_then(|cache| cache.get(directory))
        {
            if *cached_stamp == stamp {
                return Arc::clone(entries);
            }
        }
    }
    let entries = Arc::new(user_stardict_entries(directory));
    if let Ok(mut guard) = DICTIONARY_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if cache.len() >= MAX_CACHED_DICTIONARIES {
            cache.clear();
        }
        cache.insert(directory.to_path_buf(), (stamp, Arc::clone(&entries)));
    }
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
    let ifo_content = fs::read_to_string(ifo.path()).unwrap_or_default();
    let provider = ifo_content
        .lines()
        .find_map(|line| line.strip_prefix("bookname="))
        .unwrap_or("User StarDict")
        .trim()
        .to_string();
    let basename_text = basename
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let language = dictionary_source_language(&provider)
        .or_else(|| dictionary_source_language(&basename_text))
        .unwrap_or("und");
    let mut entries =
        parse_stardict_index_with_audio(&index, &dictionary, provider, Some(directory));
    for entry in &mut entries {
        entry.language = language.into();
    }
    entries
}

/// Maps the source side of a dictionary name such as `rus-eng` or
/// `Russian-English` to an ISO 639-1 code.
fn dictionary_source_language(name: &str) -> Option<&'static str> {
    const LANGUAGES: [(&str, &str, &str); 12] = [
        ("en", "eng", "english"),
        ("ru", "rus", "russian"),
        ("de", "deu", "german"),
        ("fr", "fra", "french"),
        ("es", "spa", "spanish"),
        ("it", "ita", "italian"),
        ("pt", "por", "portuguese"),
        ("pl", "pol", "polish"),
        ("uk", "ukr", "ukrainian"),
        ("tr", "tur", "turkish"),
        ("zh", "zho", "chinese"),
        ("ja", "jpn", "japanese"),
    ];
    let lowered = name.to_lowercase();
    let first = lowered
        .split(|c: char| !c.is_alphabetic())
        .find(|part| !part.is_empty())?;
    LANGUAGES
        .iter()
        .find(|(short, code, full)| first == *short || first == *code || first == *full)
        .map(|(short, _, _)| *short)
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

#[cfg(test)]
fn parse_stardict_index(index: &[u8], dictionary: &[u8], provider: String) -> Vec<LexicalEntry> {
    parse_stardict_index_with_audio(index, dictionary, provider, None)
}

fn parse_stardict_index_with_audio(
    index: &[u8],
    dictionary: &[u8],
    provider: String,
    audio_directory: Option<&Path>,
) -> Vec<LexicalEntry> {
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
            let raw_definition = String::from_utf8_lossy(&dictionary[content_offset..end]);
            let definition = clean_user_definition(&raw_definition);
            if !definition.is_empty() {
                let definition_html = add_audio_controls(&raw_definition, audio_directory);
                entries.push(LexicalEntry {
                    lemma,
                    language: "und".into(),
                    part_of_speech: "User dictionary".into(),
                    translations: Vec::new(),
                    definitions: vec![definition_html],
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

fn add_audio_controls(value: &str, directory: Option<&Path>) -> String {
    let Some(directory) = directory else {
        return value.to_owned();
    };
    let mut controls: Vec<(String, String, &'static str)> = Vec::new();
    for token in value.split(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                '"' | '\'' | '<' | '>' | '[' | ']' | '(' | ')' | ',' | ';'
            )
    }) {
        let candidate = token
            .trim_start_matches("src=")
            .trim_start_matches("sound://")
            .trim_start_matches("sound:")
            .trim_matches(|character| matches!(character, '/' | '{' | '}'));
        if candidate.is_empty() || candidate.contains("..") {
            continue;
        }
        let path = Path::new(candidate);
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(kind) = media_kind(extension) else {
            continue;
        };
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
        {
            continue;
        }
        let label = escape_html(candidate);
        if !controls.iter().any(|control| control.0 == candidate) {
            controls.push((candidate.to_owned(), label, kind));
        }
    }
    if controls.is_empty() {
        return value.to_owned();
    }
    let mut visible_value = value.to_owned();
    for (resource, _, _) in &controls {
        for reference in [
            resource.clone(),
            format!("sound://{resource}"),
            format!("sound:{resource}"),
            format!("src={resource}"),
        ] {
            visible_value = visible_value.replace(&reference, "");
        }
    }
    if controls.iter().any(|(_, _, kind)| *kind == "image") {
        visible_value = visible_value.replace("See picture:", "");
        visible_value = visible_value.replace("See picture", "");
    }
    let audio_controls = controls
        .into_iter()
        .map(|(resource, _label, kind)| {
            let resource = escape_html(&resource);
            let directory = escape_html(&directory.to_string_lossy());
            let tag = if kind == "audio" {
                format!(
                    "<audio controls preload=\"none\" data-dictionary-media-resource=\"{resource}\"></audio>"
                )
            } else {
                format!(
                    "<img loading=\"lazy\" data-dictionary-media-resource=\"{resource}\">"
                )
            };
            format!("<div class=\"dictionary-media\" data-dictionary-media-directory=\"{directory}\">{tag}</div>")
        })
        .collect::<String>();
    format!("{visible_value}{audio_controls}")
}

fn media_kind(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "wav" | "mp3" | "ogg" | "oga" | "flac" | "m4a" | "aac" => Some("audio"),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" => Some("image"),
        _ => None,
    }
}

pub fn read_media_data_uri(directory: &Path, resource: &str) -> Result<String, String> {
    let path = Path::new(resource);
    if resource.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        return Err("invalid dictionary media path".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "dictionary media extension is missing".to_string())?;
    media_kind(extension).ok_or_else(|| "unsupported dictionary media type".to_string())?;
    let mime = match extension.to_ascii_lowercase().as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => unreachable!(),
    };
    let bytes = read_media_bytes(directory, path)?;
    Ok(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn read_media_bytes(directory: &Path, resource: &Path) -> Result<Vec<u8>, String> {
    if let Some(media_path) = find_media_path(directory, resource) {
        return fs::read(media_path).map_err(|error| error.to_string());
    }
    let archive_path = find_archive_path(directory)
        .ok_or_else(|| "dictionary media file was not found".to_string())?;
    let archive_file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(archive_file).map_err(|error| error.to_string())?;
    let resource_name = resource
        .file_name()
        .ok_or_else(|| "dictionary media filename is missing".to_string())?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = Path::new(file.name());
        if name == resource || name.file_name() == Some(resource_name) {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            return Ok(bytes);
        }
    }
    Err("dictionary media file was not found".into())
}

fn find_media_path(directory: &Path, resource: &Path) -> Option<std::path::PathBuf> {
    let direct = directory.join(resource);
    if direct.is_file() {
        return Some(direct);
    }
    let file_name = resource.file_name()?;
    find_media_file_recursive(directory, file_name).or_else(|| {
        directory
            .parent()
            .and_then(|parent| find_media_file_recursive(parent, file_name))
    })
}

fn find_media_file_recursive(
    directory: &Path,
    file_name: &std::ffi::OsStr,
) -> Option<std::path::PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.file_name() == Some(file_name) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_media_file_recursive(&path, file_name) {
                return Some(found);
            }
        }
    }
    None
}

fn find_archive_path(directory: &Path) -> Option<std::path::PathBuf> {
    find_file_recursive(directory, "res.zip").or_else(|| {
        directory
            .parent()
            .and_then(|parent| find_file_recursive(parent, "res.zip"))
    })
}

fn find_file_recursive(directory: &Path, file_name: &str) -> Option<std::path::PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|value| value.to_str()) == Some(file_name) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, file_name) {
                return Some(found);
            }
        }
    }
    None
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

    #[test]
    fn maps_dictionary_names_to_source_languages() {
        use super::dictionary_source_language as map;
        assert_eq!(map("rus-eng"), Some("ru"));
        assert_eq!(map("Russian-English dictionary"), Some("ru"));
        assert_eq!(map("eng-rus"), Some("en"));
        assert_eq!(map("Big Explanatory"), None);
        assert_eq!(map(""), None);
    }

    #[test]
    fn user_stardict_folder_gets_language_from_bookname() {
        let directory =
            std::env::temp_dir().join(format!("lingvoloc-lang-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let word = "дом";
        let body = "house";
        let mut index = word.as_bytes().to_vec();
        index.push(0);
        index.extend_from_slice(&0_u32.to_be_bytes());
        index.extend_from_slice(&(body.len() as u32).to_be_bytes());
        std::fs::write(directory.join("d.idx"), index).unwrap();
        std::fs::write(directory.join("d.dict"), body).unwrap();
        std::fs::write(
            directory.join("d.ifo"),
            "StarDict's dict ifo file\nbookname=rus-eng\n",
        )
        .unwrap();
        let entries = super::user_stardict_entries(&directory);
        assert_eq!(entries[0].lemma, "дом");
        assert_eq!(entries[0].language, "ru");
        assert_eq!(entries[0].providers, vec!["rus-eng"]);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cached_dictionary_reloads_when_files_change() {
        let directory = std::env::temp_dir().join(format!(
            "lingvoloc-cache-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let write_dictionary = |word: &str| {
            let body = b"definition";
            let mut index = word.as_bytes().to_vec();
            index.push(0);
            index.extend_from_slice(&0_u32.to_be_bytes());
            index.extend_from_slice(&(body.len() as u32).to_be_bytes());
            std::fs::write(directory.join("d.idx"), index).unwrap();
            std::fs::write(directory.join("d.dict"), body).unwrap();
            std::fs::write(
                directory.join("d.ifo"),
                "StarDict's dict ifo file\nbookname=eng-rus\n",
            )
            .unwrap();
        };
        write_dictionary("house");
        let first = super::cached_stardict_entries(&directory);
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &super::cached_stardict_entries(&directory)
        ));
        assert_eq!(first[0].lemma, "house");
        write_dictionary("garden");
        let changed = super::cached_stardict_entries(&directory);
        assert_eq!(changed[0].lemma, "garden");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ignores_truncated_index_records() {
        let mut index = b"house\0".to_vec();
        index.extend_from_slice(&0_u32.to_be_bytes());
        assert!(super::parse_stardict_index(&index, b"Home", "Example".into()).is_empty());
    }

    #[test]
    fn embeds_referenced_audio_files_in_user_records() {
        let directory =
            std::env::temp_dir().join(format!("lingvoloc-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("example.wav"), [0_u8, 1, 2, 3]).unwrap();
        let mut index = b"house\0".to_vec();
        index.extend_from_slice(&0_u32.to_be_bytes());
        index.extend_from_slice(&25_u32.to_be_bytes());
        let entries = super::parse_stardict_index_with_audio(
            &index,
            b"<b>Home</b> example.wav",
            "Example".into(),
            Some(&directory),
        );
        assert!(entries[0].definitions[0].contains("<audio controls"));
        assert!(
            entries[0].definitions[0].contains("data-dictionary-media-resource=\"example.wav\"")
        );
        assert!(!entries[0].definitions[0].contains("<b>Home</b> example.wav"));
        assert_eq!(
            super::read_media_data_uri(&directory, "example.wav").unwrap(),
            "data:audio/wav;base64,AAECAw=="
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn finds_media_files_in_nested_dictionary_folders() {
        let directory =
            std::env::temp_dir().join(format!("lingvoloc-media-test-{}", std::process::id()));
        let nested = directory.join("media");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("example.wav"), [0_u8, 1, 2, 3]).unwrap();

        assert_eq!(
            super::read_media_data_uri(&directory, "example.wav").unwrap(),
            "data:audio/wav;base64,AAECAw=="
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reads_media_files_from_resource_zip() {
        let directory =
            std::env::temp_dir().join(format!("lingvoloc-zip-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let archive_file = std::fs::File::create(directory.join("res.zip")).unwrap();
        let mut archive = zip::ZipWriter::new(archive_file);
        archive
            .start_file(
                "sounds/example.wav",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        std::io::Write::write_all(&mut archive, &[0_u8, 1, 2, 3]).unwrap();
        archive.finish().unwrap();

        assert_eq!(
            super::read_media_data_uri(&directory, "example.wav").unwrap(),
            "data:audio/wav;base64,AAECAw=="
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
