//! Growing the state: new terms on demand, and `advance`.
//!
//! [`Session::request_new`] starts the next terms of the deck by rank.
//! [`Session::advance`] brings forward terms of the long stages when the
//! learner has nothing left to do: only when no known term is due and every
//! deck term is already in the state.

use crate::progress::{Progress, State, find_term};
use crate::session::{Question, Session, unpassed_questions};
use crate::stage::Stage;

impl Session<'_> {
    /// Starts up to `n` new terms: the next ones by `rank` that are not in
    /// the state, then the added words in the order they were added
    /// ([`State::new_terms`]), each as a [`crate::progress::Progress::new`] (stage Day,
    /// due today) appended to the state, their facets appended to the end of
    /// the queue. Returns how many were started: fewer than `n` when the
    /// deck has fewer left, 0 when it is exhausted.
    pub fn request_new(&mut self, n: usize) -> usize {
        let (today, deck) = (self.today, self.deck);
        let mut fresh = self.state.new_terms(deck);
        fresh.truncate(n);
        let started: Vec<(Progress, Vec<Question>)> = fresh
            .into_iter()
            .map(|term| {
                let progress = Progress::new(term.id.clone(), today);
                let questions = unpassed_questions(deck, term, &progress);
                (progress, questions)
            })
            .collect();
        let count = started.len();
        for (progress, questions) in started {
            self.queue.extend(questions);
            self.state.progress.push(progress);
        }
        count
    }

    /// Brings long-stage terms forward when nothing else is to do.
    ///
    /// Refused (returns 0, nothing changes) unless the queue is empty, no
    /// term the deck knows is due, and every deck term is in the state.
    /// Otherwise picks up to `k` terms of stage Month or later, those with
    /// the earliest `due` first (ties by `rank`), sets their `due` to today
    /// (the stage is kept) and queues their facets. Returns how many were
    /// picked; `k` above what exists takes all, none gives 0.
    pub fn advance(&mut self, k: usize) -> usize {
        if !self.may_advance() {
            return 0;
        }
        let deck = self.deck;
        let today = self.today;
        let mut picked: Vec<(usize, u32)> = Vec::new();
        for (index, progress) in self.state.progress.iter().enumerate() {
            if let Some(term) = self.state.term(deck, &progress.term)
                && progress.stage >= Stage::Month
            {
                picked.push((index, term.rank));
            }
        }
        picked.sort_by_key(|(index, rank)| (self.state.progress[*index].due, *rank));
        picked.truncate(k);
        for (index, _) in &picked {
            let State {
                progress, added, ..
            } = &mut self.state;
            let progress = &mut progress[*index];
            progress.due = today;
            if let Some(term) = find_term(deck, added, &progress.term) {
                self.queue.extend(unpassed_questions(deck, term, progress));
            }
        }
        picked.len()
    }

    /// True when nothing is queued, no known term is due and every new term
    /// has been started: the deck's, and the added words an older build
    /// left unstarted too.
    fn may_advance(&self) -> bool {
        let deck = self.deck;
        let nothing_due = !self.state.progress.iter().any(|progress| {
            progress.due <= self.today && self.state.term(deck, &progress.term).is_some()
        });
        let all_started = self.state.new_terms(deck).is_empty();
        self.queue.is_empty() && nothing_due && all_started
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;
    use crate::deck::samples::{deck as deck_of, indeclinable, noun};
    use crate::deck::{Deck, TermId};
    use crate::facet::{Facet, FacetSet};
    use crate::progress::{Progress, State};
    use crate::session::Answer;
    use crate::stage::Stage;

    const TODAY: u32 = 20731;

    fn day(days: u32) -> Date {
        Date::from_days(days)
    }

    fn entry(term: &str, stage: Stage, due: u32) -> Progress {
        Progress {
            term: TermId(term.to_string()),
            stage,
            passed: FacetSet::empty(),
            due: day(due),
            lapses: 0,
        }
    }

    fn state_of(progress: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            added: Vec::new(),
            last_today: None,
            progress,
        }
    }

    fn start(deck: &Deck, state: State) -> Session<'_> {
        Session::start(deck, state, day(TODAY), 0).unwrap()
    }

    /// The terms of the state, in order.
    fn terms(session: &Session<'_>) -> Vec<String> {
        session
            .snapshot()
            .progress
            .into_iter()
            .map(|progress| progress.term.0)
            .collect()
    }

    /// Everything that is asked from now on (every answer a Pass).
    fn drain(session: &mut Session<'_>) -> Vec<(String, Facet)> {
        let mut seen = Vec::new();
        while let Some(question) = session.next().cloned() {
            seen.push((question.term.0.clone(), question.facet));
            session
                .answer(&question.term, question.facet, Answer::Pass)
                .unwrap();
        }
        seen
    }

    fn pair(term: &str, facet: Facet) -> (String, Facet) {
        (term.to_string(), facet)
    }

    // ---- request_new ----

    #[test]
    fn new_terms_come_by_rank_whatever_the_deck_order() {
        let deck = deck_of(vec![
            indeclinable("c", 3),
            indeclinable("a", 1),
            indeclinable("b", 2),
        ]);
        let mut session = start(&deck, State::empty());
        assert_eq!(session.request_new(2), 2);
        assert_eq!(terms(&session), vec!["a", "b"]);
        for progress in session.snapshot().progress {
            assert_eq!(progress.stage, Stage::Day);
            assert_eq!(progress.due, day(TODAY));
            assert_eq!(progress.passed, FacetSet::empty());
            assert_eq!(progress.lapses, 0);
        }
    }

    #[test]
    fn new_terms_skip_the_known_ones() {
        let deck = deck_of(vec![indeclinable("a", 1), indeclinable("b", 2)]);
        let state = state_of(vec![entry("a", Stage::Week, TODAY + 3)]);
        let mut session = start(&deck, state);
        assert_eq!(session.request_new(1), 1);
        assert_eq!(terms(&session), vec!["a", "b"]);
        assert_eq!(session.request_new(1), 0, "the deck is exhausted");
    }

    #[test]
    fn asking_for_more_than_is_left_gives_what_is_left() {
        let deck = deck_of(vec![indeclinable("a", 1), indeclinable("b", 2)]);
        let mut session = start(&deck, State::empty());
        assert_eq!(session.request_new(0), 0);
        assert_eq!(session.request_new(5), 2);
        assert_eq!(session.request_new(5), 0);
        assert_eq!(terms(&session), vec!["a", "b"]);
    }

    #[test]
    fn new_terms_are_queued_with_their_facets_after_what_is_already_queued() {
        let deck = deck_of(vec![
            indeclinable("old", 1),
            noun("sati 1.1", 2),
            indeclinable("kho", 3),
        ]);
        let state = state_of(vec![entry("old", Stage::Week, TODAY)]);
        let mut session = start(&deck, state);
        assert_eq!(session.request_new(2), 2);
        assert_eq!(
            drain(&mut session),
            vec![
                pair("old", Facet::Meaning),
                pair("old", Facet::Recall),
                pair("sati 1.1", Facet::Meaning),
                pair("sati 1.1", Facet::Recall),
                pair("sati 1.1", Facet::Forms),
                pair("sati 1.1", Facet::Context),
                pair("kho", Facet::Meaning),
                pair("kho", Facet::Recall),
            ]
        );
    }

    // ---- advance ----

    fn long_stage_deck() -> Deck {
        deck_of(vec![
            indeclinable("w", 1),
            indeclinable("m1", 2),
            indeclinable("m2", 3),
            indeclinable("m3", 4),
        ])
    }

    fn long_stage_state() -> State {
        state_of(vec![
            entry("w", Stage::Week, TODAY + 1),
            entry("m1", Stage::Month, TODAY + 5),
            entry("m2", Stage::Quarter, TODAY + 2),
            entry("m3", Stage::Year, TODAY + 2),
        ])
    }

    #[test]
    fn advance_takes_the_earliest_due_then_the_lowest_rank_and_keeps_the_stage() {
        let deck = long_stage_deck();
        let mut session = start(&deck, long_stage_state());
        assert_eq!(session.advance(2), 2);
        let state = session.snapshot();
        let get = |name: &str| state.get(&TermId(name.to_string())).unwrap().clone();
        assert_eq!(
            (get("m2").stage, get("m2").due),
            (Stage::Quarter, day(TODAY))
        );
        assert_eq!((get("m3").stage, get("m3").due), (Stage::Year, day(TODAY)));
        assert_eq!(get("m1").due, day(TODAY + 5), "not picked");
        assert_eq!(get("w").due, day(TODAY + 1), "below Month: never picked");
        assert_eq!(
            drain(&mut session),
            vec![
                pair("m2", Facet::Meaning),
                pair("m2", Facet::Recall),
                pair("m3", Facet::Meaning),
                pair("m3", Facet::Recall),
            ]
        );
    }

    #[test]
    fn advance_with_k_above_the_long_terms_takes_them_all() {
        let deck = long_stage_deck();
        let mut session = start(&deck, long_stage_state());
        assert_eq!(session.advance(10), 3);
        assert_eq!(session.stats().due_today, 3);
    }

    #[test]
    fn advance_with_no_long_term_or_k_zero_gives_nothing() {
        let deck = deck_of(vec![indeclinable("a", 1), indeclinable("b", 2)]);
        let state = state_of(vec![
            entry("a", Stage::Day, TODAY + 1),
            entry("b", Stage::Week, TODAY + 3),
        ]);
        let mut session = start(&deck, state.clone());
        assert_eq!(session.advance(3), 0);
        assert_eq!(session.snapshot().progress, state.progress);
        let deck = long_stage_deck();
        let mut session = start(&deck, long_stage_state());
        assert_eq!(session.advance(0), 0);
        assert_eq!(session.stats().due_today, 0);
        assert_eq!(session.snapshot().progress, long_stage_state().progress);
    }

    #[test]
    fn advance_is_refused_while_a_term_is_due() {
        let deck = long_stage_deck();
        let mut state = long_stage_state();
        state.progress[0].due = day(TODAY); // w is due today
        let mut session = start(&deck, state.clone());
        assert_eq!(session.advance(3), 0);
        assert_eq!(session.snapshot().progress, state.progress);
    }

    #[test]
    fn advance_is_refused_while_a_deck_term_is_not_in_the_state() {
        let deck = deck_of(vec![
            indeclinable("m1", 1),
            indeclinable("never started", 2),
        ]);
        let state = state_of(vec![entry("m1", Stage::Month, TODAY + 5)]);
        let mut session = start(&deck, state.clone());
        assert_eq!(session.advance(3), 0);
        assert_eq!(session.snapshot().progress, state.progress);
    }

    #[test]
    fn an_orphan_does_not_block_advance() {
        let deck = deck_of(vec![indeclinable("m1", 1)]);
        let state = state_of(vec![
            entry("ghost", Stage::Day, TODAY - 4),
            entry("m1", Stage::Month, TODAY + 5),
        ]);
        let mut session = start(&deck, state);
        assert_eq!(session.advance(1), 1);
        let state = session.snapshot();
        assert_eq!(
            state.get(&TermId("ghost".to_string())).unwrap().due,
            day(TODAY - 4)
        );
    }
}
