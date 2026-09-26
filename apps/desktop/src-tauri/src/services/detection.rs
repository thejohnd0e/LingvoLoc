use crate::domain::{DetectedLanguage, RuntimeError};
use whatlang::{detect, Lang};

pub fn detect_supported_language(text: &str) -> Result<DetectedLanguage, RuntimeError> {
    if text.trim().is_empty() {
        return Err(RuntimeError::InvalidInput("text must not be empty".into()));
    }
    if let Some(code) = detect_cyrillic_language(text) {
        return Ok(DetectedLanguage {
            code: code.into(),
            confidence: 1.0,
        });
    }
    let info = detect(text)
        .ok_or_else(|| RuntimeError::InvalidInput("language could not be detected".into()))?;
    let code = match info.lang() {
        Lang::Eng => "en",
        Lang::Rus => "ru",
        Lang::Deu => "de",
        Lang::Spa => "es",
        Lang::Fra => "fr",
        Lang::Ita => "it",
        Lang::Por => "pt",
        Lang::Pol => "pl",
        Lang::Ukr => "uk",
        _ => {
            return Err(RuntimeError::InvalidInput(format!(
                "detected language is not supported: {:?}",
                info.lang()
            )))
        }
    };
    Ok(DetectedLanguage {
        code: code.into(),
        confidence: info.confidence(),
    })
}

fn detect_cyrillic_language(text: &str) -> Option<&'static str> {
    let cyrillic_letters = text
        .chars()
        .filter(|character| {
            ('А'..='я').contains(character) || *character == 'Ё' || *character == 'ё'
        })
        .count();
    if cyrillic_letters < 2 {
        return None;
    }
    let ukrainian_marker = text.chars().any(|character| "ієїґІЄЇҐ".contains(character));
    Some(if ukrainian_marker { "uk" } else { "ru" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_languages_locally() {
        assert_eq!(
            detect_supported_language("This is a local translation tool")
                .unwrap()
                .code,
            "en"
        );
        assert_eq!(
            detect_supported_language("Это локальный инструмент перевода")
                .unwrap()
                .code,
            "ru"
        );
        assert_eq!(
            detect_supported_language("На вид хорошая. Плотная. Красивая, но еще не пробовала.")
                .unwrap()
                .code,
            "ru"
        );
        assert_eq!(
            detect_supported_language("Dies ist ein lokales Übersetzungstool")
                .unwrap()
                .code,
            "de"
        );
        assert_eq!(
            detect_supported_language("Esta es una herramienta de traducción local")
                .unwrap()
                .code,
            "es"
        );
        assert_eq!(
            detect_supported_language("Ceci est un outil de traduction local")
                .unwrap()
                .code,
            "fr"
        );
        assert_eq!(
            detect_supported_language("Це локальний інструмент перекладу")
                .unwrap()
                .code,
            "uk"
        );
    }

    #[test]
    fn rejects_empty_text() {
        assert!(detect_supported_language("  ").is_err());
    }
}
