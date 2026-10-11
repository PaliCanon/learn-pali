//! Every word of the examples, looked up in the DPD.
//!
//! Each example is cut into its written words ([`tokenise`]), each word
//! keyed with [`word_key`] and looked up in the DPD's `lookup` table: the
//! key as written, then without its sandhi apostrophe (`sampādethā'ti` →
//! `sampādethāti`). A word written with a sandhi apostrophe shows its
//! `deconstructor` parts, each by its best entry, in place of the joined
//! form's own headwords. Otherwise a row's `headwords` ids give the
//! entries (lemma, pos, a short meaning), its `grammar` the form's
//! inflection for each headword; a row with no headword gives its
//! `deconstructor` parts' entries instead. The deck's own terms come first, then the DPD's
//! order, at most [`MAX_ENTRIES`] per form. One map for the whole deck.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use learn_core::deck::Term;
use learn_core::words::{WordEntry, word_key};
use rusqlite::{Connection, OptionalExtension};

use crate::reader::ReadError;

/// The most entries kept for one written form.
pub const MAX_ENTRIES: usize = 3;

/// The longest meaning kept, in characters (cut at a `;`).
pub const MAX_MEANING: usize = 80;

/// The words of a deck: the entries by key, and how many distinct forms
/// the examples have (found or not).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeckWords {
    /// The entries of every form the DPD knows, by [`word_key`].
    pub words: BTreeMap<String, Vec<WordEntry>>,
    /// The distinct forms of the examples.
    pub forms: usize,
}

/// A letter of a Pāli word: alphabetic, or a combining mark (a dot or a
/// macron written apart from its letter).
fn is_letter(c: char) -> bool {
    c.is_alphabetic() || ('\u{300}'..='\u{36f}').contains(&c)
}

/// The written words of a text: runs of letters (Pāli diacritics
/// included), an apostrophe between two letters kept inside the word
/// (`sampādethā'ti`); everything else separates.
pub fn tokenise(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut words = Vec::new();
    let mut start = None;
    for (i, &(at, c)) in chars.iter().enumerate() {
        let joins = matches!(c, '\'' | '’')
            && start.is_some()
            && chars.get(i + 1).is_some_and(|&(_, next)| is_letter(next));
        if is_letter(c) || joins {
            start.get_or_insert(at);
        } else if let Some(from) = start.take() {
            words.push(&text[from..at]);
        }
    }
    if let Some(from) = start {
        words.push(&text[from..]);
    }
    words
}

/// A meaning cut to at most [`MAX_MEANING`] characters at a `;`; a first
/// sense longer than that is cut at a space, with `…`.
pub fn short_meaning(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_MEANING {
        return text.to_string();
    }
    let mut kept = String::new();
    for sense in text.split(';').map(str::trim) {
        let longer = if kept.is_empty() {
            sense.to_string()
        } else {
            format!("{kept}; {sense}")
        };
        if longer.chars().count() > MAX_MEANING {
            break;
        }
        kept = longer;
    }
    if kept.is_empty() {
        let head: String = text.chars().take(MAX_MEANING).collect();
        let cut = head.rfind(' ').map_or(head.as_str(), |at| &head[..at]);
        kept = format!("{}…", cut.trim_end_matches([',', ' ']));
    }
    kept
}

/// One `lookup` row, its JSON columns parsed (empty when blank).
#[derive(Debug, Default)]
struct LookupRow {
    headwords: Vec<i64>,
    grammar: Vec<(String, String, String)>,
    deconstructor: Vec<String>,
}

/// One headword's columns the entries need.
#[derive(Debug, Clone)]
struct Headword {
    lemma_1: String,
    pos: String,
    meaning: String,
}

/// A JSON column, empty when blank or not the expected shape.
fn json_column<T: serde::de::DeserializeOwned + Default>(text: &str) -> T {
    serde_json::from_str(text).unwrap_or_default()
}

/// The lemma without its homonym number (`atha 1` → `atha`).
fn clean_lemma(lemma_1: &str) -> &str {
    match lemma_1.rsplit_once(' ') {
        Some((head, number)) if number.chars().all(|c| c.is_ascii_digit() || c == '.') => head,
        _ => lemma_1,
    }
}

