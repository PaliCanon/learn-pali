//! Codes in words: a part of speech (`pos_words`), a form label
//! (`label_words`), a date (`date_words`), in the page's language. The
//! tables live here, not in the catalogue (the design's "a table"); an
//! unknown code shows itself, never an empty string.

use learn_core::date::Date;
use palicanon_ui_lib::Language;

/// The words of one code in the three interface languages: English,
/// French, Vietnamese.
type Words = [&'static str; 3];

/// Parts of speech (DPD codes): the noun genders, the verb tenses and
/// moods, the participles, the particles.
const POS: &[(&str, Words)] = &[
    ("ind", ["indeclinable", "indéclinable", "bất biến từ"]),
    (
        "masc",
        ["masculine noun", "nom masculin", "danh từ giống đực"],
    ),
    ("fem", ["feminine noun", "nom féminin", "danh từ giống cái"]),
    ("nt", ["neuter noun", "nom neutre", "danh từ trung tính"]),
    ("adj", ["adjective", "adjectif", "tính từ"]),
    ("pron", ["pronoun", "pronom", "đại từ"]),
    ("card", ["cardinal number", "nombre cardinal", "số đếm"]),
    (
        "pr",
        ["verb (present)", "verbe (présent)", "động từ (hiện tại)"],
    ),
    (
        "imperf",
        [
            "verb (imperfect)",
            "verbe (imparfait)",
            "động từ (chưa hoàn thành)",
        ],
    ),
    (
        "aor",
        ["verb (aorist)", "verbe (aoriste)", "động từ (aorist)"],
    ),
    (
        "perf",
        ["verb (perfect)", "verbe (parfait)", "động từ (hoàn thành)"],
    ),
    (
        "fut",
        ["verb (future)", "verbe (futur)", "động từ (tương lai)"],
    ),
    (
        "imp",
        [
            "verb (imperative)",
            "verbe (impératif)",
            "động từ (mệnh lệnh)",
        ],
    ),
    (
        "opt",
        ["verb (optative)", "verbe (optatif)", "động từ (cầu khiến)"],
    ),
    (
        "cond",
        [
            "verb (conditional)",
            "verbe (conditionnel)",
            "động từ (điều kiện)",
        ],
    ),
    (
        "pp",
        ["past participle", "participe passé", "quá khứ phân từ"],
    ),
    (
        "prp",
        [
            "present participle",
            "participe présent",
            "hiện tại phân từ",
        ],
    ),
    (
        "ptp",
        [
            "potential participle",
            "participe potentiel",
            "tương lai bị động phân từ",
        ],
    ),
    (
        "abs",
        ["absolutive", "absolutif", "bất biến quá khứ phân từ"],
    ),
    ("ger", ["gerund", "gérondif", "danh động từ"]),
    ("inf", ["infinitive", "infinitif", "động từ nguyên mẫu"]),
    ("sandhi", ["sandhi", "sandhi", "sandhi"]),
];

/// The tokens of a form label (DPD): case, number, gender, person, tense
/// and mood, and the two words of "in comps".
const TOKENS: &[(&str, Words)] = &[
    ("nom", ["nominative", "nominatif", "chủ cách"]),
    ("acc", ["accusative", "accusatif", "đối cách"]),
    ("instr", ["instrumental", "instrumental", "cách công cụ"]),
    ("dat", ["dative", "datif", "cách tặng"]),
    ("abl", ["ablative", "ablatif", "cách xuất xứ"]),
    ("gen", ["genitive", "génitif", "sở hữu cách"]),
    ("loc", ["locative", "locatif", "vị trí cách"]),
    ("voc", ["vocative", "vocatif", "hô cách"]),
    ("sg", ["singular", "singulier", "số ít"]),
    ("pl", ["plural", "pluriel", "số nhiều"]),
    ("dual", ["dual", "duel", "số đôi"]),
    ("masc", ["masculine", "masculin", "giống đực"]),
    ("fem", ["feminine", "féminin", "giống cái"]),
    ("nt", ["neuter", "neutre", "trung tính"]),
    ("1st", ["1st person", "1re personne", "ngôi thứ nhất"]),
    ("2nd", ["2nd person", "2e personne", "ngôi thứ hai"]),
    ("3rd", ["3rd person", "3e personne", "ngôi thứ ba"]),
    ("pr", ["present", "présent", "hiện tại"]),
    ("imp", ["imperative", "impératif", "mệnh lệnh"]),
    ("opt", ["optative", "optatif", "cầu khiến"]),
    ("fut", ["future", "futur", "tương lai"]),
    ("aor", ["aorist", "aoriste", "aorist"]),
    ("imperf", ["imperfect", "imparfait", "chưa hoàn thành"]),
    ("cond", ["conditional", "conditionnel", "điều kiện"]),
    ("perf", ["perfect", "parfait", "hoàn thành"]),
    ("reflx", ["reflexive", "réfléchi", "phản thân"]),
    ("in", ["in", "en", "trong"]),
    ("comps", ["compounds", "composés", "từ ghép"]),
];

/// The words of `code` in `language` from `table`, if it has any.
fn lookup(
    table: &'static [(&'static str, Words)],
    language: Language,
    code: &str,
) -> Option<&'static str> {
    let column = match language {
        Language::En => 0,
        Language::Fr => 1,
        Language::Vi => 2,
    };
    table
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, words)| words[column])
}

