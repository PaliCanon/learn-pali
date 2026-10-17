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
use crate::deck::{Deck, Term, TermId};
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
    /// The `version` is 0 or above [`State::VERSION`].
    UnknownVersion(u32),
    /// The same term has two entries (or two added words).
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct State {
    /// The format version in memory: always [`State::VERSION`] (the stamp
    /// written is chosen by [`State::to_json`]).
    pub version: u32,
    /// The latest `today` a session saw: a session refuses an earlier one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_today: Option<Date>,
    /// One entry per started term, in the order they were started.
    pub progress: Vec<Progress>,
    /// The words the learner added to the practice from a passage, in the
    /// order they were added (see [`crate::added`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<Term>,
}

/// What a state is written as: the same fields, the stamp chosen by the
/// words it holds.
#[derive(Serialize)]
struct Written<'a> {
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_today: Option<Date>,
    progress: &'a [Progress],
    #[serde(skip_serializing_if = "<[Term]>::is_empty")]
    added: &'a [Term],
}

impl Serialize for State {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Written {
            version: self.stamp(),
            last_today: self.last_today,
            progress: &self.progress,
            added: &self.added,
        }
        .serialize(serializer)
    }
}

impl State {
    /// The highest format version this crate reads (1 and 2); a state is
    /// kept in memory at this one.
    pub const VERSION: u32 = 2;

    /// The version written for this state: 1 while no word was added (a
    /// file every build reads), 2 once one was.
    fn stamp(&self) -> u32 {
        if self.added.is_empty() { 1 } else { 2 }
    }

    /// A state with no term started (the first run).
    pub fn empty() -> State {
        State {
            version: State::VERSION,
            added: Vec::new(),
            last_today: None,
            progress: Vec::new(),
        }
    }

    /// Reads a state of version 1 or 2, refusing an unknown version and a
    /// term twice (in the progress, or among the added words).
    ///
    /// # Errors
    /// [`StateError::Json`], [`StateError::UnknownVersion`] or
    /// [`StateError::DuplicateTerm`].
    pub fn from_json(text: &str) -> Result<State, StateError> {
        State::from_json_upto(text, State::VERSION)
    }

