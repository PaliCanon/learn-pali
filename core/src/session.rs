//! A review session: the queue, the answers and the two transitions.
//!
//! A [`Session`] is built by [`Session::start`] from a deck, a [`State`] and
//! `today` (an input: nothing here reads a clock). The **queue** is derived
//! from the state and never stored: every term with `due <= today` that the
//! deck knows, oldest `due` first and ties by `rank`, each one expanded into
//! the facets of [`facets_of`] it has not passed at its stage, in [`Facet`]
//! order. A term the deck does not know (an orphan) stays in the state and is
//! never asked.
//!
//! [`Session::next`] shows the current question and [`Session::answer`]
//! settles it:
//!
//! - **Pass** on a queue question adds the facet to `passed`; once the
//!   effective passed facets (`passed ∩ facets_of(term)`) cover every facet of
//!   the term, the term moves to the next stage, `passed` is cleared and
//!   `due = today + interval(new stage)` (a late review counts from today).
//! - **Fail** on a queue question sends the term back to Day: `passed`
//!   cleared, `lapses + 1`, `due = today + 1`. The term is no longer due, so
//!   its other queued facets are dropped; the failed facet is asked once more
//!   at the end of the session as **practice**.
//! - A **practice** question changes nothing in the state, Pass or Fail, and
//!   is never queued again, so a session always ends.
//!
//! [`Session::into_state`] gives the state back with `last_today` recorded;
//! a later session refuses a `today` before it.

use std::collections::VecDeque;
use std::fmt;

use crate::date::Date;
use crate::deck::{Deck, Term, TermId};
use crate::facet::{Facet, FacetSet, facets_of};
use crate::progress::{Progress, State};
use crate::stage::Stage;

/// One thing to ask: a facet of a term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The term asked.
    pub term: TermId,
    /// The facet of the term asked.
    pub facet: Facet,
    /// True for the second asking of a failed facet: its answer changes
    /// nothing in the state.
    pub practice: bool,
}

/// How the learner judged their own answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Right: the facet counts as passed.
    Pass,
    /// Wrong: the term goes back to Day.
    Fail,
}

/// Why a session could not start or an answer was refused. The state is
/// never changed by a refused call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// `today` is earlier than the latest day the state saw.
    TodayBeforeLast {
        /// The day asked for.
        today: Date,
        /// The latest day recorded in the state.
        last: Date,
    },
    /// The answer is not for the current question.
    NotCurrent {
        /// The current question (`None` when the session is over).
        asked: Option<(TermId, Facet)>,
        /// The pair the caller answered.
        given: (TermId, Facet),
    },
    /// The answered term is in neither the deck nor the state.
    UnknownTerm(TermId),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::TodayBeforeLast { today, last } => write!(
                f,
                "today ({today}) is before the last day of the state ({last})"
            ),
            SessionError::NotCurrent { asked, given } => match asked {
                Some((term, facet)) => write!(
                    f,
                    "answered {} {:?}, but the current question is {term} {facet:?}",
                    given.0, given.1
                ),
                None => write!(
                    f,
                    "answered {} {:?}, but no question is pending",
                    given.0, given.1
                ),
            },
            SessionError::UnknownTerm(term) => write!(f, "term {term} is not known"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A review session over a deck.
#[derive(Debug)]
pub struct Session<'d> {
    pub(crate) deck: &'d Deck,
    pub(crate) state: State,
    pub(crate) today: Date,
    pub(crate) queue: VecDeque<Question>,
    pub(crate) practice: VecDeque<Question>,
    pub(crate) current: Option<Question>,
    changed_at_start: bool,
}