/// The part of speech `pos` (a DPD code: `ind`, `fem`, `masc`…) in words,
/// e.g. "indeclinable", "feminine noun"; an unknown code is returned as is.
pub(crate) fn pos_words(language: Language, pos: &str) -> String {
    lookup(POS, language, pos).unwrap_or(pos).to_string()
}

/// The form label `label` (DPD tokens: `acc pl`, `3rd sg pr`…) in words,
/// token by token in the label's order (A6), e.g. "accusative plural"; an
/// unknown token is kept as is.
pub(crate) fn label_words(language: Language, label: &str) -> String {
    label
        .split(' ')
        .map(|token| lookup(TOKENS, language, token).unwrap_or(token))
        .collect::<Vec<_>>()
        .join(" ")
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const MONTHS_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// `date` in words (A10): `6 October 2026`, `6 octobre 2026` (`1er` for
/// the first), `ngày 6 tháng 10 năm 2026`.
pub(crate) fn date_words(language: Language, date: Date) -> String {
    let (year, month, day) = date.ymd();
    let index = usize::try_from(month.saturating_sub(1))
        .unwrap_or(0)
        .min(11);
    match language {
        Language::En => format!("{day} {} {year}", MONTHS_EN[index]),
        Language::Fr if day == 1 => format!("1er {} {year}", MONTHS_FR[index]),
        Language::Fr => format!("{day} {} {year}", MONTHS_FR[index]),
        Language::Vi => format!("ngày {day} tháng {month} năm {year}"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const LANGUAGES: [Language; 3] = [Language::En, Language::Fr, Language::Vi];

    fn day(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    /// Every part-of-speech code of the fixture deck.
    fn fixture_pos() -> BTreeSet<String> {
        crate::deck::deck()
            .terms
            .iter()
            .map(|term| term.pos.0.clone())
            .collect()
    }

    /// Every label token of the fixture deck's forms.
    fn fixture_tokens() -> BTreeSet<String> {
        crate::deck::deck()
            .terms
            .iter()
            .flat_map(|term| &term.forms)
            .flat_map(|form| form.label.split(' '))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_fixture_uses_the_codes_the_tests_cover() {
        // A guard: if the deck changes, the two tests below still collect
        // from it, and these sets are not empty.
        assert!(fixture_pos().len() >= 8, "{:?}", fixture_pos());
        assert!(fixture_tokens().len() >= 20, "{:?}", fixture_tokens());
    }

    #[test]
    fn every_pos_of_the_fixture_has_words_in_every_language() {
        for pos in fixture_pos() {
            let words: Vec<String> = LANGUAGES
                .iter()
                .map(|language| pos_words(*language, &pos))
                .collect();
            for (language, word) in LANGUAGES.iter().zip(&words) {
                assert_ne!(word, &pos, "{pos} has no words in {language:?}");
                assert_ne!(word, "");
            }
            assert_ne!(words[0], words[1], "{pos}: en = fr");
            assert_ne!(words[0], words[2], "{pos}: en = vi");
        }
    }

    #[test]
    fn every_label_token_of_the_fixture_has_words_in_every_language() {
        for token in fixture_tokens() {
            for language in LANGUAGES {
                let words = label_words(language, &token);
                assert_ne!(words, token, "{token} has no words in {language:?}");
            }
        }
    }

    #[test]
    fn the_pos_words_in_three_languages() {
        assert_eq!(pos_words(Language::En, "ind"), "indeclinable");
        assert_eq!(pos_words(Language::Fr, "ind"), "indéclinable");
        assert_eq!(pos_words(Language::Vi, "ind"), "bất biến từ");
        assert_eq!(pos_words(Language::En, "fem"), "feminine noun");
        assert_eq!(pos_words(Language::Fr, "fem"), "nom féminin");
        assert_eq!(pos_words(Language::Vi, "fem"), "danh từ giống cái");
        assert_eq!(pos_words(Language::En, "pr"), "verb (present)");
    }

    #[test]
    fn the_other_dpd_codes_the_deck_may_gain_have_words() {
        for pos in [
            "masc", "nt", "adj", "pr", "pp", "pron", "abs", "ger", "card", "sandhi", "aor", "prp",
            "ptp", "inf", "fut", "imp", "opt", "cond", "perf", "imperf",
        ] {
            for language in LANGUAGES {
                let words = pos_words(language, pos);
                assert_ne!(words, "");
                // `sandhi` is a loan in every language.
                if pos != "sandhi" {
                    assert_ne!(words, pos, "{pos} in {language:?}");
                }
            }
        }
    }

    #[test]
    fn an_unknown_pos_shows_itself() {
        for language in LANGUAGES {
            assert_eq!(pos_words(language, "zzz"), "zzz");
            assert_eq!(pos_words(language, ""), "");
        }
    }

    #[test]
    fn a_label_is_said_token_by_token_in_its_order() {
        assert_eq!(label_words(Language::En, "acc pl"), "accusative plural");
        assert_eq!(label_words(Language::Fr, "acc pl"), "accusatif pluriel");
        assert_eq!(label_words(Language::Vi, "acc pl"), "đối cách số nhiều");
        assert_eq!(
            label_words(Language::En, "fem acc pl"),
            "feminine accusative plural"
        );
        assert_eq!(
            label_words(Language::En, "reflx pr 3rd sg"),
            "reflexive present 3rd person singular"
        );
        assert_eq!(
            label_words(Language::Fr, "1st nom sg"),
            "1re personne nominatif singulier"
        );
    }

    #[test]
    fn the_tokens_of_the_design_all_have_words() {
        for token in [
            "nom", "acc", "instr", "dat", "abl", "gen", "loc", "voc", "sg", "pl", "dual", "masc",
            "fem", "nt", "1st", "2nd", "3rd", "pr", "imp", "opt", "fut", "aor", "imperf", "cond",
            "perf", "reflx", "in", "comps",
        ] {
            for language in LANGUAGES {
                // English says "dual" and "in" as the code does.
                if language == Language::En && ["dual", "in"].contains(&token) {
                    continue;
                }
                assert_ne!(
                    label_words(language, token),
                    token,
                    "{token} in {language:?}"
                );
            }
        }
        assert_eq!(label_words(Language::En, "in comps"), "in compounds");
    }

    #[test]
    fn an_unknown_token_is_kept_among_the_known_ones() {
        assert_eq!(
            label_words(Language::En, "acc zzz pl"),
            "accusative zzz plural"
        );
        assert_eq!(label_words(Language::Fr, "zzz"), "zzz");
        assert_eq!(label_words(Language::Vi, ""), "");
    }

    #[test]
    fn a_label_does_not_gain_spaces() {
        assert_eq!(label_words(Language::En, "pl"), "plural");
        assert_eq!(label_words(Language::En, "pl "), "plural ");
    }

    #[test]
    fn a_date_in_words_in_three_languages() {
        let date = day("2026-10-06");
        assert_eq!(date_words(Language::En, date), "6 October 2026");
        assert_eq!(date_words(Language::Fr, date), "6 octobre 2026");
        assert_eq!(date_words(Language::Vi, date), "ngày 6 tháng 10 năm 2026");
    }

    #[test]
    fn every_month_has_a_name_and_the_first_is_premier_in_french() {
        let en = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let fr = [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ];
        for month in 1..=12usize {
            let date = day(&format!("2027-{month:02}-01"));
            assert_eq!(
                date_words(Language::En, date),
                format!("1 {} 2027", en[month - 1])
            );
            assert_eq!(
                date_words(Language::Fr, date),
                format!("1er {} 2027", fr[month - 1])
            );
            assert_eq!(
                date_words(Language::Vi, date),
                format!("ngày 1 tháng {month} năm 2027")
            );
        }
    }

    #[test]
    fn a_date_with_a_two_digit_day_and_a_leap_day() {
        assert_eq!(
            date_words(Language::En, day("2028-02-29")),
            "29 February 2028"
        );
        assert_eq!(
            date_words(Language::Fr, day("2026-12-25")),
            "25 décembre 2026"
        );
    }
}
