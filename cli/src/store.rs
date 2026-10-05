//! The state file: load and atomic save.
//!
//! A missing file is the first run (an empty state). A file that cannot be
//! read, or that [`State::from_json`] refuses, stops the command: it is
//! never overwritten. A write error is fatal for the caller, never skipped.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

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

/// Reads the state file; a missing file gives [`State::empty`].
///
/// # Errors
/// [`StoreError::Io`] when the file cannot be read, [`StoreError::Invalid`]
/// when it is not a valid state. The file is left as it was.
pub fn load_state(path: &Path) -> Result<State, StoreError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(State::empty()),
        Err(error) => return Err(io_error(path, &error)),
    };
    State::from_json(&text).map_err(|error| StoreError::Invalid {
        path: path.to_path_buf(),
        cause: error.to_string(),
    })
}

/// Writes the state to a temporary file beside `path`, then renames it
/// over `path`.
///
/// # Errors
/// [`StoreError::Io`] on any failure; the caller must stop.
pub fn save_state(path: &Path, state: &State) -> Result<(), StoreError> {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    let written = fs::write(&temp, state.to_json()).and_then(|()| fs::rename(&temp, path));
    written.map_err(|error| {
        let _ = fs::remove_file(&temp);
        io_error(path, &error)
    })
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
        assert_eq!(load_state(&path).unwrap(), State::empty());
        assert!(!path.exists(), "loading never creates the file");
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = dir("round").join("state.json");
        let state = sample();
        save_state(&path, &state).unwrap();
        assert_eq!(load_state(&path).unwrap(), state);
        assert_eq!(fs::read_to_string(&path).unwrap(), state.to_json());
    }

    #[test]
    fn save_replaces_an_older_file_and_leaves_no_temp_file() {
        let folder = dir("replace");
        let path = folder.join("state.json");
        save_state(&path, &State::empty()).unwrap();
        save_state(&path, &sample()).unwrap();
        assert_eq!(load_state(&path).unwrap(), sample());
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
        let error = load_state(&path).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
        assert!(error.to_string().contains("state.json"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn an_unknown_version_is_invalid() {
        let path = dir("version").join("state.json");
        let text = r#"{"version": 99, "progress": []}"#;
        fs::write(&path, text).unwrap();
        let error = load_state(&path).unwrap_err();
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
        let error = load_state(&path).unwrap_err();
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
        let error = load_state(&path).unwrap_err();
        assert!(matches!(error, StoreError::Io { .. }), "{error}");
        assert!(error.to_string().contains("state.json"), "{error}");
    }

    #[test]
    fn saving_into_a_missing_directory_is_an_io_error() {
        let path = dir("missing").join("nowhere").join("state.json");
        let error = save_state(&path, &sample()).unwrap_err();
        assert!(matches!(error, StoreError::Io { .. }), "{error}");
        assert!(error.to_string().contains("state.json"), "{error}");
    }
}
