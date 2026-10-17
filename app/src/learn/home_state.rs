//! Which state the home shows ([`home_state`]), from what the store read,
//! the deck and `today`.

use crate::store::Load;
use learn_core::date::Date;
use learn_core::deck::Deck;
use learn_core::facet::facets_of;
use learn_core::progress::State;

/// Why the saved progress could not be used (the Broken banner's two
/// sentences; Q3: only an Invalid state may be started again).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrokenKind {
    /// Not a state this build can read (bad JSON, version 0, a duplicate).
    Invalid,
    /// Saved by a newer version: read-only, never overwritten.
    NewerVersion,
    /// The store could not read it (an I/O error): an error and a Reload,
    /// nothing written over it.
    Unreadable,
}

/// What the home shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HomeState {
    /// The first paint (SSR = the first client render): the skeleton,
    /// before the store is read.
    Loading,
    /// No progress yet (no key, or a state with no entry): the teaching
    /// sentence and 'Add new terms'.
    Empty,
    /// This many cards due today (A1: the queue's length).
    Due(usize),
    /// Nothing due today.
    NothingDue,
    /// The saved progress could not be used.
    Broken(BrokenKind),
}

/// The home's state for `load` (what the store read), the deck and `today`
/// (already `session_today`): [`HomeState::Empty`] when nothing is started,
/// [`HomeState::Due`] when [`due_cards`] is above 0, otherwise
/// [`HomeState::NothingDue`]. A load the store could not use is
/// [`HomeState::Broken`]; `Loading` is the page's own, before the read.
pub(crate) fn home_state(load: &Load, deck: &Deck, today: Date) -> HomeState {
    match load {
        Load::Empty => HomeState::Empty,
        Load::Invalid { .. } => HomeState::Broken(BrokenKind::Invalid),
        Load::NewerVersion { .. } => HomeState::Broken(BrokenKind::NewerVersion),
        Load::Unreadable(_) => HomeState::Broken(BrokenKind::Unreadable),
        Load::Ok(state) if state.progress.is_empty() => HomeState::Empty,
        Load::Ok(state) => match due_cards(deck, state, today) {
            0 => HomeState::NothingDue,
            due => HomeState::Due(due),
        },
    }
}

/// The cards due on `today` for `state` and `deck` (A1: Σ over the due
/// terms the deck knows of their facets not passed at their stage — the
/// queue `Session::start` builds): a term is due when `due <= today`, its
/// facets are `facets_of` less `effective_passed`; a term neither the deck
/// nor the state's added words know counts for nothing.
pub(crate) fn due_cards(deck: &Deck, state: &State, today: Date) -> usize {
    state
        .progress
        .iter()
        .filter(|progress| progress.due <= today)
        .filter_map(|progress| {
            state
                .term(deck, &progress.term)
                .map(|term| (progress, term))
        })
        .map(|(progress, term)| {
            facets_of(term)
                .difference(&State::effective_passed(progress, term))
                .len()
        })
        .sum()
}

/// The terms not started yet in `load`, the deck's and the added words (all
/// the deck's when it holds no state; 0 when it is Broken: nothing may be
/// added). Bounds the new
/// terms control and disables the home's CTA at 0 (Q4; ruled 3698). A term
/// in the state that the deck does not know uses up nothing.
pub(crate) fn new_left(load: &Load, deck: &Deck) -> usize {
    match load {
        Load::Empty => deck.terms.len(),
        Load::Invalid { .. } | Load::NewerVersion { .. } | Load::Unreadable(_) => 0,
        Load::Ok(state) => state.new_terms(deck).len(),
    }
}

#[cfg(test)]
mod tests {
    use learn_core::deck::TermId;
    use learn_core::facet::{Facet, FacetSet};
    use learn_core::progress::Progress;
    use learn_core::stage::Stage;

    use super::*;

    fn today() -> Date {
        Date::parse("2026-10-05").unwrap()
    }

