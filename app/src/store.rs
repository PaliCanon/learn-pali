//! The learner's progress as the Learn fragment sees it: what a store
//! holds ([`Load`]) and the pure decisions on a stored or imported text
//! ([`classify`], [`validate_import`], [`export_name`]), tested natively.
//!
//! Where the progress lives is the host's choice ([`crate::backend`]): the
//! site keeps it in `localStorage[pc_learn]` ([`crate::web`]), the app in
//! a file. The progress file is a [`Book`] (one `State` per deck); the
//! screens work on the selected deck's `State` through the
//! [`Backend`](crate::backend::Backend).
//!
//! The deck is never authoritative over the state: a term the deck does not
//! know stays (an orphan), listed apart, never counted.

use learn_core::book::{Book, DeckId};
use learn_core::date::Date;
use learn_core::progress::{State, StateError};

use crate::deck::deck;

/// The `localStorage` key of the progress.
pub(crate) const KEY: &str = "pc_learn";

/// What a [`ProgressStore`](crate::backend::ProgressStore) holds, read. A
/// store holds the whole [`Book`]; the default type is the selected deck's
/// [`State`], which is what the screens read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load<T = State> {
    /// A progress this build reads, with at least one entry.
    Ok(T),
    /// Nothing yet: no key, or a state with no entry.
    Empty,
    /// Not a state (or a Book) this build reads (bad JSON, version 0, a
    /// duplicate term); `raw` kept for 'Download the raw copy'.
    Invalid {
        /// The stored text, as is.
        raw: String,
    },
    /// Saved by a newer version (A3: a version above this build's):
    /// read-only, never overwritten (Q3); `raw` kept for the download.
    NewerVersion {
        /// The stored text, as is.
        raw: String,
    },
    /// The read itself failed (an I/O error: permissions, a disk), the
    /// stored progress untouched. Not a file that parses badly (that is
    /// [`Load::Invalid`]). The home shows an error and a Reload button,
    /// Settings turns Export and Import off, and nothing is ever saved over
    /// what could not be read: the store refuses the write.
    Unreadable(String),
}

impl<T> Load<T> {
    /// The same answer with its progress turned by `f`; every other answer
    /// is unchanged.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Load<U> {
        match self {
            Load::Ok(value) => Load::Ok(f(value)),
            Load::Empty => Load::Empty,
            Load::Invalid { raw } => Load::Invalid { raw },
            Load::NewerVersion { raw } => Load::NewerVersion { raw },
            Load::Unreadable(reason) => Load::Unreadable(reason),
        }
    }
}

/// Why an imported file was refused; the progress is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportError {
    /// Not a progress file (bad JSON, or a version below this build's).
    NotAState,
    /// A progress saved by a newer version.
    Newer,
    /// The same term twice.
    DuplicateTerm,
}

/// What the stored text `raw` is (`None`: no key). A valid file is
/// [`Load::Ok`] even with no entry: [`Load::Empty`] is the absence of the
/// key alone, and the home decides what an empty state shows. Never
/// [`Load::Unreadable`]: that is a read that failed, not a text. A host's
/// store that reads a text itself (the app's file) classifies it with this.
/// A flat file (today's state) is a Book holding the `words` deck alone.
pub fn classify(raw: Option<&str>) -> Load<Book> {
    classify_as(raw, Book::VERSION)
}

/// [`classify`] as a build whose highest version is `newest` classifies:
/// the build before added words (1) holds a version-2 file read-only. An
/// added word the words deck now teaches is dropped: the deck's term
/// shadows it.
pub(crate) fn classify_as(raw: Option<&str>, newest: u32) -> Load<Book> {
    let Some(raw) = raw else {
        return Load::Empty;
    };
    match Book::from_json_upto(raw, newest) {
        Ok(mut book) => {
            book.shadow_added(&DeckId::words(), deck());
            Load::Ok(book)
        }
        // `from_json` reads the `version` before the shape: a newer
        // version is told apart whatever else it holds.
        Err(StateError::UnknownVersion(version)) if version > newest => Load::NewerVersion {
            raw: raw.to_owned(),
        },
        Err(_) => Load::Invalid {
            raw: raw.to_owned(),
        },
    }
}

