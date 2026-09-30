//! Cyrillic-capable fonts for translated text and their metrics.
//!
//! Fonts are read from the Windows font folder (Times New Roman for serif
//! source text, Arial for sans-serif). Bundling an OFL font instead is a
//! follow-up; the lookup order below already lets a bundled copy win.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::domain::RuntimeError;

use super::layout::Measure;

type Key = (bool, bool, bool);

pub struct FontSet {
    data: HashMap<Key, Vec<u8>>,
}

fn folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            folders.push(dir.join("fonts"));
            folders.push(dir.join("resources").join("fonts"));
        }
    }
    let windows = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    folders.push(PathBuf::from(windows).join("Fonts"));
    folders
}

fn file_name(serif: bool, bold: bool, italic: bool) -> &'static str {
    match (serif, bold, italic) {
        (true, false, false) => "times.ttf",
        (true, true, false) => "timesbd.ttf",
        (true, false, true) => "timesi.ttf",
        (true, true, true) => "timesbi.ttf",
        (false, false, false) => "arial.ttf",
        (false, true, false) => "arialbd.ttf",
        (false, false, true) => "ariali.ttf",
        (false, true, true) => "arialbi.ttf",
    }
}

impl FontSet {
    pub fn load() -> Result<Self, RuntimeError> {
        let mut data = HashMap::new();
        for serif in [true, false] {
            for bold in [false, true] {
                for italic in [false, true] {
                    let name = file_name(serif, bold, italic);
                    let bytes = folders()
                        .into_iter()
                        .find_map(|folder| std::fs::read(folder.join(name)).ok())
                        .ok_or_else(|| {
                            RuntimeError::InvalidInput(format!(
                                "font {name} was not found; PDF export needs Times New Roman and Arial"
                            ))
                        })?;
                    data.insert((serif, bold, italic), bytes);
                }
            }
        }
        Ok(Self { data })
    }

    pub fn bytes(&self, serif: bool, bold: bool, italic: bool) -> &[u8] {
        &self.data[&(serif, bold, italic)]
    }

    pub fn measure(&self, serif: bool) -> Metrics<'_> {
        Metrics {
            set: self,
            serif,
            cache: RefCell::new(HashMap::new()),
        }
    }
}

pub struct Metrics<'a> {
    set: &'a FontSet,
    serif: bool,
    cache: RefCell<HashMap<(bool, bool, char), f32>>,
}

impl Metrics<'_> {
    fn advance(&self, character: char, bold: bool, italic: bool) -> f32 {
        let mut cache = self.cache.borrow_mut();
        let key = (bold, italic, character);
        if let Some(value) = cache.get(&key) {
            return *value;
        }
        let bytes = self.set.bytes(self.serif, bold, italic);
        let value = ttf_parser::Face::parse(bytes, 0)
            .ok()
            .and_then(|face| {
                let glyph = face.glyph_index(character)?;
                let advance = face.glyph_hor_advance(glyph)?;
                Some(f32::from(advance) / f32::from(face.units_per_em()))
            })
            .unwrap_or(0.5);
        cache.insert(key, value);
        value
    }
}

impl Measure for Metrics<'_> {
    fn width(&self, text: &str, size: f32, bold: bool, italic: bool) -> f32 {
        text.chars()
            .map(|character| self.advance(character, bold, italic))
            .sum::<f32>()
            * size
    }
}
