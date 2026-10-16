//! The learner's progress as the Learn fragment sees it: what a store
//! holds ([`Load`]) and the pure decisions on a stored or imported text
//! ([`classify`], [`validate_import`], [`export_name`]), tested natively.
//!
//! Where the progress lives is the host's choice ([`crate::backend`]): the
//! site keeps it in `localStorage[pc_learn]` ([`crate::web`]), the app in
//! a file. The state is `learn_core::State`'s JSON either way.
//!
//! The deck is never authoritative over the state: a term the deck does not
//! know stays (an orphan), listed apart, never counted.

use learn_core::date::Date;
use learn_core::progress::{State, StateError};

/// The `localStorage` key of the progress.
pub(crate) const KEY: &str = "pc_learn";

/// What a [`ProgressStore`](crate::backend::ProgressStore) holds, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// A state this build reads, with at least one entry.
    Ok(State),
    /// Nothing yet: no key, or a state with no entry.
    Empty,
    /// Not a state this build reads (bad JSON, version 0, a duplicate
    /// term); `raw` kept for 'Download the raw copy'.
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

/// Why an imported file was refused; the progress is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportError {
    /// Not a state (bad JSON, or a version below this build's).
    NotAState,
    /// A state saved by a newer version.
    Newer,
    /// The same term twice.
    DuplicateTerm,
}

/// What the stored text `raw` is (`None`: no key). A valid state is
/// [`Load::Ok`] even with no entry: [`Load::Empty`] is the absence of the
/// key alone, and the home decides what an empty state shows. Never
/// [`Load::Unreadable`]: that is a read that failed, not a text. A host's
/// store that reads a text itself (the app's file) classifies it with this.
pub fn classify(raw: Option<&str>) -> Load {
    let Some(raw) = raw else {
        return Load::Empty;
    };
    match State::from_json(raw) {
        Ok(state) => Load::Ok(state),
        // `from_json` reads the `version` before the shape: a newer
        // version is told apart whatever else it holds.
        Err(StateError::UnknownVersion(version)) if version > State::VERSION => {
            Load::NewerVersion {
                raw: raw.to_owned(),
            }
        }
        Err(_) => Load::Invalid {
            raw: raw.to_owned(),
        },
    }
}

/// An imported file's text as a whole state, or why it is refused; never
/// a partial state.
///
/// # Errors
/// [`ImportError::NotAState`] (not JSON, not a state, or a version below
/// this build's), [`ImportError::Newer`] (a version above), or
/// [`ImportError::DuplicateTerm`].
pub(crate) fn validate_import(text: &str) -> Result<State, ImportError> {
    State::from_json(text).map_err(|error| match error {
        StateError::UnknownVersion(version) if version > State::VERSION => ImportError::Newer,
        StateError::DuplicateTerm(_) => ImportError::DuplicateTerm,
        StateError::Json(_) | StateError::UnknownVersion(_) => ImportError::NotAState,
    })
}

/// The export's file name: `pc_learn-<YYYY-MM-DD>.json`.
pub(crate) fn export_name(date: Date) -> String {
    format!("{KEY}-{date}.json")
}

#[cfg(test)]
mod tests {
    use super::{ImportError, Load, classify, export_name, validate_import};
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

    const NEWER: &str = r#"{"version": 2, "something": "else"}"#;
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
        assert_eq!(classify(Some(&empty.to_json())), Load::Ok(empty));
        let state = one_term_state();
        assert_eq!(classify(Some(&state.to_json())), Load::Ok(state));
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
        assert_eq!(validate_import(&state.to_json()), Ok(state));
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
    }

    #[test]
    fn an_import_refuses_a_duplicate_term() {
        assert_eq!(
            validate_import(&duplicate()),
            Err(ImportError::DuplicateTerm)
        );
    }

    #[test]
    fn the_export_name_carries_the_date() {
        let date = Date::parse("2026-10-05").unwrap();
        assert_eq!(export_name(date), "pc_learn-2026-10-05.json");
        assert_eq!(export_name(Date::from_days(0)), "pc_learn-1970-01-01.json");
    }
}
