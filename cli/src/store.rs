//! The state file: load and atomic save.
//!
//! The file is a [`Book`] (one state per deck id); a flat file is its
//! `words` entry. A missing file is the first run (an empty state). A file
//! that cannot be read, or that [`Book::from_json`] refuses, stops the command: it is
//! never overwritten. A write error is fatal for the caller, never skipped.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use learn_core::book::{Book, DeckId};
use learn_core::progress::State;

/// Why the state file could not be read or written.
#[derive(Debug)]
pub enum StoreError {
    /// A read, write or rename failed.
    Io {
        /// The state file.
        path: PathBuf,
        /// The system's message.
        cause: String,
    },
    /// The file was read but is not a valid state.
    Invalid {
        /// The state file.
        path: PathBuf,
        /// What is wrong with it.
        cause: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io { path, cause } => {
                write!(f, "state file {}: {cause}", path.display())
            }
            StoreError::Invalid { path, cause } => write!(
                f,
                "state file {} is not usable and was left untouched: {cause}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for StoreError {}

/// Reads the progress file as a Book: a missing file is [`Book::empty`].
fn load_book(path: &Path) -> Result<Book, StoreError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Book::empty()),
        Err(error) => return Err(io_error(path, &error)),
    };
    Book::from_json(&text).map_err(|error| StoreError::Invalid {
        path: path.to_path_buf(),
        cause: error.to_string(),
    })
}

/// Reads the entry of deck `id` from the progress file (a Book; a flat
/// file is its `words` entry). A missing file, or a deck the file lacks,
/// gives [`State::empty`].
///
/// # Errors
/// [`StoreError::Io`] when the file cannot be read, [`StoreError::Invalid`]
/// when it is not a valid state or Book, or of a newer version. The file is
/// left as it was.
pub fn load_state(path: &Path, id: &DeckId) -> Result<State, StoreError> {
    Ok(load_book(path)?.state(id))
}

/// Sets the entry of deck `id` in the progress file, keeping every other
/// entry, and writes the Book to a temporary file beside `path`, then
/// renames it over `path`. The Book is written in the flat format while
/// only `words` holds progress ([`Book::to_json`]).
///
/// # Errors
/// [`StoreError::Invalid`] when the file is there but unusable (it is never
/// overwritten), [`StoreError::Io`] on any other failure; the caller must
/// stop.
pub fn save_state(path: &Path, id: &DeckId, state: &State) -> Result<(), StoreError> {
    let mut book = load_book(path)?;
    book.set(id.clone(), state.clone());
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    let written = fs::write(&temp, book.to_json()).and_then(|()| fs::rename(&temp, path));
    written.map_err(|error| {
        let _ = fs::remove_file(&temp);
        io_error(path, &error)
    })
}

/// A state file and the Book entry a command acts on.
#[derive(Debug, Clone, Copy)]
pub struct StateSlot<'a> {
    /// The state file.
    pub path: &'a Path,
    /// The deck whose entry is read and written (`--deck-id`).
    pub id: &'a DeckId,
}

impl StateSlot<'_> {
    /// [`load_state`] on this slot.
    ///
    /// # Errors
    /// As [`load_state`].
    pub fn load(&self) -> Result<State, StoreError> {
        load_state(self.path, self.id)
    }

    /// [`save_state`] on this slot.
    ///
    /// # Errors
    /// As [`save_state`].
    pub fn save(&self, state: &State) -> Result<(), StoreError> {
        save_state(self.path, self.id, state)
    }
}

