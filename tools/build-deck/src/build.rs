//! Assembling the deck.
//!
//! For each key of the terms list, in order, one [`Term`]: the DPD columns,
//! the forms of [`parse_inflections`], the example blanked by
//! [`blank_example`]. Rank is the 1-based position in the list, so the deck
//! (ordered by rank, then id) is the same bytes at every build.

use std::fmt;

use std::collections::HashSet;

use learn_core::deck::{Deck, Example, Meaning, Pos, Term, TermId};
use learn_core::kind::DeckKind;
use rusqlite::Connection;

use crate::blank::blank_example;
use crate::inflections::parse_inflections;
use crate::reader::{RawTerm, ReadError, read_db_info, read_root, read_term};
use crate::words::deck_words;

/// What a build saw, printed on stderr by the command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Terms in the deck.
    pub terms: usize,
    /// Terms with at least one form.
    pub with_forms: usize,
    /// Terms whose example has a blank (a Context facet).
    pub with_blank: usize,
    /// Parser and data warnings, each prefixed by its term (`--strict` fails on any).
    pub warnings: Vec<String>,
    /// Titled cells with no value, over all terms: info, never a failure.
    pub empty_cells: usize,
    /// Info lines: examples with no blank, examples or languages with no
    /// translation (never a failure).
    pub notes: Vec<String>,
    /// Examples given a translation, per language (with `--api` only).
    pub translated: std::collections::BTreeMap<String, usize>,
    /// The distinct written forms of the examples.
    pub word_forms: usize,
    /// The forms the DPD gave at least one entry.
    pub words_found: usize,
}

