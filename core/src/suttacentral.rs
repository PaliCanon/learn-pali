//! The link from a deck example to the sutta on SuttaCentral, in the
//! reader's language.
//!
//! The deck's `source` is DPD style (`AN6.61`, `DN16.36`, `DHP123`,
//! `SNP40`). [`uid`] turns it into SuttaCentral's uid, [`link`] into the
//! address of the text in a language, and both are pure: nothing is asked
//! of SuttaCentral at run time. The facts come from
//! `core/fixtures/suttacentral.tsv`, checked once against SuttaCentral's
//! API for every source of the bundled deck (the file's header says how):
//! the canonical uid, and the translators SuttaCentral holds in English,
//! French and Vietnamese. A source the file does not know falls back to
//! rules for the four nikāyas (`AN6.61` is `an6.61`, `DN16.36` is `dn16`,
//! `MN2.2` is `mn2`) and for the Dhammapada's chapters; DPD's running
//! numbers (Snp, Thag, Thig, Ud) cannot be derived and give `None`.
//!
//! **Which translation.** In the reader's language: the deck's translator
//! when SuttaCentral holds that translator for the sutta, else the first
//! translator SuttaCentral holds in that language (its segmented editions
//! first, then by author uid, the order of the file), else the English one
//! (the deck's, else the first, else `sujato`), flagged by
//! [`Link::english`] so that the badge says the text is in English.

use crate::deck::Example;

/// Where the text of an example is read on SuttaCentral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `https://suttacentral.net/<uid>/<language>/<translator>`.
    pub url: String,
    /// The language of the text it opens: `en`, `fr` or `vi`.
    pub language: &'static str,
    /// The reader asked for French or Vietnamese and SuttaCentral has none
    /// of this sutta: the link opens the English text.
    pub english: bool,
}

/// The checked table (`core/fixtures/suttacentral.tsv`), read at compile time.
const TABLE: &str = include_str!("../fixtures/suttacentral.tsv");

/// The English translator when nothing better is known: the edition of
/// most of the canon on SuttaCentral.
const DEFAULT_TRANSLATOR: &str = "sujato";

/// The languages a link can open, in the order of the table's columns.
const LANGUAGES: [&str; 3] = ["en", "fr", "vi"];

/// SuttaCentral's chapters of the Dhammapada: the first and last verse.
const DHP_CHAPTERS: [(u32, u32); 26] = [
    (1, 20),
    (21, 32),
    (33, 43),
    (44, 59),
    (60, 75),
    (76, 89),
    (90, 99),
    (100, 115),
    (116, 128),
    (129, 145),
    (146, 156),
    (157, 166),
    (167, 178),
    (179, 196),
    (197, 208),
    (209, 220),
    (221, 234),
    (235, 255),
    (256, 272),
    (273, 289),
    (290, 305),
    (306, 319),
    (320, 333),
    (334, 359),
    (360, 382),
    (383, 423),
];

/// One line of the table: a source, its uid, and the translators
/// SuttaCentral holds per language (index of [`LANGUAGES`]).
struct Row {
    uid: &'static str,
    translators: [Vec<&'static str>; 3],
}

/// A source as the table keys it: lower case, no spaces.
fn key(source: &str) -> String {
    source
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The table's line for a source, when it has one.
fn row(source: &str) -> Option<Row> {
    let wanted = key(source);
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let cells: Vec<&'static str> = line.split('\t').collect();
            if cells.len() < 6 || cells[0] != wanted {
                return None;
            }
            let names = |cell: &'static str| -> Vec<&'static str> {
                cell.split(',').filter(|name| *name != "-").collect()
            };
            Some(Row {
                uid: cells[1],
                translators: [names(cells[3]), names(cells[4]), names(cells[5])],
            })
        })
}

/// All digits and not empty.
fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// The uid the rules give a source, with no table: the nikāyas and the
/// Dhammapada's chapters.
fn rule_uid(source: &str) -> Option<String> {
    let key = key(source);
    let (code, number) = key.split_at(key.find(|c: char| c.is_ascii_digit())?);
    match code {
        "mn" | "dn" => {
            let main = number.split('.').next()?;
            digits(main).then(|| format!("{code}{main}"))
        }
        "an" | "sn" => {
            let (group, sutta) = number.split_once('.')?;
            (digits(group) && digits(sutta)).then(|| format!("{code}{number}"))
        }
        "dhp" => {
            let verse: u32 = number.parse().ok()?;
            DHP_CHAPTERS
                .iter()
                .find(|(first, last)| (*first..=*last).contains(&verse))
                .map(|(first, last)| format!("dhp{first}-{last}"))
        }
        _ => None,
    }
}

/// SuttaCentral's uid of a deck source (`DN16.36` is `dn16`, `SNP40` is
/// `snp4.2`), or `None` when the source is not a known one.
#[must_use]
pub fn uid(source: &str) -> Option<String> {
    row(source)
        .map(|row| row.uid.to_string())
        .or_else(|| rule_uid(source))
}

