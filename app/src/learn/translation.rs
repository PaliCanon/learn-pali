//! The example's translation as the reveal shows it: the one in the page's
//! language, English when that language has none, nothing when the deck
//! has none ([`translation_for`]); the translator by name
//! ([`translator_name`]), from a small table of the API's translator ids
//! (an unknown id shows itself).

use learn_core::deck::{Example, Translation};
use palicanon_ui_lib::Language;

/// The API's translator ids and the names shown.
const TRANSLATORS: &[(&str, &str)] = &[
    ("christelle", "Christelle"),
    ("minh_chau", "Thích Minh Châu"),
    ("ms", "Mahāsaṅgīti"),
    ("phantuananh", "Phan Tuấn Anh"),
    ("sujato", "Bhikkhu Sujato"),
];

/// The translator's name, or the id itself when the table lacks it.
pub(crate) fn translator_name(id: &str) -> &str {
    TRANSLATORS
        .iter()
        .find(|(known, _)| *known == id)
        .map_or(id, |(_, name)| name)
}

/// The translation to show for `language`: its own, else English, else
/// none.
pub(crate) fn translation_for(example: &Example, language: Language) -> Option<&Translation> {
    let translations = &example.translations;
    translations
        .get(language.code())
        .or_else(|| translations.get(Language::En.code()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn example(langs: &[(&str, &str, &str)]) -> Example {
        let translations: BTreeMap<String, Translation> = langs
            .iter()
            .map(|(lang, text, translator)| {
                let translation = Translation {
                    text: (*text).to_string(),
                    translator: (*translator).to_string(),
                };
                ((*lang).to_string(), translation)
            })
            .collect();
        Example {
            text: "alaṃ, vakkali".to_string(),
            source: "SN22.87".to_string(),
            sutta: "vakkalisuttaṃ".to_string(),
            blank: None,
            translations,
        }
    }

    fn shown(example: &Example, language: Language) -> Option<(&str, &str)> {
        translation_for(example, language).map(|t| (t.text.as_str(), t.translator.as_str()))
    }

    #[test]
    fn the_page_language_is_shown_when_the_example_has_it() {
        let both = example(&[
            ("en", "Enough, Vakkali!", "sujato"),
            ("fr", "Assez, Vakkali !", "christelle"),
        ]);
        assert_eq!(
            shown(&both, Language::En),
            Some(("Enough, Vakkali!", "sujato"))
        );
        assert_eq!(
            shown(&both, Language::Fr),
            Some(("Assez, Vakkali !", "christelle"))
        );
    }

    #[test]
    fn a_missing_language_falls_back_to_english() {
        let both = example(&[
            ("en", "Enough, Vakkali!", "sujato"),
            ("fr", "Assez, Vakkali !", "christelle"),
        ]);
        assert_eq!(
            shown(&both, Language::Vi),
            Some(("Enough, Vakkali!", "sujato"))
        );
    }

    #[test]
    fn no_translation_or_no_english_to_fall_back_on_shows_nothing() {
        assert_eq!(shown(&example(&[]), Language::En), None);
        assert_eq!(shown(&example(&[]), Language::Fr), None);
        let vi_only = example(&[("vi", "Thôi đủ rồi", "phantuananh")]);
        assert_eq!(shown(&vi_only, Language::Fr), None);
        assert_eq!(
            shown(&vi_only, Language::Vi),
            Some(("Thôi đủ rồi", "phantuananh"))
        );
    }

    #[test]
    fn translators_are_shown_by_name_and_an_unknown_id_as_itself() {
        assert_eq!(translator_name("sujato"), "Bhikkhu Sujato");
        assert_eq!(translator_name("christelle"), "Christelle");
        assert_eq!(translator_name("phantuananh"), "Phan Tuấn Anh");
        assert_eq!(translator_name("minh_chau"), "Thích Minh Châu");
        assert_eq!(translator_name("ms"), "Mahāsaṅgīti");
        assert_eq!(translator_name("hu"), "hu");
        assert_eq!(translator_name(""), "");
    }
}