    fn deck() -> &'static Deck {
        crate::deck::deck()
    }

    fn entry(term: &str, stage: Stage, due: Date) -> Progress {
        Progress {
            term: TermId(term.to_string()),
            stage,
            passed: FacetSet::empty(),
            due,
            lapses: 0,
        }
    }

    fn state_of(entries: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            added: Vec::new(),
            last_today: Some(today()),
            progress: entries,
        }
    }

    /// Every term of the deck started at `stage`, due in `days` days.
    fn everything_started(stage: Stage, days: u32) -> Vec<Progress> {
        deck()
            .terms
            .iter()
            .map(|term| entry(&term.id.0, stage, today() + days))
            .collect()
    }

    #[test]
    fn an_unreadable_store_is_broken_and_nothing_may_be_added() {
        let load = Load::Unreadable("permission denied".into());
        assert_eq!(
            home_state(&load, deck(), today()),
            HomeState::Broken(BrokenKind::Unreadable)
        );
        assert_eq!(new_left(&load, deck()), 0);
    }

    #[test]
    fn no_key_is_empty() {
        assert_eq!(home_state(&Load::Empty, deck(), today()), HomeState::Empty);
    }

    #[test]
    fn a_state_with_no_entry_is_empty() {
        let load = Load::Ok(State::empty());
        assert_eq!(home_state(&load, deck(), today()), HomeState::Empty);
    }

    #[test]
    fn the_cards_due_are_counted_per_facet() {
        // kho: Meaning, Recall, Context (3); hoti: + Forms (4).
        let load = Load::Ok(state_of(vec![
            entry("kho", Stage::Day, today()),
            entry("hoti 1", Stage::Day, today()),
        ]));
        assert_eq!(home_state(&load, deck(), today()), HomeState::Due(7));
    }

    #[test]
    fn a_facet_already_passed_is_not_due() {
        let mut hoti = entry("hoti 1", Stage::Week, today());
        hoti.passed = [Facet::Meaning, Facet::Forms].into_iter().collect();
        let load = Load::Ok(state_of(vec![hoti]));
        assert_eq!(home_state(&load, deck(), today()), HomeState::Due(2));
    }

    #[test]
    fn a_passed_facet_the_deck_no_longer_asks_does_not_count() {
        // kho has no Forms facet: passing it earlier removes nothing.
        let mut kho = entry("kho", Stage::Day, today());
        kho.passed = [Facet::Forms, Facet::Meaning].into_iter().collect();
        assert_eq!(due_cards(deck(), &state_of(vec![kho]), today()), 2);
    }

    #[test]
    fn only_the_terms_due_today_or_before_count() {
        let yesterday = Date::from_days(today().days() - 1);
        let state = state_of(vec![
            entry("kho", Stage::Day, yesterday),
            entry("hoti 1", Stage::Day, today() + 1),
            entry("evaṃ 1", Stage::Week, today()),
        ]);
        assert_eq!(due_cards(deck(), &state, today()), 6);
        assert_eq!(due_cards(deck(), &state, today() + 1), 10);
    }

    #[test]
    fn a_term_the_deck_does_not_know_is_never_due() {
        let state = state_of(vec![
            entry("not-in-the-deck", Stage::Day, today()),
            entry("kho", Stage::Day, today()),
        ]);
        assert_eq!(due_cards(deck(), &state, today()), 3);
        let only_orphan = state_of(vec![entry("not-in-the-deck", Stage::Day, today())]);
        assert_eq!(
            home_state(&Load::Ok(only_orphan), deck(), today()),
            HomeState::NothingDue
        );
    }

    #[test]
    fn a_due_term_with_every_facet_passed_has_nothing_to_ask() {
        // The session promotes it at its start; the home does not call it due.
        let mut kho = entry("kho", Stage::Day, today());
        kho.passed = [Facet::Meaning, Facet::Recall, Facet::Context]
            .into_iter()
            .collect();
        let load = Load::Ok(state_of(vec![kho]));
        assert_eq!(home_state(&load, deck(), today()), HomeState::NothingDue);
    }

    #[test]
    fn something_due_wins_over_long_stage_terms() {
        let mut entries = everything_started(Stage::Month, 20);
        entries[1].due = today();
        entries[1].stage = Stage::Day;
        let load = Load::Ok(state_of(entries));
        assert!(matches!(
            home_state(&load, deck(), today()),
            HomeState::Due(n) if n > 0
        ));
    }

    #[test]
    fn a_broken_load_is_broken_with_its_kind() {
        let invalid = Load::Invalid {
            raw: "{".to_string(),
        };
        let newer = Load::NewerVersion {
            raw: "{\"version\":3}".to_string(),
        };
        assert_eq!(
            home_state(&invalid, deck(), today()),
            HomeState::Broken(BrokenKind::Invalid)
        );
        assert_eq!(
            home_state(&newer, deck(), today()),
            HomeState::Broken(BrokenKind::NewerVersion)
        );
    }

    #[test]
    fn the_home_state_is_never_loading_once_the_store_is_read() {
        let loads = [
            Load::Empty,
            Load::Ok(state_of(vec![entry("kho", Stage::Day, today())])),
            Load::Invalid { raw: String::new() },
            Load::NewerVersion { raw: String::new() },
        ];
        for load in &loads {
            assert_ne!(home_state(load, deck(), today()), HomeState::Loading);
        }
    }

    #[test]
    fn new_left_is_the_whole_deck_when_nothing_is_started() {
        let size = deck().terms.len();
        assert_eq!(new_left(&Load::Empty, deck()), size);
        assert_eq!(new_left(&Load::Ok(State::empty()), deck()), size);
    }

    #[test]
    fn new_left_is_the_deck_terms_not_in_the_state() {
        let load = Load::Ok(state_of(vec![
            entry("kho", Stage::Day, today()),
            entry("hoti 1", Stage::Day, today()),
        ]));
        assert_eq!(new_left(&load, deck()), deck().terms.len() - 2);
    }

    #[test]
    fn an_orphan_does_not_use_up_a_new_term() {
        let load = Load::Ok(state_of(vec![
            entry("not-in-the-deck", Stage::Day, today()),
            entry("kho", Stage::Day, today()),
        ]));
        assert_eq!(new_left(&load, deck()), deck().terms.len() - 1);
    }

    #[test]
    fn new_left_is_zero_when_the_deck_is_started() {
        let load = Load::Ok(state_of(everything_started(Stage::Day, 1)));
        assert_eq!(new_left(&load, deck()), 0);
    }

    #[test]
    fn new_left_is_zero_when_the_progress_is_broken() {
        let invalid = Load::Invalid { raw: String::new() };
        let newer = Load::NewerVersion { raw: String::new() };
        assert_eq!(new_left(&invalid, deck()), 0);
        assert_eq!(new_left(&newer, deck()), 0);
    }

    #[test]
    fn a_started_added_word_is_due_and_makes_the_home_due() {
        let state = crate::test_support::added_started(Stage::Day, today());
        assert_eq!(due_cards(deck(), &state, today()), 2);
        assert_eq!(
            home_state(&Load::Ok(state), deck(), today()),
            HomeState::Due(2)
        );
        let later = crate::test_support::added_started(Stage::Week, today() + 3);
        assert_eq!(due_cards(deck(), &later, today()), 0);
    }

    #[test]
    fn the_added_words_not_started_are_new_terms_too() {
        let mut state = state_of(vec![]);
        state.added.push(crate::test_support::added_word());
        assert_eq!(
            new_left(&Load::Ok(state.clone()), deck()),
            deck().terms.len() + 1
        );
        state.progress.push(entry("sānuvajja", Stage::Day, today()));
        assert_eq!(new_left(&Load::Ok(state), deck()), deck().terms.len());
    }
}
