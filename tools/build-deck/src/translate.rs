//! The deck's terms get their examples' translations from the PaliCanon
//! API. The passage itself (the source as a uid, the Pāli text matched, each
//! language's line) lives in `learn_passage::translate` since 062 and is
//! re-exported here; this module keeps `translate_terms`, which walks the
//! terms and counts into the build's report.

use learn_core::deck::Term;

pub use learn_passage::translate::{Found, LANGS, Why, translate_example};

use crate::api::Fetch;
use crate::build::Report;

/// The languages looked for, in the deck's order.
/// Gives every term's example its translations; the counts per language
/// and a note per skipped example or language go to `report`.
pub fn translate_terms(terms: &mut [Term], fetch: &mut dyn Fetch, report: &mut Report) {
    for lang in LANGS {
        report.translated.entry(lang.to_string()).or_default();
    }
    for term in terms.iter_mut() {
        let Some(example) = term.example.as_mut() else {
            continue;
        };
        match translate_example(example, fetch) {
            Ok(found) => {
                for lang in found.translations.keys() {
                    *report.translated.entry(lang.clone()).or_default() += 1;
                }
                let id = &term.id;
                let notes = found.notes.iter();
                report
                    .notes
                    .extend(notes.map(|note| format!("{id}: translation {note}")));
                example.translations = found.translations;
            }
            Err(reason) => report
                .notes
                .push(format!("{}: no translation: {reason}", term.id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::api::Cached;
    use crate::api::fixture::Recorded;
    use learn_core::deck::Example;

    fn example(source: &str, text: &str) -> Example {
        Example {
            text: text.to_string(),
            source: source.to_string(),
            sutta: String::new(),
            blank: None,
            translations: BTreeMap::new(),
        }
    }

    fn vakkali() -> Example {
        example(
            "SN22.87",
            "alaṃ, vakkali, kiṃ te iminā pūtikāyena diṭṭhena? yo kho, vakkali, dhammaṃ \
             passati so maṃ passati, yo maṃ passati so dhammaṃ passati.",
        )
    }

    fn dhp127() -> Example {
        example(
            "DHP127",
            "na antalikkhe na samuddamajjhe,\nna pabbatānaṃ vivaraṃ pavissa,\n\
             na vijjati so jagatippadeso,\nyatthaṭṭhito mucceyya pāpakammā.",
        )
    }

    fn by(found: &Found, lang: &str) -> Option<(String, String)> {
        found
            .translations
            .get(lang)
            .map(|t| (t.text.clone(), t.translator.clone()))
    }

    #[test]
    fn the_vakkali_example_gets_english_and_french_not_the_vietnamese_paragraphs() {
        let mut fetch = Recorded::default();
        let found = translate_example(&vakkali(), &mut fetch).unwrap();
        assert_eq!(
            by(&found, "en"),
            Some((
                "Enough, Vakkali! Why would you want to see this rotting body? One who sees \
                 the teaching sees me. One who sees me sees the teaching."
                    .to_string(),
                "sujato".to_string()
            ))
        );
        let (fr, translator) = by(&found, "fr").unwrap();
        assert!(fr.starts_with("Assez, Vakkali"), "{fr}");
        assert_eq!(translator, "christelle");
        assert_eq!(by(&found, "vi"), None);
        assert_eq!(found.notes.len(), 1, "{:?}", found.notes);
        assert!(found.notes[0].starts_with("vi: "), "{:?}", found.notes);
        assert!(found.notes[0].contains("minh_chau"), "{:?}", found.notes);
    }

    #[test]
    fn each_text_is_asked_once() {
        let mut fetch = Recorded::default();
        translate_example(&vakkali(), &mut fetch).unwrap();
        assert_eq!(
            fetch.asked,
            vec![
                "/v1/sutta/sn22.87",
                "/v1/sutta/sn22.87/pli",
                "/v1/sutta/sn22.87/en",
                "/v1/sutta/sn22.87/fr",
                "/v1/sutta/sn22.87/vi",
            ]
        );
    }

    #[test]
    fn a_dhammapada_verse_is_read_in_its_chapter_range() {
        let mut fetch = Recorded::default();
        let found = translate_example(&dhp127(), &mut fetch).unwrap();
        let (en, translator) = by(&found, "en").unwrap();
        assert!(en.starts_with("Not in midair, nor mid-ocean,"), "{en}");
        assert!(en.ends_with("to escape your wicked deeds."), "{en}");
        assert_eq!(translator, "sujato");
        assert_eq!(by(&found, "vi").unwrap().1, "phantuananh");
        assert_eq!(by(&found, "fr"), None, "the French is keyed sc127");
        assert!(
            fetch.asked[0] == "/v1/sutta/dhp116-128",
            "{:?}",
            fetch.asked
        );
    }

    #[test]
    fn an_unmapped_source_is_skipped_without_a_call() {
        let mut fetch = Recorded::default();
        let reason = translate_example(&example("KHP5", "x"), &mut fetch)
            .unwrap_err()
            .to_string();
        assert!(reason.contains("KHP5"), "{reason}");
        assert_eq!(fetch.asked.len(), 0);
    }

    #[test]
    fn a_sutta_the_api_does_not_have_is_skipped() {
        let mut fetch = Recorded::default();
        let reason = translate_example(&example("SN1.1", "x"), &mut fetch)
            .unwrap_err()
            .to_string();
        assert!(
            reason.contains("sn1.1") && reason.contains("not found"),
            "{reason}"
        );
    }

    #[test]
    fn a_refused_call_is_skipped_with_its_status() {
        let mut fetch = Recorded {
            refuse: Some(401),
            ..Recorded::default()
        };
        let reason = translate_example(&vakkali(), &mut fetch)
            .unwrap_err()
            .to_string();
        assert!(reason.contains("HTTP 401"), "{reason}");
    }

    #[test]
    fn an_example_not_in_the_text_is_skipped() {
        let mut fetch = Recorded::default();
        let other = example("SN22.87", "sabbe saṅkhārā aniccā'ti, yadā paññāya passati");
        let reason = translate_example(&other, &mut fetch)
            .unwrap_err()
            .to_string();
        assert!(reason.contains("not found in sn22.87"), "{reason}");
    }

    fn term_with(id: &str, example: Option<Example>) -> Term {
        Term {
            id: learn_core::deck::TermId(id.to_string()),
            lemma: id.to_string(),
            pos: learn_core::deck::Pos("ind".to_string()),
            meaning: learn_core::deck::Meaning {
                en: "x".to_string(),
                fr: None,
                vi: None,
                literal: None,
            },
            construction: None,
            root: None,
            example,
            forms: vec![],
            rank: 1,
        }
    }

    #[test]
    fn the_terms_get_their_translations_and_the_report_counts_them() {
        let mut terms = vec![
            term_with("yo", Some(vakkali())),
            term_with("na 1", Some(dhp127())),
            term_with("kho", Some(example("KHP5", "x"))),
            term_with("lone", None),
            term_with("so", Some(vakkali())),
        ];
        let mut fetch = Cached::new(Recorded::default(), None);
        let mut report = Report::default();
        translate_terms(&mut terms, &mut fetch, &mut report);
        let langs = |term: &Term| -> Vec<String> {
            term.example
                .as_ref()
                .map(|e| e.translations.keys().cloned().collect())
                .unwrap_or_default()
        };
        assert_eq!(langs(&terms[0]), vec!["en", "fr"]);
        assert_eq!(langs(&terms[1]), vec!["en", "vi"]);
        assert_eq!(langs(&terms[2]).len(), 0);
        assert_eq!(langs(&terms[4]), vec!["en", "fr"]);
        let counts: Vec<(&str, usize)> = report
            .translated
            .iter()
            .map(|(lang, n)| (lang.as_str(), *n))
            .collect();
        assert_eq!(counts, vec![("en", 3), ("fr", 2), ("vi", 1)]);
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.starts_with("kho: no translation: ") && n.contains("KHP5")),
            "{:?}",
            report.notes
        );
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.starts_with("yo: translation vi: ")),
            "{:?}",
            report.notes
        );
        assert_eq!(report.warnings.len(), 0);
    }
}