/// A translator id that can stand in an address.
fn is_author(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The translator to open in `language` (a column of [`LANGUAGES`]): the
/// deck's when SuttaCentral holds it (or the sutta is not in the table),
/// else the first SuttaCentral holds, else nothing.
fn translator(example: &Example, row: Option<&Row>, language: usize) -> Option<String> {
    let deck = example
        .translations
        .get(LANGUAGES[language])
        .map(|translation| translation.translator.as_str())
        .filter(|id| is_author(id));
    let held = row.map(|row| &row.translators[language]);
    match (deck, held) {
        (Some(id), Some(held)) if held.contains(&id) => Some(id.to_string()),
        (Some(id), None) => Some(id.to_string()),
        (_, Some(held)) => held.first().map(ToString::to_string),
        (None, None) => None,
    }
}

/// The link of an example's sutta in `language` (`en`, `fr`, `vi`; any
/// other code reads as English), or `None` when its source has no uid.
#[must_use]
pub fn link(example: &Example, language: &str) -> Option<Link> {
    let row = row(&example.source);
    let uid = row
        .as_ref()
        .map(|row| row.uid.to_string())
        .or_else(|| rule_uid(&example.source))?;
    let asked = LANGUAGES
        .iter()
        .position(|code| *code == language)
        .unwrap_or(0);
    let own = if asked == 0 {
        None
    } else {
        translator(example, row.as_ref(), asked)
    };
    let (index, translator) = if let Some(translator) = own {
        (asked, translator)
    } else {
        let english = translator(example, row.as_ref(), 0);
        (0, english.unwrap_or_else(|| DEFAULT_TRANSLATOR.to_string()))
    };
    Some(Link {
        url: format!(
            "https://suttacentral.net/{uid}/{}/{translator}",
            LANGUAGES[index]
        ),
        language: LANGUAGES[index],
        english: asked != 0 && index == 0,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::deck::{Deck, Translation};

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

    fn url_of(source: &str, translations: &[(&str, &str)], language: &str) -> Option<String> {
        link(&example(source, translations), language).map(|link| link.url)
    }

    fn bundled_deck() -> Deck {
        let json = include_str!("../fixtures/deck.json");
        Deck::from_json(json).expect("the bundled deck parses")
    }

    #[test]
    fn nikaya_sources_keep_their_numbers_and_drop_a_dn_or_mn_section() {
        for (source, expected) in [
            ("AN6.61", "an6.61"),
            ("an6.61", "an6.61"),
            ("SN22.87", "sn22.87"),
            ("MN 10", "mn10"),
            ("MN2.2", "mn2"),
            ("DN16.36", "dn16"),
            ("DN 22.3", "dn22"),
        ] {
            assert_eq!(uid(source).as_deref(), Some(expected), "{source}");
        }
    }

    #[test]
    fn a_dhammapada_verse_gives_the_chapter_holding_it() {
        for (source, expected) in [
            ("DHP1", "dhp1-20"),
            ("DHP20", "dhp1-20"),
            ("DHP21", "dhp21-32"),
            ("DHP123", "dhp116-128"),
            ("DHP423", "dhp383-423"),
        ] {
            assert_eq!(uid(source).as_deref(), Some(expected), "{source}");
        }
        assert_eq!(uid("DHP424"), None);
        assert_eq!(uid("DHP0"), None);
    }

    #[test]
    fn dpd_running_numbers_come_from_the_checked_table() {
        for (source, expected) in [
            ("SNP4", "snp1.4"),
            ("SNP40", "snp4.2"),
            ("SNP68", "snp5.14"),
            ("TH155", "thag2.35"),
            ("THI71", "thig14.1"),
            ("UD45", "ud5.5"),
            ("AN2.135", "an2.130-140"),
        ] {
            assert_eq!(uid(source).as_deref(), Some(expected), "{source}");
        }
    }

    #[test]
    fn an_unknown_source_has_no_uid() {
        for source in ["", "XX1", "SNP9999", "TH", "SN22", "AN", "DN", "MN x"] {
            assert_eq!(uid(source), None, "{source:?}");
        }
    }

    #[test]
    fn every_source_of_the_bundled_deck_maps_to_a_checked_uid() {
        let deck = bundled_deck();
        let mut seen = 0;
        for term in &deck.terms {
            let Some(example) = &term.example else {
                continue;
            };
            let uid = uid(&example.source).unwrap_or_else(|| panic!("{}", example.source));
            assert!(
                checked_uids().contains(&uid),
                "{}: {uid} is not in the checked table",
                example.source
            );
            seen += 1;
        }
        assert_eq!(seen, 50);
    }

    /// The uids the table lists, read from the file by the test itself.
    fn checked_uids() -> Vec<String> {
        include_str!("../fixtures/suttacentral.tsv")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split('\t').nth(1))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_rules_agree_with_the_table_but_for_a_range() {
        let table = include_str!("../fixtures/suttacentral.tsv");
        let mut differing = Vec::new();
        for line in table.lines().filter(|line| !line.starts_with('#')) {
            let mut cells = line.split('\t');
            let (source, uid) = (cells.next().unwrap(), cells.next().unwrap());
            if let Some(ruled) = super::rule_uid(source)
                && ruled != uid
            {
                differing.push(source.to_string());
            }
        }
        assert_eq!(differing, vec!["an2.135".to_string()]);
    }

    #[test]
    fn english_links_to_the_deck_translator() {
        assert_eq!(
            url_of("SN22.87", &[("en", "sujato")], "en").as_deref(),
            Some("https://suttacentral.net/sn22.87/en/sujato")
        );
        let found = link(&example("DN16.36", &[("en", "sujato")]), "en").unwrap();
        assert_eq!((found.language, found.english), ("en", false));
    }

    #[test]
    fn english_without_a_deck_translation_is_the_first_one_of_the_table() {
        assert_eq!(
            url_of("MN10", &[], "en").as_deref(),
            Some("https://suttacentral.net/mn10/en/sujato")
        );
        assert_eq!(
            url_of("XX1", &[], "en"),
            None,
            "an unknown source has no link"
        );
    }

    #[test]
    fn the_deck_translation_in_the_language_wins_when_suttacentral_has_it() {
        let deck = [("en", "sujato"), ("fr", "christelle")];
        assert_eq!(
            url_of("SN22.87", &deck, "fr").as_deref(),
            Some("https://suttacentral.net/sn22.87/fr/christelle")
        );
        let deck = [("en", "sujato"), ("vi", "phantuananh")];
        assert_eq!(
            url_of("DHP123", &deck, "vi").as_deref(),
            Some("https://suttacentral.net/dhp116-128/vi/phantuananh")
        );
        let found = link(&example("SN22.87", &deck), "vi").unwrap();
        assert_eq!((found.language, found.english), ("vi", false));
    }

    #[test]
    fn a_deck_translator_suttacentral_lacks_gives_the_one_it_has() {
        let deck = [("en", "sujato"), ("fr", "christelle")];
        assert_eq!(
            url_of("DN22.3", &deck, "fr").as_deref(),
            Some("https://suttacentral.net/dn22/fr/canonpaliorg")
        );
    }

    #[test]
    fn suttacentral_own_translation_is_used_when_the_deck_has_none() {
        assert_eq!(
            url_of("AN6.61", &[("en", "sujato")], "vi").as_deref(),
            Some("https://suttacentral.net/an6.61/vi/minh_chau")
        );
        assert_eq!(
            url_of("MN1", &[], "fr").as_deref(),
            Some("https://suttacentral.net/mn1/fr/maes")
        );
    }

    #[test]
    fn with_no_translation_in_the_language_the_english_one_opens_and_says_so() {
        let found = link(&example("AN6.61", &[("en", "sujato")]), "fr").unwrap();
        assert_eq!(found.url, "https://suttacentral.net/an6.61/en/sujato");
        assert_eq!((found.language, found.english), ("en", true));
        let found = link_of_vi("SNP4");
        assert_eq!(found.url, "https://suttacentral.net/snp1.4/en/sujato");
        assert!(found.english);
    }

    fn link_of_vi(source: &str) -> Link {
        link(&example(source, &[("en", "sujato")]), "vi").unwrap()
    }

    #[test]
    fn the_fallback_english_prefers_the_deck_translator_the_table_holds() {
        // SNP40 holds sujato and mills in English; the deck says mills.
        let found = link(&example("SNP40", &[("en", "mills")]), "vi").unwrap();
        assert_eq!(found.url, "https://suttacentral.net/snp4.2/en/mills");
        // The deck's English translator is not on SuttaCentral: the first.
        let found = link(&example("SNP40", &[("en", "nobody")]), "vi").unwrap();
        assert_eq!(found.url, "https://suttacentral.net/snp4.2/en/sujato");
    }

    #[test]
    fn another_language_code_reads_as_english() {
        assert_eq!(
            url_of("SN22.87", &[("en", "sujato")], "de").as_deref(),
            Some("https://suttacentral.net/sn22.87/en/sujato")
        );
        let found = link(&example("SN22.87", &[]), "de").unwrap();
        assert!(!found.english);
    }

    #[test]
    fn a_source_the_table_lacks_trusts_the_deck_translator() {
        assert_eq!(
            url_of("AN6.62", &[("en", "sujato"), ("vi", "phantuananh")], "vi").as_deref(),
            Some("https://suttacentral.net/an6.62/vi/phantuananh")
        );
        assert_eq!(
            url_of("AN6.62", &[("en", "sujato")], "vi").as_deref(),
            Some("https://suttacentral.net/an6.62/en/sujato")
        );
    }

    #[test]
    fn a_translator_that_is_no_author_id_is_not_put_in_an_address() {
        assert_eq!(
            url_of("AN6.62", &[("en", "a/b?c")], "en").as_deref(),
            Some("https://suttacentral.net/an6.62/en/sujato")
        );
    }
}