impl<'d> Session<'d> {
    /// Starts a session. See the module documentation for the queue.
    ///
    /// A due term whose effective passed facets already cover all its facets
    /// (the deck lost the facet it was waiting for) has nothing left to ask:
    /// it takes the pass transition at once, so it never stays due forever.
    ///
    /// # Errors
    /// [`SessionError::TodayBeforeLast`] when `today < state.last_today`.
    pub fn start(
        deck: &'d Deck,
        mut state: State,
        today: Date,
    ) -> Result<Session<'d>, SessionError> {
        if let Some(last) = state.last_today
            && today < last
        {
            return Err(SessionError::TodayBeforeLast { today, last });
        }
        let changed_at_start = settle_complete(deck, &mut state, today);
        let queue = due_queue(deck, &state, today);
        Ok(Session {
            deck,
            state,
            today,
            queue,
            practice: VecDeque::new(),
            current: None,
            changed_at_start,
        })
    }

    /// True when [`Session::start`] changed the state it was given: a due
    /// term with nothing left to ask was promoted. A caller that persists
    /// the state saves it then, even when it did nothing else, or the term
    /// would be promoted again from its stale `due` at every run.
    pub fn changed_at_start(&self) -> bool {
        self.changed_at_start
    }

    /// The current question: the front of the queue, then the practice
    /// questions once the queue is empty; `None` when the session is over.
    /// It stays the same until it is answered.
    // The name is the design's; it lends a reference to the session, so it
    // cannot be an `Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&Question> {
        if self.current.is_none() {
            self.current = self.queue.pop_front().or_else(|| self.practice.pop_front());
        }
        self.current.as_ref()
    }

    /// Settles the current question.
    ///
    /// # Errors
    /// [`SessionError::NotCurrent`] when `(term, facet)` is not the current
    /// question, [`SessionError::UnknownTerm`] when the term has no entry.
    pub fn answer(
        &mut self,
        term: &TermId,
        facet: Facet,
        answer: Answer,
    ) -> Result<(), SessionError> {
        let current = self.next().cloned();
        let matches = current
            .as_ref()
            .is_some_and(|question| &question.term == term && question.facet == facet);
        let Some(question) = current.filter(|_| matches) else {
            return Err(SessionError::NotCurrent {
                asked: self.current.as_ref().map(|q| (q.term.clone(), q.facet)),
                given: (term.clone(), facet),
            });
        };
        if !question.practice {
            self.settle(&question, answer)?;
        }
        self.current = None;
        Ok(())
    }

    /// The day of this session.
    pub fn today(&self) -> Date {
        self.today
    }

    /// The state as it stands, with `last_today` recorded: what to write
    /// after an answer. `last_today` is the last day the state was written
    /// by a session, whatever the command (a `stats` that saves a start-time
    /// promotion stamps it too).
    pub fn snapshot(&self) -> State {
        let mut state = self.state.clone();
        state.last_today = Some(self.today);
        state
    }

    /// Ends the session: the state with `last_today = today`.
    pub fn into_state(mut self) -> State {
        self.state.last_today = Some(self.today);
        self.state
    }

    /// Applies a queue question's answer to the state.
    fn settle(&mut self, question: &Question, answer: Answer) -> Result<(), SessionError> {
        let today = self.today;
        let term = self
            .deck
            .term(&question.term)
            .ok_or_else(|| SessionError::UnknownTerm(question.term.clone()))?;
        let progress = self
            .state
            .progress
            .iter_mut()
            .find(|entry| entry.term == question.term)
            .ok_or_else(|| SessionError::UnknownTerm(question.term.clone()))?;
        match answer {
            Answer::Pass => pass(progress, term, question.facet, today),
            Answer::Fail => {
                fail(progress, today);
                self.queue.retain(|queued| queued.term != question.term);
                self.practice.push_back(Question {
                    practice: true,
                    ..question.clone()
                });
            }
        }
        Ok(())
    }
}

/// The pass transition: the facet is passed; when every facet of the term
/// is, the term moves up and is due again after the new stage's interval,
/// counted from `today`.
fn pass(progress: &mut Progress, term: &Term, facet: Facet, today: Date) {
    progress.passed.insert(facet);
    if State::effective_passed(progress, term).is_superset(&facets_of(term)) {
        promote(progress, today);
    }
}