/// An imported file's text as a whole Book (any version, a flat file
/// wrapped), or why it is refused; never a partial one.
///
/// # Errors
/// [`ImportError::NotAState`] (not JSON, not a progress, or a version below
/// this build's), [`ImportError::Newer`] (a version above), or
/// [`ImportError::DuplicateTerm`].
pub(crate) fn validate_import(text: &str) -> Result<Book, ImportError> {
    Book::from_json(text)
        .map(|mut book| {
            book.shadow_added(&DeckId::words(), deck());
            book
        })
        .map_err(|error| match error {
            StateError::UnknownVersion(version) if version > Book::VERSION => ImportError::Newer,
            StateError::DuplicateTerm(_) => ImportError::DuplicateTerm,
            StateError::Json(_) | StateError::UnknownVersion(_) => ImportError::NotAState,
        })
}

/// The `today` an export of `book` is named for: the later of `browser`
/// and every deck's `last_today` (Q2: a time zone moved west never goes
/// back). For a learner of the words deck alone, the state's own
/// [`session_today`](crate::today::session_today).
pub(crate) fn book_today(browser: Date, book: &Book) -> Date {
    book.decks()
        .filter_map(|(_, state)| state.last_today)
        .fold(browser, Date::max)
}

/// The export's file name: `pc_learn-<YYYY-MM-DD>.json`.
pub(crate) fn export_name(date: Date) -> String {
    format!("{KEY}-{date}.json")
}

#[cfg(test)]
mod tests {
    use super::{ImportError, Load, classify, classify_as, export_name, validate_import};
    use crate::test_support::added_word;
    use learn_core::book::{Book, DeckId};
    use learn_core::date::Date;
    use learn_core::deck::TermId;
    use learn_core::progress::{Progress, State};

    fn one_term_state() -> State {
        let today = Date::parse("2026-10-05").unwrap();
        let mut state = State::empty();
        state.last_today = Some(today);
        state
            .progress
            .push(Progress::new(TermId("sati 1.1".into()), today));
        state
    }

    /// A stamp this build does not read: the Book is 3.
    const NEWER: &str = r#"{"version": 4, "something": "else"}"#;

    /// A Book holding a started term in the `words` entry and another in
    /// the deck `zz`: written with stamp 3.
    fn two_deck_book() -> Book {
        let mut book = Book::from_state(one_term_state());
        let mut other = one_term_state();
        other.progress[0].term = TermId("zz-1".into());
        book.set(DeckId("zz".into()), other);
        book
    }
    /// A state that serialises well but holds one term twice.
    fn duplicate() -> String {
        let mut state = one_term_state();
        state.progress.push(state.progress[0].clone());
        state.to_json()
    }

    #[test]
    fn no_key_is_empty() {
        assert_eq!(classify(None), Load::Empty);
    }

    #[test]
    fn a_valid_state_is_ok_even_with_no_progress() {
        let empty = State::empty();
        assert_eq!(
            classify(Some(&empty.to_json())),
            Load::Ok(Book::from_state(empty))
        );
        let state = one_term_state();
        assert_eq!(
            classify(Some(&state.to_json())),
            Load::Ok(Book::from_state(state))
        );
    }

    /// A flat v2 text (today's file) is a Book with the `words` entry
    /// alone, equal to the state.
    #[test]
    fn a_flat_file_is_a_book_holding_words_alone() {
        let state = one_term_state();
        let Load::Ok(book) = classify(Some(&state.to_json())) else {
            panic!("a flat file reads");
        };
        assert_eq!(book.words(), &state);
        assert_eq!(book.decks().count(), 1);
    }

    /// A stamp-3 text is a Book kept whole; a stamp-4 text is a newer
    /// version, read-only, whatever its shape.
    #[test]
    fn a_stamp_three_text_is_a_book_and_a_stamp_four_text_is_newer() {
        let book = two_deck_book();
        let text = book.to_json();
        assert!(text.contains("\"version\": 3"), "{text}");
        assert_eq!(classify(Some(&text)), Load::Ok(book));
        assert_eq!(
            classify(Some(NEWER)),
            Load::NewerVersion { raw: NEWER.into() }
        );
    }

