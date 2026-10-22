//! The Book: the progress file of every deck a learner follows.
//!
//! A [`Book`] is `{version, decks: {deck_id: {last_today, progress, added}}}`:
//! each entry is exactly the body of a [`State`] (a wrapper, not a rewrite),
//! and the `words` entry (the DPD deck every learner has) is always there.
//! `last_today` and `added` stay per deck.
//!
//! **The flat rule.** A Book is written in today's flat shape (a [`State`]
//! text, stamp 1 or 2) whenever every entry other than `words` has an empty
//! `progress` and an empty `added`: an entry holding only a `last_today`
//! stays flat, and the build before the Book keeps reading the file. Once
//! another deck holds a started term or an added word the Book is written
//! with stamp 3, which an older build refuses (`State::from_json_upto(text,
//! 2)` gives [`StateError::UnknownVersion`]) and so never overwrites.
//! A flat file reads as `decks: {words: it}`.
//!
//! **Sync.** [`merge_books`] is today's join per deck id; a deck only one
//! side has is kept whole, and an unknown deck id is kept as it is, so a
//! build that does not know a deck still carries it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::deck::{Deck, Term};
use crate::merge::merge;
use crate::progress::{Progress, State, StateError};

/// The id of a deck in the Book (`words`, `devanagari`, ...).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeckId(pub String);

impl fmt::Display for DeckId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl DeckId {
    /// The id of the words deck, whose entry a Book always holds.
    pub const WORDS: &'static str = "words";

    /// The id of the words deck.
    pub fn words() -> DeckId {
        DeckId(DeckId::WORDS.to_string())
    }

    /// Whether this is the words deck.
    pub fn is_words(&self) -> bool {
        self.0 == DeckId::WORDS
    }
}

/// The progress of every deck, one [`State`] per deck id; the `words` entry
/// is always present. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    decks: BTreeMap<DeckId, State>,
}

/// One deck's entry as the stamp-3 text holds it: a [`State`] body without
/// the version.
#[derive(Deserialize)]
struct ReadEntry {
    #[serde(default)]
    last_today: Option<Date>,
    progress: Vec<Progress>,
    #[serde(default)]
    added: Vec<Term>,
}

impl ReadEntry {
    /// The entry as a state, refusing a term twice in `progress` or `added`.
    fn into_state(self) -> Result<State, StateError> {
        let mut seen = HashSet::new();
        if let Some(twice) = self.progress.iter().find(|p| !seen.insert(&p.term)) {
            return Err(StateError::DuplicateTerm(twice.term.clone()));
        }
        let mut seen = HashSet::new();
        if let Some(twice) = self.added.iter().find(|t| !seen.insert(&t.id)) {
            return Err(StateError::DuplicateTerm(twice.id.clone()));
        }
        Ok(State {
            version: State::VERSION,
            last_today: self.last_today,
            progress: self.progress,
            added: self.added,
        })
    }
}

/// The stamp-3 text as read.
#[derive(Deserialize)]
struct ReadBook {
    decks: BTreeMap<DeckId, ReadEntry>,
}

/// One deck's entry as the stamp-3 text is written.
#[derive(Serialize)]
struct WrittenEntry<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    last_today: Option<Date>,
    progress: &'a [Progress],
    #[serde(skip_serializing_if = "<[Term]>::is_empty")]
    added: &'a [Term],
}

/// The stamp-3 text as written.
#[derive(Serialize)]
struct WrittenBook<'a> {
    version: u32,
    decks: BTreeMap<&'a DeckId, WrittenEntry<'a>>,
}

impl Book {
    /// The version stamp of a Book that is not flat.
    pub const VERSION: u32 = 3;

    /// A Book with an empty `words` entry (the first run).
    pub fn empty() -> Book {
        Book::from_state(State::empty())
    }

    /// The flat file as a Book: `words` is `words`, the only entry.
    pub fn from_state(words: State) -> Book {
        Book {
            decks: BTreeMap::from([(DeckId::words(), words)]),
        }
    }