/// Why a deck was not built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// The terms list has no key.
    NoKeys,
    /// A key appears twice in the terms list.
    DuplicateKey(String),
    /// The database could not give a key, a root or its info.
    Read(ReadError),
    /// `--strict` and the build had warnings (the report holds them).
    Strict(Report),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::NoKeys => f.write_str("the terms list has no key"),
            BuildError::DuplicateKey(key) => write!(f, "'{key}' is in the terms list twice"),
            BuildError::Read(error) => error.fmt(f),
            BuildError::Strict(report) => {
                write!(f, "--strict: {} warning(s)", report.warnings.len())?;
                report
                    .warnings
                    .iter()
                    .try_for_each(|warning| write!(f, "\n  {warning}"))
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl From<ReadError> for BuildError {
    fn from(error: ReadError) -> Self {
        BuildError::Read(error)
    }
}

/// The keys of a terms file: one per line, blank lines and `#` comment
/// lines skipped, each line trimmed.
pub fn parse_terms(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Builds the deck of `keys` from the database.
///
/// # Errors
/// [`BuildError::NoKeys`], [`BuildError::DuplicateKey`], [`BuildError::Read`]
/// (a missing key included), and with `strict` [`BuildError::Strict`] when
/// any warning was raised (blank notes are info, never strict).
pub fn build(
    conn: &Connection,
    keys: &[String],
    strict: bool,
) -> Result<(Deck, Report), BuildError> {
    if keys.is_empty() {
        return Err(BuildError::NoKeys);
    }
    let mut seen = HashSet::new();
    if let Some(twice) = keys.iter().find(|key| !seen.insert(key.as_str())) {
        return Err(BuildError::DuplicateKey(twice.clone()));
    }
    let info = read_db_info(conn)?;
    let mut report = Report::default();
    let mut terms = Vec::with_capacity(keys.len());
    for (index, key) in keys.iter().enumerate() {
        let rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        terms.push(make_term(conn, key, rank, &mut report)?);
    }
    terms.sort_by(|a, b| (a.rank, &a.id).cmp(&(b.rank, &b.id)));
    report.terms = terms.len();
    report.with_forms = terms.iter().filter(|t| !t.forms.is_empty()).count();
    report.with_blank = terms
        .iter()
        .filter(|t| t.example.as_ref().is_some_and(|e| e.blank.is_some()))
        .count();
    if strict && !report.warnings.is_empty() {
        return Err(BuildError::Strict(report));
    }
    let words = deck_words(conn, &terms)?;
    report.word_forms = words.forms;
    report.words_found = words.words.len();
    let source = format!("Digital Pāḷi Dictionary {}, {}", info.release, info.licence);
    let deck = Deck {
        version: Deck::VERSION,
        source,
        terms,
        words: words.words,
        kind: DeckKind::Words,
    };
    Ok((deck, report))
}

/// One term from its row, its root, its forms and its blanked example;
/// warnings and notes go to the report, prefixed by the key.
fn make_term(
    conn: &Connection,
    key: &str,
    rank: u32,
    report: &mut Report,
) -> Result<Term, BuildError> {
    let raw = read_term(conn, key)?;
    let parsed = parse_inflections(&raw.inflections_html);
    report
        .warnings
        .extend(parsed.warnings.iter().map(|w| format!("{key}: {w}")));
    report.empty_cells += parsed.empty_cells;
    if raw.meaning_1.trim().is_empty() {
        report.warnings.push(format!("{key}: empty meaning_1"));
    }
    let root = if raw.root_key.is_empty() {
        None
    } else {
        let root = read_root(conn, &raw.root_key)?;
        if root.is_none() {
            let message = format!("{key}: root '{}' is not in dpd_roots", raw.root_key);
            report.warnings.push(message);
        }
        root
    };
    let lemma = raw
        .lemma_1
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let example = make_example(&raw, &parsed.forms, report);
    Ok(Term {
        id: TermId(raw.lemma_1.clone()),
        lemma,
        pos: Pos(raw.pos.clone()),
        meaning: Meaning {
            en: raw.meaning_1.trim().to_string(),
            fr: None,
            vi: None,
            literal: non_empty(&raw.meaning_lit),
        },
        construction: non_empty(&raw.construction),
        root,
        example,
        forms: parsed.forms,
        rank,
    })
}

/// The example of a row, when it has one; a missing blank is a note.
fn make_example(
    raw: &RawTerm,
    forms: &[learn_core::deck::Form],
    report: &mut Report,
) -> Option<Example> {
    if raw.example_1.trim().is_empty() {
        return None;
    }
    let blanked = blank_example(&raw.example_1, &raw.lemma_1, forms);
    if let Some(note) = blanked.note {
        report.notes.push(format!("{}: {note}", raw.lemma_1));
    }
    Some(Example {
        text: blanked.text,
        source: raw.source_1.trim().to_string(),
        sutta: raw.sutta_1.trim().to_string(),
        blank: blanked.blank,
        translations: std::collections::BTreeMap::new(),
    })
}

/// The trimmed text, or `None` when it is empty.
fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::testdb::connection;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_string()).collect()
    }

    #[test]
    fn terms_files_skip_comments_and_blank_lines() {
        assert_eq!(
            parse_terms("# the glue\nkho\n\n  ca 2.1  \n#more\nsati 1.1\n"),
            keys(&["kho", "ca 2.1", "sati 1.1"])
        );
        assert_eq!(parse_terms("").len(), 0);
    }

    #[test]
    fn a_term_gets_every_column_of_its_row() {
        let (deck, report) = build(&connection(), &keys(&["sati 1.1"]), false).unwrap();
        let term = &deck.terms[0];
        assert_eq!(term.id.0, "sati 1.1");
        assert_eq!(term.lemma, "sati");
        assert_eq!(term.pos.0, "fem");
        assert_eq!(term.meaning.en, "memory; remembering");
        assert_eq!(term.meaning.literal.as_deref(), Some("remembering"));
        assert_eq!(
            (term.meaning.fr.as_ref(), term.meaning.vi.as_ref()),
            (None, None)
        );
        assert_eq!(term.construction.as_deref(), Some("√sar + ti"));
        let root = term.root.as_ref().unwrap();
        assert_eq!(
            (root.key.as_str(), root.meaning.as_str()),
            ("√sar 2", "remember")
        );
        assert_eq!(term.rank, 1);
        assert_eq!(term.forms.len(), 2);
        assert!(!term.forms[0].rare && term.forms[1].rare);
        let example = term.example.as_ref().unwrap();
        assert_eq!(example.text, "atthi dhammā satiyā sacchikaraṇīyā");
        assert_eq!(example.source, "AN4.189");
        assert_eq!(example.sutta, "sacchikaraṇīyasuttaṃ");
        assert_eq!(example.blank.as_ref().unwrap().answer, "satiyā");
        assert_eq!(report.warnings.len(), 0);
    }

    #[test]
    fn empty_columns_become_absent_options() {
        let (deck, _) = build(&connection(), &keys(&["kho"]), false).unwrap();
        let term = &deck.terms[0];
        assert_eq!(term.meaning.literal, None);
        assert_eq!(term.construction, None);
        assert_eq!(term.root, None);
        assert_eq!(term.forms.len(), 0);
        assert!(term.example.is_some());
    }

    #[test]
    fn a_row_with_no_example_has_none() {
        let (deck, _) = build(&connection(), &keys(&["badtable 1"]), false).unwrap();
        assert_eq!(deck.terms[0].example, None);
    }

    #[test]
    fn the_source_line_comes_from_db_info() {
        let (deck, _) = build(&connection(), &keys(&["kho"]), false).unwrap();
        assert_eq!(deck.version, Deck::VERSION);
        assert_eq!(
            deck.source,
            "Digital Pāḷi Dictionary v0.4.test, CC BY-NC-SA 4.0"
        );
    }

    #[test]
    fn rank_is_the_line_order_and_the_output_is_byte_stable() {
        let list = keys(&["kho", "sati 1.1", "lone 1"]);
        let (first, _) = build(&connection(), &list, false).unwrap();
        let (second, _) = build(&connection(), &list, false).unwrap();
        assert_eq!(first.to_json(), second.to_json());
        let order: Vec<(&str, u32)> = first
            .terms
            .iter()
            .map(|t| (t.id.0.as_str(), t.rank))
            .collect();
        assert_eq!(order, vec![("kho", 1), ("sati 1.1", 2), ("lone 1", 3)]);
        assert_eq!(Deck::from_json(&first.to_json()).unwrap(), first);
    }

    #[test]
    fn the_report_counts_forms_blanks_warnings_and_notes() {
        let list = keys(&["sati 1.1", "kho", "lone 1", "badtable 1"]);
        let (_, report) = build(&connection(), &list, false).unwrap();
        assert_eq!(report.terms, 4);
        assert_eq!(report.with_forms, 1);
        assert_eq!(report.with_blank, 2);
        assert_eq!(report.warnings.len(), 1, "{report:?}");
        assert!(report.warnings[0].starts_with("badtable 1: "), "{report:?}");
        assert_eq!(report.notes.len(), 1, "{report:?}");
        assert!(
            report.notes[0].starts_with("lone 1: no blank"),
            "{report:?}"
        );
    }

    #[test]
    fn the_deck_carries_the_words_of_its_examples_its_own_terms_first() {
        let (deck, report) = build(&connection(), &keys(&["kho", "sati 1.1"]), false).unwrap();
        let heads: Vec<&str> = deck.words["kho"]
            .iter()
            .map(|e| e.headword.as_str())
            .collect();
        assert_eq!(heads, vec!["kho", "kho 2"]);
        // evaṃ kho bhagavā · atthi dhammā satiyā sacchikaraṇīyā
        assert_eq!((report.word_forms, report.words_found), (7, 1));
        let again = Deck::from_json(&deck.to_json()).unwrap();
        assert_eq!(again.words, deck.words);
    }

    #[test]
    fn strict_turns_a_warning_into_a_failure_but_not_a_note() {
        let conn = connection();
        let err = build(&conn, &keys(&["kho", "badtable 1"]), true).unwrap_err();
        let BuildError::Strict(report) = err else {
            panic!("expected Strict, got {err:?}");
        };
        assert_eq!(report.warnings.len(), 1);
        // a note alone is info
        assert!(build(&conn, &keys(&["lone 1"]), true).is_ok());
    }

    /// `kho` given a table with two empty titled cells and one value.
    fn with_empty_cells() -> Connection {
        let conn = connection();
        conn.execute(
            "UPDATE dpd_headwords SET inflections_html = \"<table class='inflection'><tr>\
             <td title='masc nom sg'></td><td title='masc acc sg'></td>\
             <td title='masc nom pl'>x<b>o</b></td></tr></table>\" WHERE lemma_1 = 'kho'",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn empty_cells_are_counted_and_never_fail_strict() {
        let conn = with_empty_cells();
        let (_, report) = build(&conn, &keys(&["kho", "sati 1.1"]), true).unwrap();
        assert_eq!(report.empty_cells, 2, "{report:?}");
        assert_eq!(report.warnings.len(), 0, "{report:?}");
    }

    #[test]
    fn an_empty_meaning_is_a_warning() {
        let (_, report) = build(&connection(), &keys(&["nomeaning 1"]), false).unwrap();
        assert_eq!(report.warnings.len(), 1, "{report:?}");
        assert!(report.warnings[0].contains("meaning"));
    }

    #[test]
    fn an_unknown_root_is_a_warning() {
        let conn = connection();
        conn.execute(
            "UPDATE dpd_headwords SET root_key = '√zzz' WHERE lemma_1 = 'kho'",
            [],
        )
        .unwrap();
        let (deck, report) = build(&conn, &keys(&["kho"]), false).unwrap();
        assert_eq!(deck.terms[0].root, None);
        assert_eq!(report.warnings.len(), 1, "{report:?}");
    }

    #[test]
    fn a_duplicate_key_fails() {
        assert_eq!(
            build(&connection(), &keys(&["kho", "sati 1.1", "kho"]), false).unwrap_err(),
            BuildError::DuplicateKey("kho".to_string())
        );
    }

    #[test]
    fn a_missing_key_fails() {
        assert_eq!(
            build(&connection(), &keys(&["kho", "nope 9"]), false).unwrap_err(),
            BuildError::Read(ReadError::MissingKey("nope 9".to_string()))
        );
    }

    #[test]
    fn a_deck_with_no_key_fails() {
        assert_eq!(
            build(&connection(), &[], false).unwrap_err(),
            BuildError::NoKeys
        );
    }

    #[test]
    fn each_error_says_what_is_wrong() {
        assert_ne!(BuildError::NoKeys.to_string(), "");
        assert!(
            BuildError::DuplicateKey("kho".into())
                .to_string()
                .contains("kho")
        );
        assert!(
            BuildError::Read(ReadError::MissingKey("zz".into()))
                .to_string()
                .contains("zz")
        );
        let report = Report {
            warnings: vec!["a: b".into()],
            ..Report::default()
        };
        assert!(BuildError::Strict(report).to_string().contains("a: b"));
    }
}