    /// [`State::from_json`] as a build whose highest version is `newest`
    /// reads it: the build before added words has 1 and refuses a file of
    /// version 2, which it never overwrites.
    ///
    /// # Errors
    /// As [`State::from_json`], with `newest` for [`State::VERSION`].
    pub fn from_json_upto(text: &str, newest: u32) -> Result<State, StateError> {
        let json_error = |error: serde_json::Error| StateError::Json(error.to_string());
        // The version is read first: a future version may have another shape.
        let raw: serde_json::Value = serde_json::from_str(text).map_err(json_error)?;
        if let Some(version) = raw.get("version").and_then(serde_json::Value::as_u64)
            && (version == 0 || version > u64::from(newest))
        {
            return Err(StateError::UnknownVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let mut state: State = serde_json::from_value(raw).map_err(json_error)?;
        let mut seen = HashSet::new();
        for entry in &state.progress {
            if !seen.insert(&entry.term) {
                return Err(StateError::DuplicateTerm(entry.term.clone()));
            }
        }
        let mut seen = HashSet::new();
        for term in &state.added {
            if !seen.insert(&term.id) {
                return Err(StateError::DuplicateTerm(term.id.clone()));
            }
        }
        state.version = State::VERSION;
        Ok(state)
    }

    /// [`State::from_json`], then [`State::shadow_added`] by `deck`: the
    /// read a host with a deck uses.
    ///
    /// # Errors
    /// As [`State::from_json`].
    pub fn from_json_for(text: &str, deck: &Deck) -> Result<State, StateError> {
        let mut state = State::from_json(text)?;
        state.shadow_added(deck);
        Ok(state)
    }

    /// Drops the added words the `deck` now teaches: a deck term shadows an
    /// added word of the same id, so the word is gone from `added` and its
    /// progress is served by the deck's term (the next save writes the file
    /// without it). Returns how many were dropped.
    pub fn shadow_added(&mut self, deck: &Deck) -> usize {
        let before = self.added.len();
        self.added.retain(|term| deck.term(&term.id).is_none());
        before - self.added.len()
    }

    /// The state as pretty JSON; entries keep their insertion order, so the
    /// same state always gives the same text.
    ///
    /// # Panics
    /// Never in practice: the types serialise without failing.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a state always serialises")
    }

    /// The term with this id: the deck's, else an added word.
    ///
    /// A deck term shadows an added word of the same id: the progress
    /// carries over, and a deck update that later adds the headword changes
    /// nothing for the learner: a file read with its deck drops the word
    /// ([`State::shadow_added`]).
    pub fn term<'a>(&'a self, deck: &'a Deck, id: &TermId) -> Option<&'a Term> {
        find_term(deck, &self.added, id)
    }

    /// The terms not started yet, in the order they are given as new terms:
    /// the deck's by `rank`, then the added words in the order they were
    /// added. An added word whose id is a deck term is that deck term (it
    /// is not listed twice).
    pub fn new_terms<'a>(&'a self, deck: &'a Deck) -> Vec<&'a Term> {
        let mut fresh: Vec<&Term> = deck
            .terms
            .iter()
            .filter(|term| self.get(&term.id).is_none())
            .collect();
        fresh.sort_by_key(|term| term.rank);
        fresh.extend(
            self.added
                .iter()
                .filter(|term| deck.term(&term.id).is_none() && self.get(&term.id).is_none()),
        );
        fresh
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

/// [`State::term`] over the parts of a state, for a caller that holds
/// `progress` mutably and so cannot borrow the whole state.
pub(crate) fn find_term<'a>(deck: &'a Deck, added: &'a [Term], id: &TermId) -> Option<&'a Term> {
    deck.term(id)
        .or_else(|| added.iter().find(|term| &term.id == id))
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
            added: Vec::new(),
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
    fn the_empty_state_is_stamped_version_one_and_holds_no_term() {
        let state = State::empty();
        assert_eq!(state.version, State::VERSION);
        assert_eq!(state.last_today, None);
        assert_eq!(state.progress.len(), 0);
        assert_eq!(state.added.len(), 0);
        let value: serde_json::Value = serde_json::from_str(&state.to_json()).unwrap();
        assert_eq!(value["version"], 1);
        assert!(value.get("added").is_none(), "{value}");
        assert_eq!(State::from_json(&state.to_json()).unwrap(), state);
    }

    fn with_added() -> State {
        State {
            added: vec![indeclinable("sānuvajja", u32::MAX), noun("abhiññā 1", 7)],
            ..two_terms()
        }
    }

    #[test]
    fn the_highest_version_this_build_reads_is_two() {
        assert_eq!(State::VERSION, 2);
    }

    #[test]
    fn a_state_with_added_words_is_stamped_two_and_round_trips() {
        let state = with_added();
        let value: serde_json::Value = serde_json::from_str(&state.to_json()).unwrap();
        assert_eq!(value["version"], 2);
        assert_eq!(value["added"][0]["id"], "sānuvajja");
        assert_eq!(value["added"][1]["id"], "abhiññā 1");
        assert_eq!(State::from_json(&state.to_json()).unwrap(), state);
    }

    #[test]
    fn the_stamp_follows_the_added_words_not_the_memory() {
        // Same progress, with and without a word: 1 then 2 then 1 again.
        let mut state = two_terms();
        assert_eq!(state.version, State::VERSION);
        assert!(state.to_json().contains("\"version\": 1"));
        state.added.push(indeclinable("sānuvajja", u32::MAX));
        assert!(state.to_json().contains("\"version\": 2"));
        state.added.clear();
        assert!(state.to_json().contains("\"version\": 1"));
    }

    #[test]
    fn a_state_read_from_either_version_holds_the_current_one_in_memory() {
        for text in [
            r#"{"version":1,"progress":[]}"#,
            r#"{"version":2,"progress":[]}"#,
        ] {
            assert_eq!(State::from_json(text).unwrap().version, State::VERSION);
        }
        assert_eq!(
            State::from_json(r#"{"version":2,"progress":[]}"#).unwrap(),
            State::empty()
        );
    }

    #[test]
    fn a_file_of_version_one_with_no_added_key_reads_with_no_added_word() {
        let text = two_terms().to_json();
        assert_eq!(State::from_json(&text).unwrap().added.len(), 0);
    }

    #[test]
    fn an_added_word_twice_is_refused() {
        let mut state = with_added();
        state.added.push(indeclinable("sānuvajja", u32::MAX));
        assert_eq!(
            State::from_json(&state.to_json()),
            Err(StateError::DuplicateTerm(id("sānuvajja")))
        );
    }

    /// A version-2 file as an earlier session wrote it, not built by
    /// `to_json`: one started term and one added word with its example.
    const VERSION_TWO: &str = r#"{
  "version": 2,
  "last_today": "2026-10-17",
  "progress": [
    {"term": "kho", "stage": "Day", "passed": ["Meaning"], "due": "2026-10-18", "lapses": 0}
  ],
  "added": [
    {
      "id": "sānuvajja",
      "lemma": "sānuvajja",
      "pos": "adj",
      "meaning": {"en": "blameworthy"},
      "example": {
        "text": "natthi sānuvajjo loke",
        "source": "AN 1",
        "sutta": "an1",
        "blank": {"before": "natthi ", "answer": "sānuvajjo", "after": " loke"}
      },
      "forms": [],
      "rank": 4294967295
    }
  ]
}"#;

    #[test]
    fn a_real_version_two_file_is_read_with_its_added_word() {
        let state = State::from_json(VERSION_TWO).unwrap();
        assert_eq!(state.version, State::VERSION);
        assert_eq!(state.progress.len(), 1);
        assert_eq!(state.added.len(), 1);
        let word = &state.added[0];
        assert_eq!(word.id, id("sānuvajja"));
        assert_eq!(word.rank, u32::MAX);
        let blank = word.example.as_ref().unwrap().blank.as_ref().unwrap();
        assert_eq!(blank.answer, "sānuvajjo");
        // Written back, it is the same state, still version 2.
        let value: serde_json::Value = serde_json::from_str(&state.to_json()).unwrap();
        assert_eq!(value["version"], 2);
        assert_eq!(State::from_json(&state.to_json()).unwrap(), state);
    }

    #[test]
    fn an_older_build_refuses_a_version_two_file_and_reads_version_one() {
        // The build before added words: its highest version is 1.
        assert_eq!(
            State::from_json_upto(VERSION_TWO, 1),
            Err(StateError::UnknownVersion(2))
        );
        let one = two_terms().to_json();
        assert_eq!(
            State::from_json_upto(&one, 1).unwrap(),
            State::from_json(&one).unwrap()
        );
        assert_eq!(
            State::from_json_upto(VERSION_TWO, 2),
            State::from_json(VERSION_TWO)
        );
    }

    #[test]
    fn a_deck_term_shadows_an_added_word_when_a_file_is_read_with_its_deck() {
        let deck = deck(vec![noun("sati 1.1", 1), indeclinable("kho", 2)]);
        let mut state = with_added();
        state.added.push(indeclinable("kho", u32::MAX));
        let read = State::from_json_for(&state.to_json(), &deck).unwrap();
        assert_eq!(
            read.added,
            with_added().added,
            "kho is dropped, the rest stays"
        );
        assert_eq!(read.progress, state.progress, "its progress carries over");
        assert_eq!(read.term(&deck, &id("kho")), deck.term(&id("kho")));
        // The next write holds the word that stays and not the shadowed one.
        let written: serde_json::Value = serde_json::from_str(&read.to_json()).unwrap();
        assert_eq!(written["added"].as_array().unwrap().len(), 2);
        let mut only = State::empty();
        only.added.push(indeclinable("kho", u32::MAX));
        let mut only = State::from_json(&only.to_json()).unwrap();
        assert_eq!(only.shadow_added(&deck), 1);
        assert_eq!(only.added.len(), 0);
        assert_eq!(
            only.to_json(),
            State::empty().to_json(),
            "written as version 1"
        );
        // The deck-less read does not know the deck.
        assert_eq!(State::from_json(&state.to_json()).unwrap().added.len(), 3);
    }

    #[test]
    fn version_zero_is_still_refused() {
        assert_eq!(
            State::from_json(r#"{"version":0,"progress":[]}"#),
            Err(StateError::UnknownVersion(0))
        );
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
            State::from_json(r#"{"version":3,"progress":[]}"#),
            Err(StateError::UnknownVersion(3))
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
    fn term_is_the_decks_else_an_added_word() {
        let d = deck(vec![noun("sati 1.1", 1)]);
        let state = with_added();
        assert_eq!(state.term(&d, &id("sati 1.1")).unwrap().rank, 1);
        assert_eq!(state.term(&d, &id("sānuvajja")).unwrap().rank, u32::MAX);
        assert!(state.term(&d, &id("missing")).is_none());
    }

    #[test]
    fn a_deck_term_shadows_an_added_word_of_the_same_id() {
        let d = deck(vec![noun("sānuvajja", 3)]);
        let state = with_added();
        assert_eq!(state.term(&d, &id("sānuvajja")).unwrap().rank, 3);
    }

    #[test]
    fn new_terms_are_the_decks_by_rank_then_the_added_words_in_order() {
        // The deck lists rank 2 before rank 1: the rank decides, not the file.
        let d = deck(vec![indeclinable("kho", 2), noun("sati 1.1", 1)]);
        let mut state = with_added();
        state.progress.clear();
        let ids = |state: &State| -> Vec<String> {
            state.new_terms(&d).iter().map(|t| t.id.0.clone()).collect()
        };
        assert_eq!(ids(&state), ["sati 1.1", "kho", "sānuvajja", "abhiññā 1"]);
        // Started terms and shadowed words are not new.
        state
            .progress
            .push(Progress::new(id("kho"), day("2026-10-05")));
        state
            .progress
            .push(Progress::new(id("abhiññā 1"), day("2026-10-05")));
        assert_eq!(ids(&state), ["sati 1.1", "sānuvajja"]);
        state.added.push(indeclinable("sati 1.1", u32::MAX));
        assert_eq!(ids(&state), ["sati 1.1", "sānuvajja"]);
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