/// Whether a `lookup.grammar` part of speech (`verb`, `noun`, or a DPD
/// code) names a headword's `pos`.
fn pos_matches(grammar_pos: &str, pos: &str) -> bool {
    grammar_pos == pos
        || match grammar_pos {
            "noun" => matches!(pos, "masc" | "fem" | "nt"),
            "verb" => matches!(
                pos,
                "pr" | "aor"
                    | "opt"
                    | "imp"
                    | "fut"
                    | "cond"
                    | "perf"
                    | "imperf"
                    | "abs"
                    | "ger"
                    | "inf"
            ),
            _ => false,
        }
}

/// Reads the database for one deck, each row read once.
struct Resolver<'c> {
    conn: &'c Connection,
    own: HashSet<String>,
    headwords: HashMap<i64, Option<Headword>>,
}

impl Resolver<'_> {
    /// The `lookup` row of `key`, if any.
    fn row(&self, key: &str) -> Result<Option<LookupRow>, ReadError> {
        let row = self
            .conn
            .query_row(
                "SELECT headwords, grammar, deconstructor FROM lookup WHERE lookup_key = ?1",
                [key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(headwords, grammar, deconstructor)| LookupRow {
            headwords: json_column(&headwords),
            grammar: json_column(&grammar),
            deconstructor: json_column(&deconstructor),
        }))
    }

    /// The headword `id`, read once; `None` when absent.
    fn headword(&mut self, id: i64) -> Result<Option<Headword>, ReadError> {
        if let Some(known) = self.headwords.get(&id) {
            return Ok(known.clone());
        }
        let read = self
            .conn
            .query_row(
                "SELECT lemma_1, pos, meaning_1, meaning_2 FROM dpd_headwords WHERE id = ?1",
                [id],
                |row| {
                    let text = |at| {
                        row.get::<_, Option<String>>(at)
                            .map(Option::unwrap_or_default)
                    };
                    let (meaning_1, meaning_2) = (text(2)?, text(3)?);
                    let meaning = if meaning_1.trim().is_empty() {
                        meaning_2
                    } else {
                        meaning_1
                    };
                    Ok(Headword {
                        lemma_1: text(0)?,
                        pos: text(1)?,
                        meaning: short_meaning(&meaning),
                    })
                },
            )
            .optional()?;
        self.headwords.insert(id, read.clone());
        Ok(read)
    }

    /// A row's entries: its headwords with a meaning, the deck's own first.
    fn entries(&mut self, row: &LookupRow) -> Result<Vec<WordEntry>, ReadError> {
        let mut entries = Vec::new();
        for &id in &row.headwords {
            let Some(headword) = self.headword(id)? else {
                continue;
            };
            if headword.meaning.is_empty() {
                continue;
            }
            let grammar: Vec<&str> = row
                .grammar
                .iter()
                .filter(|(word, pos, _)| {
                    word == clean_lemma(&headword.lemma_1) && pos_matches(pos, &headword.pos)
                })
                .map(|(_, _, grammar)| grammar.as_str())
                .collect();
            entries.push(WordEntry {
                grammar: (!grammar.is_empty()).then(|| grammar.join("; ")),
                headword: headword.lemma_1,
                pos: headword.pos,
                meaning: headword.meaning,
            });
        }
        entries.sort_by_key(|entry| !self.own.contains(&entry.headword));
        Ok(entries)
    }

    /// The entries of one keyed form, at most [`MAX_ENTRIES`].
    fn form(&mut self, key: &str) -> Result<Vec<WordEntry>, ReadError> {
        let mut rows = vec![self.row(key)?];
        if key.contains('\'') {
            rows.push(self.row(&key.replace('\'', ""))?);
        }
        let rows: Vec<LookupRow> = rows.into_iter().flatten().collect();
        let sandhi = rows.iter().find_map(|row| row.deconstructor.first());
        if let Some(parts) = sandhi.filter(|_| key.contains('\'')) {
            let best = self.best_of_parts(parts)?;
            if !best.is_empty() {
                return Ok(best);
            }
        }
        for row in rows.iter().filter(|row| !row.headwords.is_empty()) {
            let mut entries = self.entries(row)?;
            if !entries.is_empty() {
                entries.truncate(MAX_ENTRIES);
                return Ok(entries);
            }
        }
        match sandhi {
            Some(parts) => self.parts(parts),
            None => Ok(Vec::new()),
        }
    }

    /// A sandhi word's parts (`sampādetha + iti`), each by its best entry
    /// (the deck's own, else the DPD's first), at most [`MAX_ENTRIES`].
    fn best_of_parts(&mut self, parts: &str) -> Result<Vec<WordEntry>, ReadError> {
        let mut best = Vec::new();
        for part in parts.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            if let Some(row) = self.row(part)?
                && let Some(entry) = self.entries(&row)?.into_iter().next()
            {
                best.push(entry);
            }
        }
        best.truncate(MAX_ENTRIES);
        Ok(best)
    }

    /// The entries of a deconstruction's parts (`sampādetha + iti`): taken
    /// in turn, one per part, up to [`MAX_ENTRIES`], shown part by part.
    fn parts(&mut self, parts: &str) -> Result<Vec<WordEntry>, ReadError> {
        let mut by_part = Vec::new();
        for part in parts.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            let entries = match self.row(part)? {
                Some(row) => self.entries(&row)?,
                None => Vec::new(),
            };
            by_part.push(entries);
        }
        let mut taken = vec![0; by_part.len()];
        let mut count = 0;
        for round in 0..MAX_ENTRIES {
            for (part, entries) in by_part.iter().enumerate() {
                if count < MAX_ENTRIES && round < entries.len() {
                    taken[part] += 1;
                    count += 1;
                }
            }
        }
        Ok(by_part
            .into_iter()
            .zip(taken)
            .flat_map(|(entries, n)| entries.into_iter().take(n))
            .collect())
    }
}