/// Next stage, `passed` cleared, `due = today + interval(new stage)`.
fn promote(progress: &mut Progress, today: Date) {
    progress.stage = progress.stage.next();
    progress.passed = FacetSet::empty();
    progress.due = today + progress.stage.interval_days();
}

/// The fail transition: back to Day, `passed` cleared, one more lapse,
/// due tomorrow.
fn fail(progress: &mut Progress, today: Date) {
    progress.stage = Stage::Day;
    progress.passed = FacetSet::empty();
    progress.lapses += 1;
    progress.due = today + 1;
}

/// Promotes every due term of the deck that has no facet left to ask;
/// true when it promoted at least one.
fn settle_complete(deck: &Deck, state: &mut State, today: Date) -> bool {
    let mut changed = false;
    for progress in &mut state.progress {
        if progress.due <= today
            && let Some(term) = deck.term(&progress.term)
            && unpassed_questions(term, progress).is_empty()
        {
            promote(progress, today);
            changed = true;
        }
    }
    changed
}

/// The queue: due terms known to the deck, oldest `due` first and ties by
/// `rank`, each expanded into its unpassed facets.
fn due_queue(deck: &Deck, state: &State, today: Date) -> VecDeque<Question> {
    let mut due: Vec<(&Progress, &Term)> = state
        .progress
        .iter()
        .filter(|progress| progress.due <= today)
        .filter_map(|progress| deck.term(&progress.term).map(|term| (progress, term)))
        .collect();
    due.sort_by_key(|(progress, term)| (progress.due, term.rank));
    due.into_iter()
        .flat_map(|(progress, term)| unpassed_questions(term, progress))
        .collect()
}