    #[test]
    fn bad_json_is_invalid_and_keeps_the_raw_text() {
        assert_eq!(
            classify(Some("{not json")),
            Load::Invalid {
                raw: "{not json".into()
            }
        );
        assert_eq!(classify(Some("")), Load::Invalid { raw: String::new() });
    }

    #[test]
    fn version_zero_is_invalid() {
        let raw = r#"{"version":0,"progress":[]}"#;
        assert_eq!(classify(Some(raw)), Load::Invalid { raw: raw.into() });
    }

    #[test]
    fn a_duplicate_term_is_invalid() {
        let raw = duplicate();
        assert_eq!(classify(Some(&raw)), Load::Invalid { raw });
    }

    #[test]
    fn a_newer_version_is_read_only_whatever_its_shape() {
        assert_eq!(
            classify(Some(NEWER)),
            Load::NewerVersion { raw: NEWER.into() }
        );
    }

    #[test]
    fn an_import_of_a_valid_file_is_the_whole_state() {
        let state = one_term_state();
        assert_eq!(
            validate_import(&state.to_json()),
            Ok(Book::from_state(state))
        );
    }

    /// An old flat backup returns the Book to flat: only `words`. A stamp-3
    /// file keeps every deck.
    #[test]
    fn an_import_of_a_flat_file_is_words_alone_and_of_a_book_every_deck() {
        let flat = validate_import(&one_term_state().to_json()).unwrap();
        assert_eq!(flat.decks().count(), 1);
        assert!(flat.get(&DeckId("zz".into())).is_none());
        let book = two_deck_book();
        let imported = validate_import(&book.to_json()).unwrap();
        assert_eq!(imported, book);
        assert_eq!(imported.decks().count(), 2);
    }