/// Looks up every word of every term's example.
///
/// # Errors
/// [`ReadError::Sql`] when SQLite fails (a missing `lookup` table
/// included).
pub fn deck_words(conn: &Connection, terms: &[Term]) -> Result<DeckWords, ReadError> {
    let forms: BTreeSet<String> = terms
        .iter()
        .filter_map(|term| term.example.as_ref())
        .flat_map(|example| tokenise(&example.text))
        .map(word_key)
        .filter(|key| !key.is_empty())
        .collect();
    let mut resolver = Resolver {
        conn,
        own: terms.iter().map(|term| term.id.0.clone()).collect(),
        headwords: HashMap::new(),
    };
    let mut words = BTreeMap::new();
    for key in &forms {
        let entries = resolver.form(key)?;
        if !entries.is_empty() {
            words.insert(key.clone(), entries);
        }
    }
    Ok(DeckWords {
        words,
        forms: forms.len(),
    })
}

#[cfg(test)]
mod tests {
    use learn_core::deck::{Example, Meaning, Pos, TermId};

    use super::*;
    use crate::reader::testdb::connection;

    #[test]
    fn words_are_runs_of_letters_with_their_sandhi_apostrophe() {
        assert_eq!(
            tokenise("Atha kho bhagavā bhikkhū āmantesi: ‘sampādethā’ti.’"),
            vec![
                "Atha",
                "kho",
                "bhagavā",
                "bhikkhū",
                "āmantesi",
                "sampādethā’ti"
            ]
        );
        assert_eq!(
            tokenise("na'y'idaṃ, m'ākāsi\n'ti diṭṭhi-saṃyojana 12 vadāmi'"),
            vec![
                "na'y'idaṃ",
                "m'ākāsi",
                "ti",
                "diṭṭhi",
                "saṃyojana",
                "vadāmi"
            ]
        );
        assert_eq!(tokenise("ākaṅkhissasi… "), vec!["ākaṅkhissasi"]);
        // a combining dot stays in its word
        assert_eq!(tokenise("evam\u{323} me"), vec!["evam\u{323}", "me"]);
        assert_eq!(tokenise(" , . '' "), Vec::<&str>::new());
    }

    #[test]
    fn a_long_meaning_is_cut_at_a_semicolon() {
        assert_eq!(short_meaning(" then; also "), "then; also");
        let long = "presses on; causes to accomplish; strives to achieve; tries to accomplish \
                    one's purpose";
        assert_eq!(
            short_meaning(long),
            "presses on; causes to accomplish; strives to achieve"
        );
        let one = "a very long single sense with no semicolon at all that goes on and on \
                   past the limit of eighty characters";
        let cut = short_meaning(one);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(cut.chars().count() <= MAX_MEANING + 1, "{cut}");
        assert!(one.starts_with(cut.trim_end_matches('…')), "{cut}");
    }

    fn term(id: &str, text: &str) -> Term {
        Term {
            id: TermId(id.to_string()),
            lemma: id.to_string(),
            pos: Pos("ind".to_string()),
            meaning: Meaning {
                en: "x".to_string(),
                fr: None,
                vi: None,
                literal: None,
            },
            construction: None,
            root: None,
            example: Some(Example {
                text: text.to_string(),
                source: "MN 1".to_string(),
                sutta: "mn1".to_string(),
                blank: None,
                translations: BTreeMap::new(),
            }),
            forms: vec![],
            rank: 1,
        }
    }