/// The facets of `term` that `progress` has not passed, as queue questions,
/// in [`Facet`] order.
pub(crate) fn unpassed_questions(term: &Term, progress: &Progress) -> Vec<Question> {
    facets_of(term)
        .difference(&State::effective_passed(progress, term))
        .iter()
        .map(|facet| Question {
            term: term.id.clone(),
            facet,
            practice: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::samples::{deck as deck_of, indeclinable, noun, rare_form};

    const TODAY: u32 = 20731;

    fn day(days: u32) -> Date {
        Date::from_days(days)
    }

    fn id(text: &str) -> TermId {
        TermId(text.to_string())
    }

    fn entry(term: &str, stage: Stage, due: Date) -> Progress {
        Progress {
            term: id(term),
            stage,
            passed: FacetSet::empty(),
            due,
            lapses: 0,
        }
    }

    fn state_of(progress: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            last_today: None,
            progress,
        }
    }

    fn start(deck: &Deck, state: State, today: u32) -> Session<'_> {
        Session::start(deck, state, day(today)).unwrap()
    }

    /// Answers every question with `answer`; what was asked, in order.
    fn drain(session: &mut Session<'_>, answer: Answer) -> Vec<(String, Facet, bool)> {
        let mut seen = Vec::new();
        while let Some(question) = session.next().cloned() {
            seen.push((question.term.0.clone(), question.facet, question.practice));
            session
                .answer(&question.term, question.facet, answer)
                .unwrap();
        }
        seen
    }

    /// What a session would ask, in order (every answer a Pass).
    fn asked(deck: &Deck, state: State, today: u32) -> Vec<(String, Facet)> {
        let mut session = start(deck, state, today);
        drain(&mut session, Answer::Pass)
            .into_iter()
            .map(|(term, facet, _)| (term, facet))
            .collect()
    }

    fn pair(term: &str, facet: Facet) -> (String, Facet) {
        (term.to_string(), facet)
    }

    // ---- T2: pass ----

    #[test]
    fn passing_every_facet_climbs_one_stage_at_each_stage() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        for stage in Stage::ALL {
            let state = state_of(vec![entry("kho", stage, day(TODAY))]);
            let mut session = start(&deck, state, TODAY);
            assert_eq!(drain(&mut session, Answer::Pass).len(), 2);
            let progress = session.into_state().progress.remove(0);
            assert_eq!(progress.stage, stage.next(), "{stage:?}");
            assert_eq!(
                progress.due,
                day(TODAY) + stage.next().interval_days(),
                "{stage:?}"
            );
            assert_eq!(progress.passed, FacetSet::empty());
            assert_eq!(progress.lapses, 0);
        }
    }

    #[test]
    fn a_year_term_stays_year_and_is_due_in_365_days() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let state = state_of(vec![entry("kho", Stage::Year, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        drain(&mut session, Answer::Pass);
        let progress = session.into_state().progress.remove(0);
        assert_eq!(progress.stage, Stage::Year);
        assert_eq!(progress.due, day(TODAY + 365));
    }

    #[test]
    fn a_partial_pass_records_the_facet_and_keeps_stage_and_due() {
        let deck = deck_of(vec![noun("sati 1.1", 1)]);
        let state = state_of(vec![entry("sati 1.1", Stage::Week, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        let first = session.next().cloned().unwrap();
        session
            .answer(&first.term, first.facet, Answer::Pass)
            .unwrap();
        let progress = session.snapshot().progress.remove(0);
        assert_eq!(progress.stage, Stage::Week);
        assert_eq!(progress.due, day(TODAY));
        assert_eq!(
            progress.passed.iter().collect::<Vec<_>>(),
            vec![Facet::Meaning]
        );
    }

    #[test]
    fn a_late_review_counts_from_today() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let state = state_of(vec![entry("kho", Stage::Day, day(TODAY - 10))]);
        let mut session = start(&deck, state, TODAY);
        drain(&mut session, Answer::Pass);
        let progress = session.into_state().progress.remove(0);
        assert_eq!(progress.stage, Stage::Week);
        assert_eq!(progress.due, day(TODAY + 7));
    }

    // ---- T3: fail and practice ----

    #[test]
    fn a_fail_sends_the_term_to_day_at_any_stage() {
        let deck = deck_of(vec![noun("sati 1.1", 1)]);
        for stage in [Stage::Day, Stage::Week, Stage::Year] {
            let mut progress = entry("sati 1.1", stage, day(TODAY));
            progress.lapses = 2;
            let mut session = start(&deck, state_of(vec![progress]), TODAY);
            let first = session.next().cloned().unwrap();
            session
                .answer(&first.term, first.facet, Answer::Fail)
                .unwrap();
            let progress = session.snapshot().progress.remove(0);
            assert_eq!(progress.stage, Stage::Day, "{stage:?}");
            assert_eq!(progress.due, day(TODAY + 1));
            assert_eq!(progress.lapses, 3);
            assert_eq!(progress.passed, FacetSet::empty());
        }
    }

    #[test]
    fn a_fail_clears_what_was_passed_and_drops_the_other_queued_facets() {
        let deck = deck_of(vec![noun("sati 1.1", 1)]);
        let state = state_of(vec![entry("sati 1.1", Stage::Month, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        session.next();
        session
            .answer(&id("sati 1.1"), Facet::Meaning, Answer::Pass)
            .unwrap();
        session.next();
        session
            .answer(&id("sati 1.1"), Facet::Recall, Answer::Fail)
            .unwrap();
        let progress = session.snapshot().progress.remove(0);
        assert_eq!(progress.stage, Stage::Day);
        assert_eq!(progress.passed, FacetSet::empty());
        assert_eq!(progress.lapses, 1);
        // Forms and Context are gone; only the practice of Recall is left.
        assert_eq!(
            drain(&mut session, Answer::Pass),
            vec![("sati 1.1".to_string(), Facet::Recall, true)]
        );
    }

    #[test]
    fn the_practice_question_comes_after_the_other_terms() {
        let deck = deck_of(vec![indeclinable("a", 1), indeclinable("b", 2)]);
        let state = state_of(vec![
            entry("a", Stage::Week, day(TODAY - 3)),
            entry("b", Stage::Week, day(TODAY)),
        ]);
        let mut session = start(&deck, state, TODAY);
        session.next();
        session
            .answer(&id("a"), Facet::Meaning, Answer::Fail)
            .unwrap();
        assert_eq!(
            drain(&mut session, Answer::Pass),
            vec![
                ("b".to_string(), Facet::Meaning, false),
                ("b".to_string(), Facet::Recall, false),
                ("a".to_string(), Facet::Meaning, true),
            ]
        );
    }

    #[test]
    fn a_practice_answer_changes_nothing_and_the_session_ends() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        for answer in [Answer::Pass, Answer::Fail] {
            let state = state_of(vec![entry("kho", Stage::Quarter, day(TODAY))]);
            let mut session = start(&deck, state, TODAY);
            session.next();
            session
                .answer(&id("kho"), Facet::Meaning, Answer::Fail)
                .unwrap();
            let question = session.next().cloned().unwrap();
            assert!(question.practice);
            let before = session.snapshot();
            session
                .answer(&question.term, question.facet, answer)
                .unwrap();
            assert_eq!(session.snapshot(), before, "{answer:?}");
            assert_eq!(before.progress[0].lapses, 1);
            assert_eq!(session.next(), None, "never re-queued");
        }
    }

    #[test]
    fn a_practice_question_answered_fail_leaves_the_failed_term_as_the_first_fail_made_it() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let state = state_of(vec![entry("kho", Stage::Quarter, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        session.next();
        session
            .answer(&id("kho"), Facet::Meaning, Answer::Fail)
            .unwrap();
        let question = session.next().cloned().unwrap();
        assert!(question.practice);
        session
            .answer(&question.term, question.facet, Answer::Fail)
            .unwrap();
        let term = session.snapshot().progress.remove(0);
        assert_eq!(term.stage, Stage::Day);
        assert_eq!(term.due, day(TODAY + 1));
        assert_eq!(term.lapses, 1, "the practice miss is not a second lapse");
        assert_eq!(term.passed, FacetSet::empty());
        assert_eq!(session.next(), None, "the practice is not asked again");
    }

    #[test]
    fn a_forms_practice_after_a_fail_may_show_another_form_label() {
        use crate::deck::samples::form;
        use crate::forms::forms_question;
        let term = Term {
            forms: vec![
                form("fem nom sg", "sati"),
                form("fem acc sg", "satiṃ"),
                form("fem acc pl", "satiyo"),
            ],
            example: None,
            ..noun("sati 1.1", 1)
        };
        let deck = deck_of(vec![term.clone()]);
        let mut progress = entry("sati 1.1", Stage::Day, day(TODAY));
        progress.passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
        let before = forms_question(&term, &progress).unwrap().label.clone();
        let mut session = start(&deck, state_of(vec![progress]), TODAY);
        let question = session.next().cloned().unwrap();
        assert_eq!(question.facet, Facet::Forms, "only Forms is left to ask");
        assert!(!question.practice);
        session
            .answer(&question.term, question.facet, Answer::Fail)
            .unwrap();
        let practice = session.next().cloned().unwrap();
        assert_eq!(practice.term, id("sati 1.1"));
        assert_eq!(practice.facet, Facet::Forms);
        assert!(practice.practice);
        let failed = session.snapshot().progress.remove(0);
        assert_eq!(failed.lapses, 1);
        let after = forms_question(&term, &failed).unwrap().label.clone();
        assert_ne!(
            after, before,
            "practice is practice: the label follows the state"
        );
    }

    #[test]
    fn the_next_day_every_facet_is_asked_again_and_all_passed_reach_week() {
        let deck = deck_of(vec![noun("sati 1.1", 1)]);
        let state = state_of(vec![entry("sati 1.1", Stage::Month, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        session.next();
        session
            .answer(&id("sati 1.1"), Facet::Meaning, Answer::Fail)
            .unwrap();
        drain(&mut session, Answer::Fail);
        let state = session.into_state();
        assert_eq!(
            asked(&deck, state.clone(), TODAY).len(),
            0,
            "not due again today"
        );
        let mut next_day = start(&deck, state, TODAY + 1);
        let seen = drain(&mut next_day, Answer::Pass);
        assert_eq!(seen.len(), 4);
        assert!(seen.iter().all(|(_, _, practice)| !practice));
        let progress = next_day.into_state().progress.remove(0);
        assert_eq!(progress.stage, Stage::Week);
        assert_eq!(progress.due, day(TODAY + 1 + 7));
    }

    // ---- T4: the queue ----

    #[test]
    fn only_due_terms_are_asked() {
        let deck = deck_of(vec![indeclinable("a", 1), indeclinable("b", 2)]);
        let state = state_of(vec![
            entry("a", Stage::Week, day(TODAY + 1)),
            entry("b", Stage::Week, day(TODAY)),
        ]);
        assert_eq!(
            asked(&deck, state, TODAY),
            vec![pair("b", Facet::Meaning), pair("b", Facet::Recall)]
        );
    }

    #[test]
    fn oldest_due_first_then_by_rank() {
        let deck = deck_of(vec![
            indeclinable("a", 1),
            indeclinable("b", 2),
            indeclinable("c", 3),
        ]);
        let state = state_of(vec![
            entry("c", Stage::Day, day(TODAY)),
            entry("b", Stage::Day, day(TODAY - 2)),
            entry("a", Stage::Day, day(TODAY - 2)),
        ]);
        let order: Vec<String> = asked(&deck, state, TODAY)
            .into_iter()
            .map(|(term, _)| term)
            .collect();
        assert_eq!(order, vec!["a", "a", "b", "b", "c", "c"]);
    }

    #[test]
    fn a_term_passed_today_is_not_asked_again_in_a_new_session_today() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let state = state_of(vec![entry("kho", Stage::Day, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        drain(&mut session, Answer::Pass);
        assert_eq!(asked(&deck, session.into_state(), TODAY), vec![]);
    }

    #[test]
    fn a_partly_passed_term_resumes_with_its_unpassed_facets_only() {
        let deck = deck_of(vec![noun("sati 1.1", 1)]);
        let mut progress = entry("sati 1.1", Stage::Week, day(TODAY));
        progress.passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
        assert_eq!(
            asked(&deck, state_of(vec![progress]), TODAY),
            vec![
                pair("sati 1.1", Facet::Forms),
                pair("sati 1.1", Facet::Context)
            ]
        );
    }

    #[test]
    fn pass_then_fail_on_one_term_ends_at_day() {
        let deck = deck_of(vec![noun("sati 1.1", 1)]);
        let state = state_of(vec![entry("sati 1.1", Stage::Quarter, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        session.next();
        session
            .answer(&id("sati 1.1"), Facet::Meaning, Answer::Pass)
            .unwrap();
        session.next();
        session
            .answer(&id("sati 1.1"), Facet::Recall, Answer::Fail)
            .unwrap();
        let progress = session.into_state().progress.remove(0);
        assert_eq!(progress.stage, Stage::Day);
        assert_eq!(progress.due, day(TODAY + 1));
        assert_eq!(progress.lapses, 1);
    }

    #[test]
    fn next_keeps_the_same_question_until_it_is_answered() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let state = state_of(vec![entry("kho", Stage::Day, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        let first = session.next().cloned();
        assert_eq!(session.next().cloned(), first);
        assert_eq!(first.unwrap().facet, Facet::Meaning);
    }

    #[test]
    fn a_forms_facet_is_asked_only_when_the_term_has_a_regular_form() {
        let rare_only = Term {
            forms: vec![rare_form("fem acc pl", "satibhi")],
            ..indeclinable("rare", 3)
        };
        let deck = deck_of(vec![noun("sati 1.1", 1), indeclinable("kho", 2), rare_only]);
        let state = state_of(vec![
            entry("sati 1.1", Stage::Day, day(TODAY)),
            entry("kho", Stage::Day, day(TODAY)),
            entry("rare", Stage::Day, day(TODAY)),
        ]);
        let forms: Vec<String> = asked(&deck, state, TODAY)
            .into_iter()
            .filter(|(_, facet)| *facet == Facet::Forms)
            .map(|(term, _)| term)
            .collect();
        assert_eq!(forms, vec!["sati 1.1"]);
    }

    // ---- deck skew (T9, session half) ----

    #[test]
    fn a_term_that_lost_a_facet_still_reaches_the_next_stage() {
        // The state remembers Meaning and Forms; the deck term has no forms now.
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let mut progress = entry("kho", Stage::Week, day(TODAY));
        progress.passed = [Facet::Meaning, Facet::Forms].into_iter().collect();
        let mut session = start(&deck, state_of(vec![progress]), TODAY);
        assert_eq!(
            drain(&mut session, Answer::Pass),
            vec![("kho".to_string(), Facet::Recall, false)]
        );
        let progress = session.into_state().progress.remove(0);
        assert_eq!(progress.stage, Stage::Month);
        assert_eq!(progress.due, day(TODAY + 30));
    }

    #[test]
    fn a_due_term_with_nothing_left_to_ask_is_promoted_at_start() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let mut progress = entry("kho", Stage::Week, day(TODAY));
        progress.passed = [Facet::Meaning, Facet::Recall, Facet::Forms]
            .into_iter()
            .collect();
        let mut session = start(&deck, state_of(vec![progress]), TODAY);
        assert_eq!(session.next(), None);
        let progress = session.into_state().progress.remove(0);
        assert_eq!(progress.stage, Stage::Month);
        assert_eq!(progress.due, day(TODAY + 30));
        assert_eq!(progress.passed, FacetSet::empty());
    }

    #[test]
    fn start_reports_whether_it_changed_the_state() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let mut complete = entry("kho", Stage::Week, day(TODAY));
        complete.passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
        let promoted = start(&deck, state_of(vec![complete]), TODAY);
        assert!(promoted.changed_at_start());
        let untouched = [
            state_of(vec![entry("kho", Stage::Week, day(TODAY))]), // due, facets left
            state_of(vec![entry("kho", Stage::Week, day(TODAY + 3))]), // not due
            State::empty(),
        ];
        for state in untouched {
            assert!(!start(&deck, state, TODAY).changed_at_start());
        }
    }

    // ---- T6: refusals, orphans, last_today ----

    #[test]
    fn a_today_before_the_last_one_is_refused_naming_both_dates() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let mut state = state_of(vec![]);
        state.last_today = Some(day(TODAY));
        let error = Session::start(&deck, state.clone(), day(TODAY - 1)).unwrap_err();
        assert_eq!(
            error,
            SessionError::TodayBeforeLast {
                today: day(TODAY - 1),
                last: day(TODAY)
            }
        );
        let message = error.to_string();
        assert!(message.contains("2026-10-04"), "{message}");
        assert!(message.contains("2026-10-05"), "{message}");
        assert!(
            Session::start(&deck, state, day(TODAY)).is_ok(),
            "same day is fine"
        );
    }

    #[test]
    fn into_state_records_last_today() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let session = start(&deck, State::empty(), TODAY);
        assert_eq!(session.today(), day(TODAY));
        assert_eq!(session.into_state().last_today, Some(day(TODAY)));
    }

    #[test]
    fn answering_something_other_than_the_current_question_is_refused() {
        let deck = deck_of(vec![indeclinable("kho", 1), indeclinable("ca", 2)]);
        let state = state_of(vec![
            entry("kho", Stage::Day, day(TODAY)),
            entry("ca", Stage::Day, day(TODAY)),
        ]);
        let mut session = start(&deck, state, TODAY);
        let current = session.next().cloned().unwrap();
        assert_eq!(
            (current.term.0.as_str(), current.facet),
            ("kho", Facet::Meaning)
        );
        let before = session.snapshot();
        for (term, facet) in [
            ("kho", Facet::Recall),
            ("ca", Facet::Meaning),
            ("zzz", Facet::Meaning),
        ] {
            let error = session.answer(&id(term), facet, Answer::Pass).unwrap_err();
            assert_eq!(
                error,
                SessionError::NotCurrent {
                    asked: Some((id("kho"), Facet::Meaning)),
                    given: (id(term), facet)
                }
            );
            assert!(error.to_string().contains(term));
            assert_eq!(session.snapshot(), before, "state untouched");
        }
        assert_eq!(session.next().cloned(), Some(current));
    }

    #[test]
    fn answering_after_the_session_is_over_is_refused() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let mut session = start(&deck, State::empty(), TODAY);
        let error = session
            .answer(&id("kho"), Facet::Meaning, Answer::Pass)
            .unwrap_err();
        assert!(matches!(
            error,
            SessionError::NotCurrent { asked: None, .. }
        ));
        assert!(error.to_string().contains("no question"));
    }

    #[test]
    fn an_orphan_is_kept_never_asked_and_still_in_the_state() {
        let deck = deck_of(vec![indeclinable("kho", 1)]);
        let ghost = entry("ghost", Stage::Quarter, day(TODAY - 5));
        let state = state_of(vec![ghost.clone(), entry("kho", Stage::Day, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        let seen = drain(&mut session, Answer::Pass);
        assert!(seen.iter().all(|(term, _, _)| term == "kho"));
        let state = session.into_state();
        assert_eq!(state.progress[0], ghost);
        assert_eq!(state.progress.len(), 2);
    }

    #[test]
    fn a_wrong_pair_while_the_practice_question_is_current_is_refused() {
        let deck = deck_of(vec![indeclinable("kho", 1), indeclinable("ca", 2)]);
        let state = state_of(vec![entry("kho", Stage::Week, day(TODAY))]);
        let mut session = start(&deck, state, TODAY);
        session.next();
        session
            .answer(&id("kho"), Facet::Meaning, Answer::Fail)
            .unwrap();
        let practice = session.next().cloned().unwrap();
        assert!(practice.practice);
        assert_eq!(
            (practice.term.0.as_str(), practice.facet),
            ("kho", Facet::Meaning)
        );
        let before = session.snapshot();
        for (term, facet) in [("kho", Facet::Recall), ("ca", Facet::Meaning)] {
            let error = session.answer(&id(term), facet, Answer::Pass).unwrap_err();
            assert_eq!(
                error,
                SessionError::NotCurrent {
                    asked: Some((id("kho"), Facet::Meaning)),
                    given: (id(term), facet)
                }
            );
            assert_eq!(session.snapshot(), before, "state untouched");
        }
        assert_eq!(session.next().cloned(), Some(practice), "still pending");
    }

    #[test]
    fn after_a_fail_next_is_never_another_facet_of_the_failed_term() {
        // Month term of four facets, failed on the first: the three others
        // are queued behind it.
        let deck = deck_of(vec![noun("sati 1.1", 1), indeclinable("kho", 2)]);
        let state = state_of(vec![
            entry("sati 1.1", Stage::Month, day(TODAY - 1)),
            entry("kho", Stage::Day, day(TODAY)),
        ]);
        let mut session = start(&deck, state, TODAY);
        assert_eq!(session.queue.len(), 6, "sati: 4 facets, kho: 2");
        session.next();
        session
            .answer(&id("sati 1.1"), Facet::Meaning, Answer::Fail)
            .unwrap();
        assert!(
            session.queue.iter().all(|q| q.term != id("sati 1.1")),
            "the queue holds none of the failed term's facets"
        );
        assert_eq!(
            session.current, None,
            "answering clears the current question"
        );
        let next = session.next().cloned().unwrap();
        assert_eq!(next.term, id("kho"));
        assert!(!next.practice);
        let rest = drain(&mut session, Answer::Pass);
        let failed: Vec<_> = rest
            .iter()
            .filter(|(term, _, _)| term == "sati 1.1")
            .collect();
        assert_eq!(
            failed,
            vec![&("sati 1.1".to_string(), Facet::Meaning, true)],
            "only the practice of the failed facet"
        );
    }

    #[test]
    fn the_unknown_term_error_names_the_term() {
        assert!(
            SessionError::UnknownTerm(id("zzz"))
                .to_string()
                .contains("zzz")
        );
    }
}