fn io_error(path: &Path, error: &std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.to_path_buf(),
        cause: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use learn_core::date::Date;
    use learn_core::deck::TermId;
    use learn_core::progress::Progress;
    use std::fs;

    fn words() -> DeckId {
        DeckId::words()
    }

    /// A fresh directory per test, under the system's temporary folder.
    fn dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir()
            .join(format!("learn-cli-store-{}", std::process::id()))
            .join(name);
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn sample() -> State {
        let today = Date::parse("2026-10-05").unwrap();
        let mut state = State::empty();
        state.last_today = Some(today);
        state
            .progress
            .push(Progress::new(TermId("sati 1.1".to_owned()), today));
        state
    }

    #[test]
    fn an_absent_file_is_the_empty_state() {
        let path = dir("absent").join("state.json");
        assert_eq!(load_state(&path, &words()).unwrap(), State::empty());
        assert!(!path.exists(), "loading never creates the file");
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = dir("round").join("state.json");
        let state = sample();
        save_state(&path, &words(), &state).unwrap();
        assert_eq!(load_state(&path, &words()).unwrap(), state);
        assert_eq!(fs::read_to_string(&path).unwrap(), state.to_json());
    }

    #[test]
    fn save_replaces_an_older_file_and_leaves_no_temp_file() {
        let folder = dir("replace");
        let path = folder.join("state.json");
        save_state(&path, &words(), &State::empty()).unwrap();
        save_state(&path, &words(), &sample()).unwrap();
        assert_eq!(load_state(&path, &words()).unwrap(), sample());
        let names: Vec<_> = fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("state.json")]);
    }

    #[test]
    fn corrupt_json_is_invalid_and_the_file_is_untouched() {
        let path = dir("corrupt").join("state.json");
        let bytes = b"{ not json at all";
        fs::write(&path, bytes).unwrap();
        let error = load_state(&path, &words()).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
        assert!(error.to_string().contains("state.json"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn an_unknown_version_is_invalid() {
        let path = dir("version").join("state.json");
        let text = r#"{"version": 99, "progress": []}"#;
        fs::write(&path, text).unwrap();
        let error = load_state(&path, &words()).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
        assert!(error.to_string().contains("99"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn a_term_twice_is_invalid() {
        let path = dir("twice").join("state.json");
        let today = Date::parse("2026-10-05").unwrap();
        let mut state = State::empty();
        for _ in 0..2 {
            state
                .progress
                .push(Progress::new(TermId("sati 1.1".to_owned()), today));
        }
        let text = state.to_json();
        fs::write(&path, &text).unwrap();
        let error = load_state(&path, &words()).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
        assert!(error.to_string().contains("sati 1.1"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn an_unreadable_file_is_an_io_error() {
        // A directory where the file should be: reading it fails, and it is
        // not "absent".
        let path = dir("unreadable").join("state.json");
        fs::create_dir(&path).unwrap();
        let error = load_state(&path, &words()).unwrap_err();
        assert!(matches!(error, StoreError::Io { .. }), "{error}");
        assert!(error.to_string().contains("state.json"), "{error}");
    }

    #[test]
    fn saving_into_a_missing_directory_is_an_io_error() {
        let path = dir("missing").join("nowhere").join("state.json");
        let error = save_state(&path, &words(), &sample()).unwrap_err();
        assert!(matches!(error, StoreError::Io { .. }), "{error}");
        assert!(error.to_string().contains("state.json"), "{error}");
    }

    fn zz() -> DeckId {
        DeckId("zz".to_owned())
    }

    #[test]
    fn a_words_only_save_stays_flat_and_equals_the_states_own_text() {
        let path = dir("flat").join("state.json");
        save_state(&path, &words(), &sample()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, sample().to_json());
        assert!(!text.contains("decks"), "{text}");
    }

    #[test]
    fn saving_a_deck_with_progress_writes_stamp_3_and_each_deck_reads_back() {
        let path = dir("stamp3").join("state.json");
        save_state(&path, &words(), &sample()).unwrap();
        save_state(&path, &zz(), &sample()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 3"), "{text}");
        assert_eq!(load_state(&path, &zz()).unwrap(), sample());
        assert_eq!(load_state(&path, &words()).unwrap(), sample());
        assert_eq!(
            load_state(&path, &DeckId("other".to_owned())).unwrap(),
            State::empty()
        );
    }

    #[test]
    fn saving_one_deck_keeps_the_other_entries_whole() {
        let path = dir("keeps").join("state.json");
        save_state(&path, &zz(), &sample()).unwrap();
        save_state(&path, &words(), &State::empty()).unwrap();
        assert_eq!(load_state(&path, &zz()).unwrap(), sample());
        let mut other = sample();
        other.last_today = None;
        other.progress.clear();
        save_state(&path, &words(), &other).unwrap();
        assert_eq!(load_state(&path, &zz()).unwrap(), sample());
    }

    #[test]
    fn a_deck_missing_from_the_file_is_empty_and_loading_writes_nothing() {
        let path = dir("absent-deck").join("state.json");
        save_state(&path, &words(), &sample()).unwrap();
        let before = fs::read(&path).unwrap();
        assert_eq!(load_state(&path, &zz()).unwrap(), State::empty());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_refused_file_is_never_overwritten_by_a_save() {
        for (name, text) in [
            ("stamp4", r#"{"version": 4, "decks": {}}"#),
            ("garbage", "{ not json"),
        ] {
            let path = dir(name).join("state.json");
            fs::write(&path, text).unwrap();
            let error = save_state(&path, &zz(), &sample()).unwrap_err();
            assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
            assert_eq!(fs::read_to_string(&path).unwrap(), text, "{name}");
            assert!(matches!(
                load_state(&path, &zz()),
                Err(StoreError::Invalid { .. })
            ));
        }
    }
}
