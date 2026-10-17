//! The badge that opens a passage's sutta on SuttaCentral: the passage's
//! reference as a small link chip outlined in gold ([`Variant::Accent`]),
//! beside the passage, in the language of the page
//! ([`learn_core::suttacentral::link`] picks the text).

use learn_core::deck::Example;
use learn_core::suttacentral::link;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::button::classes;
use palicanon_ui_lib::{Language, Size, Variant};

use crate::catalogue::Locale;
use crate::learn::window::source_ref;

/// The link chip of an example, or nothing when its source has no uid on
/// SuttaCentral. A new tab, `noopener`; its title says when the text opens
/// in English because SuttaCentral has none in the page's language.
pub(crate) fn passage_badge(example: &Example, language: Language) -> Option<AnyView> {
    let link = link(example, language.code())?;
    let locale = Locale::from(language);
    let title = if link.english {
        td_string!(locale, suttacentral_read_english)
    } else {
        td_string!(locale, suttacentral_read)
    };
    let look = classes(Variant::Accent, Size::Sm, "");
    let view = view! {
        <a href=link.url target="_blank" rel="noopener" title=title class=look>
            {source_ref(&example.source)}
        </a>
    };
    Some(view.into_any())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use learn_core::deck::Translation;

    use super::*;

    fn example(source: &str, translations: &[(&str, &str)]) -> Example {
        let translations: BTreeMap<String, Translation> = translations
            .iter()
            .map(|(lang, translator)| {
                let translation = Translation {
                    text: "text".to_string(),
                    translator: (*translator).to_string(),
                };
                ((*lang).to_string(), translation)
            })
            .collect();
        Example {
            text: "text".to_string(),
            source: source.to_string(),
            sutta: "sutta".to_string(),
            blank: None,
            translations,
        }
    }

    fn render(example: &Example, language: Language) -> String {
        Owner::new().with(|| {
            let badge = passage_badge(example, language);
            view! { <p>{badge}</p> }.to_html()
        })
    }

    #[test]
    fn the_badge_is_a_gold_outlined_link_to_the_sutta_in_a_new_tab() {
        let html = render(&example("DN16.36", &[("en", "sujato")]), Language::En);
        assert!(
            html.contains(r#"href="https://suttacentral.net/dn16/en/sujato""#),
            "{html}"
        );
        assert!(html.contains(r#"target="_blank""#), "{html}");
        assert!(html.contains(r#"rel="noopener""#), "{html}");
        assert!(html.contains(r#"title="Read on SuttaCentral""#), "{html}");
        assert!(html.contains(">DN 16.36</a>"), "{html}");
        assert!(
            html.contains("rounded-full") && html.contains("border-gold/40"),
            "the accent variant, outlined in gold: {html}"
        );
        assert!(!html.contains("bg-chip"), "not the plain chip: {html}");
    }

    #[test]
    fn there_is_no_badge_for_a_source_without_a_uid() {
        for source in ["XX1", "", "SNP9999"] {
            let html = render(&example(source, &[("en", "sujato")]), Language::En);
            assert!(!html.contains("<a "), "{source:?}: {html}");
            assert!(!html.contains("suttacentral"), "{source:?}: {html}");
        }
    }

    #[test]
    fn the_link_follows_the_language_of_the_page() {
        let deck = [
            ("en", "sujato"),
            ("fr", "christelle"),
            ("vi", "phantuananh"),
        ];
        let sample = example("SN22.87", &deck);
        let fr = render(&sample, Language::Fr);
        assert!(
            fr.contains(r#"href="https://suttacentral.net/sn22.87/fr/christelle""#),
            "{fr}"
        );
        assert!(fr.contains(r#"title="Lire sur SuttaCentral""#), "{fr}");
        let vi = render(&sample, Language::Vi);
        assert!(
            vi.contains(r#"href="https://suttacentral.net/sn22.87/vi/minh_chau""#),
            "{vi}"
        );
        assert!(vi.contains(r#"title="Đọc trên SuttaCentral""#), "{vi}");
        let en = render(&sample, Language::En);
        assert!(en.contains("/sn22.87/en/sujato"), "{en}");
    }

    #[test]
    fn a_link_to_the_english_text_says_so_in_its_title() {
        let sample = example("AN6.61", &[("en", "sujato")]);
        let fr = render(&sample, Language::Fr);
        assert!(
            fr.contains(r#"href="https://suttacentral.net/an6.61/en/sujato""#),
            "{fr}"
        );
        assert!(
            fr.contains(r#"title="Lire sur SuttaCentral (en anglais)""#),
            "{fr}"
        );
        let vi = render(&sample, Language::Vi);
        assert!(
            vi.contains("/an6.61/vi/minh_chau"),
            "SuttaCentral has Vietnamese: {vi}"
        );
        let sample = example("SNP4", &[("en", "sujato")]);
        let vi = render(&sample, Language::Vi);
        assert!(
            vi.contains(r#"title="Đọc trên SuttaCentral (tiếng Anh)""#),
            "{vi}"
        );
        let en = render(&sample, Language::En);
        assert!(en.contains(r#"title="Read on SuttaCentral""#), "{en}");
    }
}
