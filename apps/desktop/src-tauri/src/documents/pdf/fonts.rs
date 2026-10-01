//! Cyrillic-capable fonts for translated text and their metrics.
//!
//! The export path uses bundled Noto Sans and Noto Serif faces so it does not
//! depend on the fonts installed on the host machine.

use crate::domain::RuntimeError;
use std::cell::RefCell;
use std::collections::HashMap;

use super::layout::Measure;

type Key = (bool, bool, bool);

pub struct FontSet {
    data: HashMap<Key, &'static [u8]>,
}

fn font_bytes(serif: bool, bold: bool, italic: bool) -> &'static [u8] {
    match (serif, bold, italic) {
        (true, false, false) => include_bytes!("../../../resources/fonts/NotoSerif-Regular.ttf"),
        (true, true, false) => include_bytes!("../../../resources/fonts/NotoSerif-Bold.ttf"),
        (true, false, true) => include_bytes!("../../../resources/fonts/NotoSerif-Italic.ttf"),
        (true, true, true) => {
            include_bytes!("../../../resources/fonts/NotoSerif-BoldItalic.ttf")
        }
        (false, false, false) => include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf"),
        (false, true, false) => include_bytes!("../../../resources/fonts/NotoSans-Bold.ttf"),
        (false, false, true) => include_bytes!("../../../resources/fonts/NotoSans-Italic.ttf"),
        (false, true, true) => {
            include_bytes!("../../../resources/fonts/NotoSans-BoldItalic.ttf")
        }
    }
}

impl FontSet {
    pub fn load() -> Result<Self, RuntimeError> {
        let mut data = HashMap::new();
        for serif in [true, false] {
            for bold in [false, true] {
                for italic in [false, true] {
                    data.insert((serif, bold, italic), font_bytes(serif, bold, italic));
                }
            }
        }
        Ok(Self { data })
    }

    pub fn bytes(&self, serif: bool, bold: bool, italic: bool) -> &[u8] {
        self.data[&(serif, bold, italic)]
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

#[cfg(test)]
mod tests {
    use super::FontSet;

    #[test]
    fn bundled_fonts_cover_latin_and_cyrillic() {
        let fonts = FontSet::load().expect("bundled fonts");
        for serif in [true, false] {
            for bold in [false, true] {
                for italic in [false, true] {
                    let face = ttf_parser::Face::parse(fonts.bytes(serif, bold, italic), 0)
                        .expect("valid TTF");
                    assert!(face.glyph_index('A').is_some());
                    assert!(face.glyph_index('Я').is_some());
                    assert!(face.glyph_index('ё').is_some());
                }
            }
        }
    }
}