    #[test]
    fn an_import_refuses_what_is_not_a_state() {
        assert_eq!(validate_import("{nope"), Err(ImportError::NotAState));
        assert_eq!(validate_import("[1, 2]"), Err(ImportError::NotAState));
        assert_eq!(
            validate_import(r#"{"version":0,"progress":[]}"#),
            Err(ImportError::NotAState)
        );
    }

    #[test]
    fn an_import_refuses_a_newer_version() {
        assert_eq!(validate_import(NEWER), Err(ImportError::Newer));
        // A stamp-3 text of another shape is not a Book.
        assert_eq!(
            validate_import(r#"{"version": 3, "something": "else"}"#),
            Err(ImportError::NotAState)
        );
    }

    #[test]
    fn an_import_refuses_a_duplicate_term() {
        assert_eq!(
            validate_import(&duplicate()),
            Err(ImportError::DuplicateTerm)
        );
    }

    /// A version-2 file as a session wrote it, text and not `to_json`.
    const VERSION_TWO: &str = r#"{
  "version": 2,
  "last_today": "2026-10-17",
  "progress": [
    {"term": "kho", "stage": "Day", "passed": [], "due": "2026-10-18", "lapses": 0}
  ],
  "added": [
    {"id": "sānuvajja", "lemma": "sānuvajja", "pos": "adj",
     "meaning": {"en": "blameworthy"}, "forms": [], "rank": 4294967295}
  ]
}"#;

    #[test]
    fn a_version_two_file_with_an_added_word_loads() {
        match classify(Some(VERSION_TWO)) {
            Load::Ok(book) => {
                let state = book.words();
                assert_eq!(state.added.len(), 1);
                assert_eq!(state.added[0].id, TermId("sānuvajja".into()));
                assert_eq!(state.progress.len(), 1);
            }
            other => panic!("a version-2 file loads, got {other:?}"),
        }
    }

    #[test]
    fn the_build_before_added_words_keeps_a_version_two_file_read_only() {
        // That build's highest version is 1: it never overwrites the file.
        assert_eq!(
            classify_as(Some(VERSION_TWO), 1),
            Load::NewerVersion {
                raw: VERSION_TWO.into()
            }
        );
        assert!(matches!(classify_as(Some(VERSION_TWO), 2), Load::Ok(_)));
        let one = one_term_state().to_json();
        assert!(matches!(classify_as(Some(&one), 1), Load::Ok(_)));
    }

    /// A file holding the progress of `kho` and an added word of that id,
    /// which the deck now teaches, beside one the deck lacks.
    fn shadowed() -> State {
        let mut state = one_term_state();
        state.progress[0].term = TermId("kho".into());
        let mut word = added_word();
        word.id = TermId("kho".into());
        word.meaning.en = "the added word's sense".into();
        state.added.push(word);
        state.added.push(added_word());
        state
    }

    #[test]
    fn an_added_word_the_deck_now_teaches_is_shadowed_never_invalid() {
        let text = shadowed().to_json();
        let Load::Ok(book) = classify(Some(&text)) else {
            panic!("the file reads");
        };
        let state = book.words().clone();
        let kho = TermId("kho".into());
        assert_eq!(
            state.added,
            vec![added_word()],
            "only the deck's term is dropped"
        );
        assert_eq!(
            state.get(&kho),
            shadowed().get(&kho),
            "its progress is kept"
        );
        let served = state.term(crate::deck::deck(), &kho).unwrap();
        assert_eq!(
            Some(served),
            crate::deck::deck().term(&kho),
            "served by the deck"
        );
        // The next save has no trace of the shadowed word.
        let written: serde_json::Value = serde_json::from_str(&state.to_json()).unwrap();
        assert_eq!(written["added"].as_array().unwrap().len(), 1);
        assert_eq!(written["added"][0]["id"], "sānuvajja");
        // An import reads the same.
        assert_eq!(validate_import(&text), Ok(book));
    }

    #[test]
    fn a_file_whose_only_added_word_the_deck_teaches_is_written_back_as_version_one() {
        // A version-2 file holding one added word, which the deck now
        // teaches: read, the word is dropped and the next write is a file
        // every build reads.
        let mut state = one_term_state();
        state.progress[0].term = TermId("kho".into());
        let mut word = added_word();
        word.id = TermId("kho".into());
        state.added.push(word);
        let text = state.to_json();
        assert!(
            text.contains("\"version\": 2"),
            "it is written as 2: {text}"
        );
        let Load::Ok(book) = classify(Some(&text)) else {
            panic!("the file reads");
        };
        let read = book.words();
        assert_eq!(read.added.len(), 0);
        assert!(read.to_json().contains("\"version\": 1"), "now 1");
        let Ok(imported) = validate_import(&text) else {
            panic!("an import reads");
        };
        assert!(imported.to_json().contains("\"version\": 1"), "imported: 1");
    }

    #[test]
    fn the_same_added_word_twice_makes_the_file_invalid() {
        let mut state = one_term_state();
        state.added.push(added_word());
        state.added.push(added_word());
        let raw = state.to_json();
        assert_eq!(classify(Some(&raw)), Load::Invalid { raw });
        assert_eq!(
            validate_import(&state.to_json()),
            Err(ImportError::DuplicateTerm)
        );
    }

    #[test]
    fn map_turns_the_progress_and_keeps_every_other_answer() {
        assert_eq!(Load::Ok(2).map(|n| n + 1), Load::Ok(3));
        assert_eq!(Load::<u8>::Empty.map(u32::from), Load::Empty);
        let raw = String::from("x");
        assert_eq!(
            Load::<u8>::Invalid { raw: raw.clone() }.map(u32::from),
            Load::Invalid { raw: raw.clone() }
        );
        assert_eq!(
            Load::<u8>::NewerVersion { raw: raw.clone() }.map(u32::from),
            Load::NewerVersion { raw }
        );
        assert_eq!(
            Load::<u8>::Unreadable("denied".into()).map(u32::from),
            Load::Unreadable("denied".into())
        );
    }

    #[test]
    fn the_export_name_carries_the_date() {
        let date = Date::parse("2026-10-05").unwrap();
        assert_eq!(export_name(date), "pc_learn-2026-10-05.json");
        assert_eq!(export_name(Date::from_days(0)), "pc_learn-1970-01-01.json");
    }
}