    fn entry(headword: &str, pos: &str, grammar: Option<&str>, meaning: &str) -> WordEntry {
        WordEntry {
            headword: headword.to_string(),
            pos: pos.to_string(),
            grammar: grammar.map(str::to_string),
            meaning: meaning.to_string(),
        }
    }

    fn sample() -> DeckWords {
        let terms = [
            term("kho", "Atha kho bhagavā āmantesi: ‘sampādethā’ti.’"),
            term("lone 1", "tass'eva ti, atha zzz nomean m'ākāsi tasseva"),
        ];
        deck_words(&connection(), &terms).unwrap()
    }

    #[test]
    fn a_word_gets_its_headwords_in_dpd_order() {
        let words = sample();
        assert_eq!(
            words.words["atha"],
            vec![
                entry("atha 1", "ind", None, "then; also; and so; after that"),
                entry("atha 2", "ind", None, "but; rather; even"),
            ]
        );
    }

    #[test]
    fn a_form_gets_its_grammar_for_the_matching_headword_only() {
        let words = sample();
        assert_eq!(
            words.words["āmantesi"],
            vec![
                entry(
                    "āmanteti",
                    "pr",
                    Some("pr 2nd sg"),
                    "invites; calls; summons"
                ),
                entry(
                    "āmantesi",
                    "aor",
                    Some("aor 3rd sg; aor 2nd sg"),
                    "addressed; said (to); advised"
                ),
            ]
        );
        // `ti` has four headwords: three kept; the grammar is the `ve` one's
        let ti = &words.words["ti"];
        let heads: Vec<(&str, Option<&str>)> = ti
            .iter()
            .map(|e| (e.headword.as_str(), e.grammar.as_deref()))
            .collect();
        assert_eq!(
            heads,
            vec![
                ("ti 1", None),
                ("ti 2", None),
                ("ti 3", Some("masc nom sg"))
            ]
        );
    }

    #[test]
    fn a_sandhi_form_with_no_deconstruction_is_found_without_its_apostrophe() {
        let words = sample();
        assert_eq!(
            words.words["m'ākāsi"],
            vec![entry("mākāsi", "aor", Some("aor 3rd sg"), "do not make")]
        );
    }

    #[test]
    fn a_sandhi_word_shows_its_parts_even_when_it_has_its_own_headword() {
        let words = sample();
        assert_eq!(
            words.words["sampādethā'ti"],
            vec![
                entry(
                    "sampādeti 1",
                    "pr",
                    Some("imp 2nd pl"),
                    "tries to accomplish"
                ),
                entry("iti", "ind", None, "thus; like this"),
            ]
        );
    }

    #[test]
    fn a_sandhi_word_shows_each_parts_best_entry_only() {
        let words = sample();
        assert_eq!(
            words.words["tass'eva"],
            vec![
                entry("ta 1", "pron", Some("masc dat sg"), "that; he; it"),
                entry("eva", "ind", None, "only; just; really"),
            ]
        );
    }

    #[test]
    fn a_joined_form_with_no_headword_gets_its_parts_entries_in_turn() {
        let words = sample();
        let heads: Vec<&str> = words.words["tasseva"]
            .iter()
            .map(|e| e.headword.as_str())
            .collect();
        assert_eq!(heads, vec!["ta 1", "ta 2", "eva"]);
    }

    #[test]
    fn the_decks_own_term_comes_first() {
        let words = sample();
        let heads: Vec<&str> = words.words["kho"]
            .iter()
            .map(|e| e.headword.as_str())
            .collect();
        assert_eq!(heads, vec!["kho", "kho 2"]);
    }

    #[test]
    fn an_unknown_word_and_a_headword_without_meaning_are_absent() {
        let words = sample();
        for absent in ["zzz", "bhagavā", "nomean", "Atha"] {
            assert!(!words.words.contains_key(absent), "{absent}");
        }
        // atha kho bhagavā āmantesi sampādethā'ti tass'eva ti zzz nomean
        // m'ākāsi tasseva
        assert_eq!(words.forms, 11);
        assert_eq!(words.words.len(), 8);
    }

    #[test]
    fn a_database_without_the_lookup_table_is_an_error() {
        let conn = connection();
        conn.execute_batch("DROP TABLE lookup").unwrap();
        let terms = [term("kho", "atha")];
        assert!(matches!(deck_words(&conn, &terms), Err(ReadError::Sql(_))));
    }
}
