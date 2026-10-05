//! The persisted progress state.
//!
//! A [`State`] is what a device keeps between sessions:
//! `{version, last_today, progress: [{term, stage, passed, due, lapses}]}`,
//! pretty JSON with dates as `YYYY-MM-DD`. It holds one [`Progress`] per term
//! the learner has started, in the order they were started. A term the deck
//! does not know (a different or newer deck) is kept as it is, never asked.
//!
//! `passed` is what has been answered right at the current stage. It is read
//! through [`State::effective_passed`] as `passed ∩ facets_of(term)`: a deck
//! that gains a facet makes the term ask it at its next due, and one that
//! loses a facet does not block the stage.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::deck::{Term, TermId};
use crate::facet::{FacetSet, facets_of};
use crate::stage::Stage;

/// Where one term stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// The term (a deck term id).
    pub term: TermId,
    /// The current Leitner stage.
    pub stage: Stage,
    /// The facets answered right at this stage since the last transition.
    pub passed: FacetSet,
    /// The first day the term is asked again: it is due when `due <= today`.
    pub due: Date,
    /// How many times the term was failed.
    pub lapses: u32,
}

impl Progress {
    /// A term started on `today`: stage Day, nothing passed, due `today`,
    /// no lapse.
    pub fn new(term: TermId, today: Date) -> Progress {
        Progress {
            term,
            stage: Stage::Day,
            passed: FacetSet::empty(),
            due: today,
            lapses: 0,
        }
    }
}

/// Why a state could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// The text is not a state (the parser's message).
    Json(String),
    /// The `version` is not [`State::VERSION`].
    UnknownVersion(u32),
    /// The same term has two entries.
    DuplicateTerm(TermId),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Json(message) => write!(f, "not a state file: {message}"),
            StateError::UnknownVersion(version) => write!(
                f,
                "state version {version} is not supported (this build reads version {})",
                State::VERSION
            ),
            StateError::DuplicateTerm(id) => write!(f, "term {id} appears twice in the state"),
        }
    }
}

impl std::error::Error for StateError {}

/// The whole progress file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// The format version; [`State::VERSION`].
    pub version: u32,
    /// The latest `today` a session saw: a session refuses an earlier one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_today: Option<Date>,
    /// One entry per started term, in the order they were started.
    pub progress: Vec<Progress>,
}

impl State {
    /// The only format version this crate reads and writes.
    pub const VERSION: u32 = 1;

    /// A state with no term started (the first run).
    pub fn empty() -> State {
        State {
            version: State::VERSION,
            last_today: None,
            progress: Vec::new(),
        }
    }