    /// Reads a flat file (version 1 or 2) or a stamp-3 Book.
    ///
    /// # Errors
    /// As [`Book::from_json_upto`].
    pub fn from_json(text: &str) -> Result<Book, StateError> {
        Book::from_json_upto(text, Book::VERSION)
    }

    /// [`Book::from_json`] as a build whose highest version is `newest`
    /// reads it. A flat file (version 1 or 2, as [`State::from_json_upto`]
    /// reads it) is the `words` entry; version 3 is the `decks` object, each
    /// entry a state body, an unknown deck id kept and a missing `words`
    /// entry added empty. The version is read before the shape, so a future
    /// version of another shape is an unknown version, not bad JSON.
    ///
    /// # Errors
    /// [`StateError::Json`], [`StateError::UnknownVersion`] (0, or above
    /// `newest`) or [`StateError::DuplicateTerm`] (a term twice in one
    /// entry's `progress` or `added`).
    pub fn from_json_upto(text: &str, newest: u32) -> Result<Book, StateError> {
        let json_error = |error: serde_json::Error| StateError::Json(error.to_string());
        let raw: serde_json::Value = serde_json::from_str(text).map_err(json_error)?;
        let version = raw.get("version").and_then(serde_json::Value::as_u64);
        if let Some(version) = version
            && (version == 0 || version > u64::from(newest))
        {
            return Err(StateError::UnknownVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        if version != Some(u64::from(Book::VERSION)) {
            return State::from_json_upto(text, newest.min(State::VERSION)).map(Book::from_state);
        }
        let read: ReadBook = serde_json::from_value(raw).map_err(json_error)?;
        let mut book = Book::empty();
        for (id, entry) in read.decks {
            book.decks.insert(id, entry.into_state()?);
        }
        Ok(book)
    }

    /// The Book as pretty JSON: flat (the `words` state's own text, stamp 1
    /// or 2) while [`Book::is_flat`], else stamp 3 with every entry,
    /// `words` included even when empty, ids sorted.
    ///
    /// # Panics
    /// Never in practice: the types serialise without failing.
    pub fn to_json(&self) -> String {
        if self.is_flat() {
            return self.words().to_json();
        }
        let decks = self
            .decks
            .iter()
            .map(|(id, state)| {
                let entry = WrittenEntry {
                    last_today: state.last_today,
                    progress: &state.progress,
                    added: &state.added,
                };
                (id, entry)
            })
            .collect();
        let written = WrittenBook {
            version: Book::VERSION,
            decks,
        };
        serde_json::to_string_pretty(&written).expect("a book always serialises")
    }

    /// Whether the Book is written flat: every entry other than `words` has
    /// an empty `progress` and an empty `added` (a `last_today` alone does
    /// not count).
    pub fn is_flat(&self) -> bool {
        self.decks.iter().all(|(id, state)| {
            id.is_words() || (state.progress.is_empty() && state.added.is_empty())
        })
    }

    /// The entry of a deck, if the Book holds it.
    pub fn get(&self, id: &DeckId) -> Option<&State> {
        self.decks.get(id)
    }

    /// A copy of the entry of a deck: empty when the Book has none.
    pub fn state(&self, id: &DeckId) -> State {
        self.get(id).cloned().unwrap_or_else(State::empty)
    }

    /// Replaces the entry of a deck (adding it when absent).
    pub fn set(&mut self, id: DeckId, state: State) {
        self.decks.insert(id, state);
    }

    /// The `words` entry, which a Book always holds.
    ///
    /// # Panics
    /// Never: the constructors and the reader keep the entry.
    pub fn words(&self) -> &State {
        &self.decks[&DeckId::words()]
    }

    /// Every entry, sorted by deck id.
    pub fn decks(&self) -> impl Iterator<Item = (&DeckId, &State)> {
        self.decks.iter()
    }

    /// [`State::shadow_added`] on the entry of `id` by `deck`; 0 when the
    /// Book has no such entry.
    pub fn shadow_added(&mut self, id: &DeckId, deck: &Deck) -> usize {
        self.decks
            .get_mut(id)
            .map_or(0, |state| state.shadow_added(deck))
    }
}

/// The join of two Books: per deck id of the union, both sides give
/// [`merge`] of their entries; a deck only one side has is kept whole (an
/// unknown deck id too).
pub fn merge_books(a: &Book, b: &Book) -> Book {
    let ids: BTreeSet<&DeckId> = a.decks.keys().chain(b.decks.keys()).collect();
    let decks = ids
        .into_iter()
        .map(|id| {
            let joined = match (a.get(id), b.get(id)) {
                (Some(ours), Some(theirs)) => merge(ours, theirs),
                (Some(only), None) | (None, Some(only)) => only.clone(),
                (None, None) => State::empty(),
            };
            (id.clone(), joined)
        })
        .collect();
    Book { decks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::TermId;
    use crate::deck::samples::{deck, indeclinable};
    use crate::facet::Facet;
    use crate::stage::Stage;

    fn day(days: u32) -> Date {
        Date::from_days(20_000 + days)
    }

    fn id(name: &str) -> DeckId {
        DeckId(name.to_string())
    }

    fn entry(term: &str, lapses: u32, stage: Stage, due: u32, passed: &[Facet]) -> Progress {
        Progress {
            term: TermId(term.to_string()),
            stage,
            passed: passed.iter().copied().collect(),
            due: day(due),
            lapses,
        }
    }

    fn state(last: Option<u32>, progress: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            added: Vec::new(),
            last_today: last.map(day),
            progress,
        }
    }

    fn word(term: &str, sense: &str) -> Term {
        let mut term = indeclinable(term, u32::MAX);
        term.meaning.en = sense.to_string();
        term
    }

    fn with_added(mut state: State, added: Vec<Term>) -> State {
        state.added = added;
        state
    }

    /// One started term, `t`, at the given lapses.
    fn started(term: &str, lapses: u32) -> State {
        state(
            Some(1),
            vec![entry(term, lapses, Stage::Week, 3, &[Facet::Meaning])],
        )
    }

    /// A stamp-1 state text with one term.
    fn v1_state() -> State {
        started("sati 1.1", 0)
    }

    /// A stamp-2 state text: one term and one added word.
    fn v2_state() -> State {
        with_added(started("sati 1.1", 0), vec![word("citta", "mind")])
    }

    /// A Book with `words` and a `devanagari` entry.
    fn book_with(words: State, devanagari: State) -> Book {
        let mut book = Book::from_state(words);
        book.set(id("devanagari"), devanagari);
        book
    }

    /// A stamp-3 text: words empty, devanagari with one started term.
    fn v3_text() -> String {
        book_with(State::empty(), started("ka", 0)).to_json()
    }

    // ---- Reading a flat file ----

    #[test]
    fn a_flat_v1_text_reads_as_words() {
        let words = v1_state();
        let text = words.to_json();
        assert!(text.contains("\"version\": 1"), "{text}");
        let book = Book::from_json(&text).unwrap();
        assert_eq!(book.get(&DeckId::words()), Some(&words));
        assert_eq!(book.decks().count(), 1);
    }

    #[test]
    fn a_flat_v2_text_with_an_added_word_reads_as_words() {
        let words = v2_state();
        let text = words.to_json();
        assert!(text.contains("\"version\": 2"), "{text}");
        let book = Book::from_json(&text).unwrap();
        assert_eq!(book.get(&DeckId::words()), Some(&words));
        assert_eq!(book.decks().count(), 1);
    }

    #[test]
    fn an_empty_flat_text_reads_as_an_empty_book() {
        let book = Book::from_json(&State::empty().to_json()).unwrap();
        assert_eq!(book, Book::empty());
    }

    // ---- Writing: the flat rule ----

    #[test]
    fn a_words_only_book_is_written_flat_byte_for_byte() {
        for text in [v1_state().to_json(), v2_state().to_json()] {
            let book = Book::from_json(&text).unwrap();
            assert!(book.is_flat());
            assert_eq!(book.to_json(), text);
        }
    }

    #[test]
    fn devanagari_holding_only_a_last_today_stays_flat() {
        let words = v1_state();
        let only_a_day = state(Some(7), Vec::new());
        let book = book_with(words.clone(), only_a_day);
        assert!(book.is_flat());
        assert_eq!(book.to_json(), words.to_json());
    }

    #[test]
    fn one_progress_entry_in_devanagari_gives_stamp_3() {
        let book = book_with(v1_state(), started("ka", 0));
        assert!(!book.is_flat());
        let text = book.to_json();
        assert!(text.contains("\"version\": 3"), "{text}");
        assert!(text.contains("\"words\""), "{text}");
        assert!(text.contains("\"devanagari\""), "{text}");
        assert_eq!(Book::from_json(&text).unwrap(), book);
    }

    #[test]
    fn one_added_word_in_devanagari_gives_stamp_3() {
        let only_a_word = with_added(State::empty(), vec![word("ka", "letter")]);
        let book = book_with(v1_state(), only_a_word);
        assert!(!book.is_flat());
        let text = book.to_json();
        assert!(text.contains("\"version\": 3"), "{text}");
        assert_eq!(Book::from_json(&text).unwrap(), book);
    }

    #[test]
    fn a_stamp_3_book_round_trips() {
        let book = book_with(State::empty(), started("ka", 2));
        let text = book.to_json();
        assert!(
            text.contains("\"words\""),
            "words is written even empty: {text}"
        );
        assert_eq!(Book::from_json(&text).unwrap(), book);
        assert_eq!(Book::from_json(&text).unwrap().to_json(), text);
    }

    #[test]
    fn an_unknown_deck_id_is_kept_as_it_is() {
        let mut book = book_with(v1_state(), started("ka", 0));
        book.set(
            id("zz-future"),
            with_added(started("zz", 1), vec![word("zz", "later")]),
        );
        let text = book.to_json();
        let read = Book::from_json(&text).unwrap();
        assert!(read.get(&id("zz-future")).is_some());
        assert_eq!(read.to_json(), text);
    }

    #[test]
    fn a_missing_words_entry_is_added_empty() {
        let text = r#"{"version":3,"decks":{"devanagari":{"progress":[]}}}"#;
        let book = Book::from_json(text).unwrap();
        assert_eq!(book.get(&DeckId::words()), Some(&State::empty()));
        assert!(book.get(&id("devanagari")).is_some());
        assert_eq!(book.decks().count(), 2);
    }

    // ---- The older build ----

    #[test]
    fn an_older_build_refuses_a_stamp_3_file() {
        let text = v3_text();
        assert_eq!(
            State::from_json_upto(&text, 2),
            Err(StateError::UnknownVersion(3))
        );
        assert_eq!(
            Book::from_json_upto(&text, 2),
            Err(StateError::UnknownVersion(3))
        );
        let flat = v2_state().to_json();
        assert!(Book::from_json_upto(&flat, 2).is_ok());
    }

    // ---- Refusals ----

    #[test]
    fn version_0_is_refused() {
        assert_eq!(
            Book::from_json(r#"{"version":0,"progress":[]}"#),
            Err(StateError::UnknownVersion(0))
        );
    }

    #[test]
    fn version_4_is_refused_whatever_its_shape() {
        assert_eq!(
            Book::from_json(r#"{"version":4,"other":"shape"}"#),
            Err(StateError::UnknownVersion(4))
        );
    }

    #[test]
    fn bad_json_is_refused() {
        assert!(matches!(
            Book::from_json("{not json"),
            Err(StateError::Json(_))
        ));
    }

    #[test]
    fn a_term_twice_in_an_entry_is_refused() {
        let twice = state(
            None,
            vec![
                entry("ka", 0, Stage::Day, 0, &[]),
                entry("ka", 1, Stage::Day, 0, &[]),
            ],
        );
        let entry_text = serde_json::to_string(&twice).unwrap();
        let body = entry_text.replace("\"version\":1,", "");
        let text = format!(r#"{{"version":3,"decks":{{"devanagari":{body}}}}}"#);
        assert_eq!(
            Book::from_json(&text),
            Err(StateError::DuplicateTerm(TermId("ka".to_string())))
        );
    }

    #[test]
    fn an_added_word_twice_in_an_entry_is_refused() {
        let twice = with_added(State::empty(), vec![word("ka", "one"), word("ka", "two")]);
        let entry_text = serde_json::to_string(&twice).unwrap();
        let body = entry_text.replace("\"version\":2,", "");
        let text = format!(r#"{{"version":3,"decks":{{"devanagari":{body}}}}}"#);
        assert_eq!(
            Book::from_json(&text),
            Err(StateError::DuplicateTerm(TermId("ka".to_string())))
        );
    }

    // ---- Access ----

    #[test]
    fn the_state_of_an_absent_deck_is_empty() {
        let book = Book::empty();
        assert_eq!(book.state(&id("devanagari")), State::empty());
        assert_eq!(book.get(&id("devanagari")), None);
    }

    #[test]
    fn set_then_state_round_trips() {
        let mut book = Book::empty();
        let ka = started("ka", 1);
        book.set(id("devanagari"), ka.clone());
        assert_eq!(book.state(&id("devanagari")), ka);
        assert_eq!(book.get(&id("devanagari")), Some(&ka));
        book.set(DeckId::words(), v1_state());
        assert_eq!(book.words(), &v1_state());
    }

    #[test]
    fn the_decks_come_sorted_by_id() {
        let mut book = book_with(State::empty(), State::empty());
        book.set(id("aaa"), State::empty());
        let ids: Vec<&str> = book
            .decks()
            .map(|(deck_id, _)| deck_id.0.as_str())
            .collect();
        assert_eq!(ids, ["aaa", "devanagari", "words"]);
    }

    #[test]
    fn a_deck_id_knows_the_words_deck() {
        assert!(DeckId::words().is_words());
        assert!(!id("devanagari").is_words());
        assert_eq!(DeckId::words().to_string(), "words");
    }

    #[test]
    fn shadow_added_drops_from_the_named_entry_only() {
        let both = with_added(
            State::empty(),
            vec![word("ka", "letter"), word("kha", "letter")],
        );
        let mut book = book_with(both.clone(), both);
        let taught = deck(vec![indeclinable("ka", 1)]);
        assert_eq!(book.shadow_added(&id("devanagari"), &taught), 1);
        assert_eq!(book.state(&id("devanagari")).added.len(), 1);
        assert_eq!(book.words().added.len(), 2);
        assert_eq!(book.shadow_added(&id("absent"), &taught), 0);
    }

    // ---- Merge ----

    #[test]
    fn a_deck_only_one_side_has_is_kept_whole() {
        let local = book_with(v1_state(), started("ka", 2));
        let remote = Book::from_state(v1_state());
        let kept = local.get(&id("devanagari")).unwrap().clone();
        for merged in [merge_books(&local, &remote), merge_books(&remote, &local)] {
            assert_eq!(merged.get(&id("devanagari")), Some(&kept));
            assert_eq!(merged.decks().count(), 2);
        }
    }

    #[test]
    fn an_unknown_id_is_kept_by_a_merge() {
        let mut local = Book::from_state(v1_state());
        local.set(id("zz-future"), started("zz", 1));
        let remote = Book::from_state(v1_state());
        let kept = local.get(&id("zz-future")).unwrap().clone();
        for merged in [merge_books(&local, &remote), merge_books(&remote, &local)] {
            assert_eq!(merged.get(&id("zz-future")), Some(&kept));
        }
    }

    #[test]
    fn words_and_devanagari_are_joined_separately() {
        let local = book_with(started("sati 1.1", 0), started("ka", 0));
        let remote = book_with(started("sati 1.1", 1), started("ka", 0));
        let merged = merge_books(&local, &remote);
        assert_eq!(
            merged
                .words()
                .get(&TermId("sati 1.1".into()))
                .unwrap()
                .lapses,
            1
        );
        assert_eq!(
            merged
                .state(&id("devanagari"))
                .get(&TermId("ka".into()))
                .unwrap()
                .lapses,
            0
        );
        for name in ["words", "devanagari"] {
            let (a, b) = (local.state(&id(name)), remote.state(&id(name)));
            assert_eq!(merged.state(&id(name)), merge(&a, &b), "{name}");
        }
    }

    #[test]
    fn merging_keeps_the_stamp_monotone() {
        let deep = book_with(v1_state(), started("ka", 0));
        let flat = Book::from_state(v1_state());
        assert!(!deep.is_flat() && flat.is_flat());
        assert!(!merge_books(&deep, &flat).is_flat());
        assert!(!merge_books(&flat, &deep).is_flat());
    }

    // ---- Laws, per deck, over generated Books ----

    /// A `SplitMix64` generator (the same as merge.rs): the cases are the
    /// same at every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }

        fn index(&mut self, bound: usize) -> usize {
            usize::try_from(self.below(bound as u64)).unwrap()
        }
    }

    /// A state over a pool of five terms and three possible added words,
    /// values from small ranges so equal keys come up often.
    fn generated_state(rng: &mut Rng) -> State {
        let mut pool: Vec<&str> = vec!["a", "b", "c", "d", "e"];
        for end in (1..pool.len()).rev() {
            let at = rng.index(end + 1);
            pool.swap(end, at);
        }
        let mut progress = Vec::new();
        for term in pool {
            if rng.below(3) == 0 {
                continue;
            }
            let stage = [Stage::Day, Stage::Week, Stage::Year][rng.index(3)];
            let passed: Vec<Facet> = Facet::ALL
                .into_iter()
                .filter(|_| rng.below(2) == 0)
                .collect();
            let lapses = u32::try_from(rng.below(2)).unwrap();
            let due = u32::try_from(rng.below(2)).unwrap();
            progress.push(entry(term, lapses, stage, due, &passed));
        }
        let last = match rng.below(3) {
            0 => None,
            n => Some(u32::try_from(n).unwrap()),
        };
        let mut added = Vec::new();
        for term in ["u", "v", "w"] {
            if rng.below(2) == 0 {
                added.push(word(term, ["north", "south"][rng.index(2)]));
            }
        }
        if rng.below(2) == 0 {
            added.reverse();
        }
        with_added(state(last, progress), added)
    }

    /// A Book with `words` always and each other deck on a coin.
    fn generated(rng: &mut Rng) -> Book {
        let mut book = Book::from_state(generated_state(rng));
        for name in ["devanagari", "zz-future"] {
            if rng.below(3) != 0 {
                book.set(id(name), generated_state(rng));
            }
        }
        book
    }

    /// The Book with every entry's terms in a fixed order: equality as maps.
    fn canonical(book: &Book) -> BTreeMap<DeckId, State> {
        book.decks()
            .map(|(deck_id, entry)| {
                let mut entry = entry.clone();
                entry.progress.sort_by(|x, y| x.term.cmp(&y.term));
                entry.added.sort_by(|x, y| x.id.cmp(&y.id));
                (deck_id.clone(), entry)
            })
            .collect()
    }

    const CASES: u64 = 2000;

    #[test]
    fn merge_books_is_commutative() {
        let mut rng = Rng(11);
        for _ in 0..CASES {
            let (a, b) = (generated(&mut rng), generated(&mut rng));
            assert_eq!(
                canonical(&merge_books(&a, &b)),
                canonical(&merge_books(&b, &a)),
                "{a:?} {b:?}"
            );
        }
    }

    #[test]
    fn merge_books_is_associative() {
        let mut rng = Rng(12);
        for _ in 0..CASES {
            let (a, b, c) = (
                generated(&mut rng),
                generated(&mut rng),
                generated(&mut rng),
            );
            assert_eq!(
                canonical(&merge_books(&merge_books(&a, &b), &c)),
                canonical(&merge_books(&a, &merge_books(&b, &c))),
                "{a:?} {b:?} {c:?}"
            );
        }
    }

    #[test]
    fn merge_books_is_idempotent() {
        let mut rng = Rng(13);
        for _ in 0..CASES {
            let (a, b) = (generated(&mut rng), generated(&mut rng));
            assert_eq!(merge_books(&a, &a), a);
            let once = merge_books(&a, &b);
            assert_eq!(merge_books(&once, &b), once, "{a:?} {b:?}");
        }
    }
}