    /// Reads a state, refusing an unknown version and a term twice.
    ///
    /// # Errors
    /// [`StateError::Json`], [`StateError::UnknownVersion`] or
    /// [`StateError::DuplicateTerm`].
    pub fn from_json(text: &str) -> Result<State, StateError> {
        let json_error = |error: serde_json::Error| StateError::Json(error.to_string());
        // The version is read first: a future version may have another shape.
        let raw: serde_json::Value = serde_json::from_str(text).map_err(json_error)?;
        if let Some(version) = raw.get("version").and_then(serde_json::Value::as_u64)
            && version != u64::from(State::VERSION)
        {
            return Err(StateError::UnknownVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let state: State = serde_json::from_value(raw).map_err(json_error)?;
        let mut seen = HashSet::new();
        for entry in &state.progress {
            if !seen.insert(&entry.term) {
                return Err(StateError::DuplicateTerm(entry.term.clone()));
            }
        }
        Ok(state)
    }

    /// The state as pretty JSON; entries keep their insertion order, so the
    /// same state always gives the same text.
    ///
    /// # Panics
    /// Never in practice: the types serialise without failing.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a state always serialises")
    }

    /// The entry of a term, if it was started.
    pub fn get(&self, term: &TermId) -> Option<&Progress> {
        self.progress.iter().find(|entry| &entry.term == term)
    }

    /// What counts as passed for this term now: `passed ∩ facets_of(term)`.
    pub fn effective_passed(progress: &Progress, term: &Term) -> FacetSet {
        progress.passed.intersection(&facets_of(term))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::Deck;
    use crate::deck::samples::{deck, example, form, indeclinable, noun};
    use crate::facet::Facet;

    fn day(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    fn id(text: &str) -> TermId {
        TermId(text.to_string())
    }

    fn two_terms() -> State {
        State {
            version: State::VERSION,
            last_today: Some(day("2026-10-05")),
            progress: vec![
                Progress {
                    term: id("sati 1.1"),
                    stage: Stage::Week,
                    passed: [Facet::Meaning].into_iter().collect(),
                    due: day("2026-10-12"),
                    lapses: 1,
                },
                Progress::new(id("kho"), day("2026-10-05")),
            ],
        }
    }

    #[test]
    fn a_new_progress_is_day_due_today_with_nothing_passed() {
        let progress = Progress::new(id("kho"), day("2026-10-05"));
        assert_eq!(progress.stage, Stage::Day);
        assert_eq!(progress.due, day("2026-10-05"));
        assert_eq!(progress.passed, FacetSet::empty());
        assert_eq!(progress.lapses, 0);
    }

    #[test]
    fn the_empty_state_has_version_one_and_no_term() {
        let state = State::empty();
        assert_eq!(state.version, 1);
        assert_eq!(state.last_today, None);
        assert_eq!(state.progress.len(), 0);
        assert_eq!(State::from_json(&state.to_json()).unwrap(), state);
    }

    #[test]
    fn a_state_with_two_terms_round_trips() {
        let state = two_terms();
        assert_eq!(State::from_json(&state.to_json()).unwrap(), state);
    }

    #[test]
    fn the_json_shape_is_the_documented_one() {
        let value: serde_json::Value = serde_json::from_str(&two_terms().to_json()).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["last_today"], "2026-10-05");
        assert_eq!(value["progress"][0]["term"], "sati 1.1");
        assert_eq!(value["progress"][0]["stage"], "Week");
        assert_eq!(
            value["progress"][0]["passed"],
            serde_json::json!(["Meaning"])
        );
        assert_eq!(value["progress"][0]["due"], "2026-10-12");
        assert_eq!(value["progress"][0]["lapses"], 1);
        assert_eq!(value["progress"][1]["passed"], serde_json::json!([]));
    }

    #[test]
    fn the_output_is_stable_and_keeps_insertion_order() {
        let state = two_terms();
        assert_eq!(state.to_json(), state.to_json());
        let json = state.to_json();
        assert!(json.find("sati 1.1").unwrap() < json.find("kho").unwrap());
        assert!(json.contains('\n'), "pretty printed");
    }

    #[test]
    fn last_today_is_optional() {
        let text = r#"{"version":1,"progress":[]}"#;
        assert_eq!(State::from_json(text).unwrap().last_today, None);
        assert!(!State::empty().to_json().contains("last_today"));
    }

    #[test]
    fn an_unknown_version_is_refused_before_the_shape() {
        assert_eq!(
            State::from_json(r#"{"version":2,"progress":[]}"#),
            Err(StateError::UnknownVersion(2))
        );
        assert_eq!(
            State::from_json(r#"{"version":9,"anything":true}"#),
            Err(StateError::UnknownVersion(9))
        );
    }

    #[test]
    fn a_term_twice_is_refused() {
        let mut state = two_terms();
        state
            .progress
            .push(Progress::new(id("sati 1.1"), day("2026-10-05")));
        assert_eq!(
            State::from_json(&state.to_json()),
            Err(StateError::DuplicateTerm(id("sati 1.1")))
        );
    }

    #[test]
    fn text_that_is_not_a_state_is_a_json_error() {
        for text in [
            "",
            "{}",
            "[]",
            r#"{"version":1}"#,
            r#"{"version":1,"progress":[{"term":"a"}]}"#,
        ] {
            assert!(
                matches!(State::from_json(text), Err(StateError::Json(_))),
                "{text:?}"
            );
        }
        let bad_date = two_terms().to_json().replace("2026-10-12", "2026-13-12");
        assert!(matches!(
            State::from_json(&bad_date),
            Err(StateError::Json(_))
        ));
    }

    #[test]
    fn each_error_says_what_is_wrong() {
        assert!(StateError::UnknownVersion(3).to_string().contains('3'));
        assert!(
            StateError::DuplicateTerm(id("kho"))
                .to_string()
                .contains("kho")
        );
        assert_ne!(StateError::Json("boom".to_string()).to_string(), "");
    }

    #[test]
    fn get_finds_a_started_term_only() {
        let state = two_terms();
        assert_eq!(state.get(&id("kho")).unwrap().stage, Stage::Day);
        assert!(state.get(&id("missing")).is_none());
    }

    #[test]
    fn a_state_loads_against_a_deck_of_the_same_terms() {
        // The pair as the CLI reads it: both documents round-trip together.
        let d: Deck = deck(vec![noun("sati 1.1", 1), indeclinable("kho", 2)]);
        assert_eq!(Deck::from_json(&d.to_json()).unwrap(), d);
        assert!(
            two_terms()
                .progress
                .iter()
                .all(|p| d.term(&p.term).is_some())
        );
    }

    #[test]
    fn effective_passed_is_the_intersection_with_the_terms_facets() {
        let mut progress = Progress::new(id("x"), day("2026-10-05"));
        progress.passed = [Facet::Meaning, Facet::Forms, Facet::Context]
            .into_iter()
            .collect();

        // The term still has every facet: nothing is dropped.
        let full = noun("x", 1);
        assert_eq!(State::effective_passed(&progress, &full), progress.passed);

        // The deck lost Forms and Context: only Meaning counts, so the stage
        // is not blocked by a facet that cannot be asked any more.
        let lost = indeclinable("x", 1);
        let expected: FacetSet = [Facet::Meaning].into_iter().collect();
        assert_eq!(State::effective_passed(&progress, &lost), expected);

        // The deck gained a facet the learner never passed: it stays unpassed.
        let mut gained = indeclinable("x", 1);
        gained.forms.push(form("nom sg", "x"));
        gained.example = Some(example(true));
        let effective = State::effective_passed(&progress, &gained);
        assert!(effective.contains(Facet::Forms) && effective.contains(Facet::Context));
        let empty = Progress::new(id("x"), day("2026-10-05"));
        assert_eq!(State::effective_passed(&empty, &gained), FacetSet::empty());
    }
}
