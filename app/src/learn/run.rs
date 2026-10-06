//! A review session as the review page drives it ([`ReviewRun`]): the
//! question shown, its presentation key, the grade saved before the next
//! question, the header's counts and the summary at the end.

use learn_core::deck::{Form, Term};
use learn_core::facet::Facet;
use learn_core::forms::forms_question;
use learn_core::progress::State;
use learn_core::session::{Answer, Question, Session};

use crate::deck::deck;
use crate::learn::home_state::{due_cards, new_left};
use crate::learn::key::presentation_key;
use crate::learn::summary::{Summary, summary};
use crate::store::Load;

/// What the card of the current question shows, resolved on the deck.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Card {
    /// The facet asked.
    pub(crate) facet: Facet,
    /// The second asking of a failed facet: its answer changes nothing.
    pub(crate) practice: bool,
    /// The term asked.
    pub(crate) term: &'static Term,
    /// For a Forms question, the form asked at the term's stage
    /// ([`forms_question`]); `None` for the other facets.
    pub(crate) form: Option<&'static Form>,
}

/// A [`Session`] on the embedded deck, with what the page shows around it.
#[derive(Debug)]
pub(crate) struct ReviewRun {
    session: Session<'static>,
    before: State,
    current: Option<Question>,
    counter: u32,
    done: usize,
    /// Practice questions queued by a Fail and not answered yet (the
    /// session keeps them private): the header counts them (Q1).
    practice_pending: usize,
}

impl ReviewRun {
    /// A run over `session` (just started); `before` is the state it
    /// started from, for the summary. The first question is taken at once.
    pub(crate) fn new(mut session: Session<'static>, before: State) -> ReviewRun {
        let current = session.next().cloned();
        ReviewRun {
            session,
            before,
            current,
            counter: 0,
            done: 0,
            practice_pending: 0,
        }
    }

    /// The question shown, `None` once the session is over (or when
    /// nothing was due: then [`ReviewRun::done`] is 0).
    pub(crate) fn current(&self) -> Option<&Question> {
        self.current.as_ref()
    }

    /// The current presentation's key ([`presentation_key`] of the current
    /// question and the counter); empty when there is no question.
    pub(crate) fn key(&self) -> String {
        self.current.as_ref().map_or_else(String::new, |question| {
            presentation_key(&question.term, question.facet, self.counter)
        })
    }

    /// Settles the current question with `answer`, calls `save` with the
    /// session's snapshot (before the question moves), then moves to the
    /// next question (the counter rises: a new key). Nothing happens when
    /// no question is pending.
    pub(crate) fn grade(&mut self, answer: Answer, save: impl FnOnce(&State)) {
        let Some(question) = self.current.clone() else {
            return;
        };
        if self
            .session
            .answer(&question.term, question.facet, answer)
            .is_err()
        {
            return;
        }
        save(&self.session.snapshot());
        if question.practice {
            self.practice_pending = self.practice_pending.saturating_sub(1);
        } else if answer == Answer::Fail {
            self.practice_pending += 1;
        }
        self.done += 1;
        self.counter += 1;
        self.current = self.session.next().cloned();
    }

    /// The card of the current question, `None` when nothing is asked.
    pub(crate) fn card(&self) -> Option<Card> {
        let question = self.current.as_ref()?;
        let term = deck().term(&question.term)?;
        let form = (question.facet == Facet::Forms)
            .then(|| {
                let state = self.session.snapshot();
                state
                    .get(&question.term)
                    .and_then(|progress| forms_question(term, progress))
            })
            .flatten();
        Some(Card {
            facet: question.facet,
            practice: question.practice,
            term,
            form,
        })
    }

    /// Starts up to `n` new terms of the deck after the ones in the
    /// session, saves the state (`save` is not called when none was
    /// started) and goes on with their first question; the session need
    /// not be over. Returns how many were started.
    pub(crate) fn add_new(&mut self, n: usize, save: impl FnOnce(&State)) -> usize {
        let added = self.session.request_new(n);
        if added == 0 {
            return 0;
        }
        save(&self.session.snapshot());
        if self.current.is_none() {
            self.current = self.session.next().cloned();
        }
        added
    }

    /// The deck's terms not started yet: what 'Add new terms' may add.
    pub(crate) fn new_left(&self) -> usize {
        new_left(&Load::Ok(self.session.snapshot()), deck())
    }

    /// The answers given so far.
    pub(crate) fn done(&self) -> usize {
        self.done
    }

    /// The header's total (Q1): done + the snapshot's due cards + the
    /// practice still pending, recomputed after each answer (a Fail drops
    /// the term's other queued cards, so the queue at start would
    /// overcount).
    pub(crate) fn total(&self) -> usize {
        let snapshot = self.session.snapshot();
        self.done + due_cards(deck(), &snapshot, self.session.today()) + self.practice_pending
    }

    /// What the session changed, from the state it started from to the
    /// snapshot ([`crate::learn::summary::summary`]).
    pub(crate) fn summary(&self) -> Summary {
        summary(&self.before, &self.session.snapshot(), deck())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use learn_core::date::Date;
    use learn_core::deck::TermId;
    use learn_core::facet::{Facet, FacetSet};
    use learn_core::progress::{Progress, State};
    use learn_core::session::{Answer, Session};
    use learn_core::stage::Stage;

    use super::ReviewRun;
    use crate::deck::deck;

    /// `(term, facet, practice)`.
    type Asked = (String, Facet, bool);

    fn today() -> Date {
        Date::parse("2026-10-05").unwrap()
    }

    /// A run on the embedded deck over `n` new terms: `kho` and `ca 2.1`
    /// are indeclinables, so each asks Meaning, Recall, Context (3 cards).
    fn started(n: usize) -> ReviewRun {
        let mut session = Session::start(deck(), State::empty(), today()).unwrap();
        session.request_new(n);
        ReviewRun::new(session, State::empty())
    }

    /// What is asked now, as `(term, facet, practice)`.
    fn asked(run: &ReviewRun) -> Option<Asked> {
        run.current()
            .map(|q| (q.term.0.clone(), q.facet, q.practice))
    }

    fn grade(run: &mut ReviewRun, answer: Answer) {
        run.grade(answer, |_| ());
    }

    #[allow(clippy::unnecessary_wraps, reason = "compared with `asked`, an Option")]
    fn pair(term: &str, facet: Facet, practice: bool) -> Option<Asked> {
        Some((term.to_string(), facet, practice))
    }

    #[test]
    fn a_new_run_asks_the_first_question_of_the_queue() {
        let run = started(2);
        assert_eq!(asked(&run), pair("kho", Facet::Meaning, false));
        assert_eq!(run.key(), "kho|Meaning|0");
        assert_eq!(run.done(), 0);
        assert_eq!(run.total(), 6);
    }

    #[test]
    fn the_counter_rises_with_every_grade_so_the_key_changes() {
        let mut run = started(2);
        let mut keys = vec![run.key()];
        for _ in 0..3 {
            grade(&mut run, Answer::Pass);
            keys.push(run.key());
        }
        assert_eq!(
            keys,
            [
                "kho|Meaning|0",
                "kho|Recall|1",
                "kho|Context|2",
                "ca 2.1|Meaning|3"
            ]
        );
    }

    #[test]
    fn a_practice_of_the_same_term_and_facet_gets_a_new_key() {
        let mut run = started(2);
        let first = run.key();
        grade(&mut run, Answer::Fail);
        for _ in 0..3 {
            assert_eq!(asked(&run).map(|(term, _, _)| term), Some("ca 2.1".into()));
            grade(&mut run, Answer::Pass);
        }
        assert_eq!(asked(&run), pair("kho", Facet::Meaning, true));
        assert_eq!(run.key(), "kho|Meaning|4");
        assert_ne!(run.key(), first);
    }

    #[test]
    fn grade_saves_the_state_after_the_answer_once_and_then_moves_on() {
        let mut run = started(2);
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        run.grade(Answer::Pass, |state| saved.borrow_mut().push(state.clone()));
        let saved = saved.into_inner();
        assert_eq!(saved.len(), 1, "saved once per grade");
        let kho = saved[0]
            .progress
            .iter()
            .find(|p| p.term.0 == "kho")
            .unwrap();
        assert!(kho.passed.contains(Facet::Meaning), "the answer is in it");
        assert!(!kho.passed.contains(Facet::Recall), "and nothing more");
        assert_eq!(saved[0].last_today, Some(today()), "the day is recorded");
        assert_eq!(
            asked(&run),
            pair("kho", Facet::Recall, false),
            "then it moved"
        );
    }

    #[test]
    fn the_state_is_saved_before_the_question_moves() {
        let mut run = started(2);
        let order = RefCell::new(Vec::new());
        let calls = Cell::new(0);
        run.grade(Answer::Pass, |_| {
            calls.set(calls.get() + 1);
            order.borrow_mut().push("save");
        });
        order.borrow_mut().push("moved");
        assert_eq!(order.into_inner(), ["save", "moved"]);
        assert_eq!(calls.get(), 1);
        assert_eq!(run.done(), 1);
    }

    #[test]
    fn a_practice_answer_is_saved_too_and_changes_nothing() {
        let mut run = started(1);
        grade(&mut run, Answer::Fail);
        assert_eq!(asked(&run), pair("kho", Facet::Meaning, true));
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        run.grade(Answer::Pass, |state| saved.borrow_mut().push(state.clone()));
        let saved = saved.into_inner();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].progress[0].lapses, 1);
        assert_eq!(saved[0].progress[0].passed, FacetSet::empty());
        assert_eq!(asked(&run), None, "the session is over");
    }

    #[test]
    fn the_total_is_done_plus_what_is_due_plus_the_practice_pending() {
        let mut run = started(2);
        assert_eq!((run.done(), run.total()), (0, 6));
        grade(&mut run, Answer::Pass);
        assert_eq!((run.done(), run.total()), (1, 6));
        // A fail sends kho back to Day: its other card (Context) leaves the
        // queue and one practice is pending: 2 done + 3 of ca + 1 practice.
        grade(&mut run, Answer::Fail);
        assert_eq!((run.done(), run.total()), (2, 6));
        for _ in 0..3 {
            grade(&mut run, Answer::Pass);
        }
        assert_eq!((run.done(), run.total()), (5, 6));
        assert_eq!(asked(&run), pair("kho", Facet::Recall, true));
        grade(&mut run, Answer::Fail);
        assert_eq!((run.done(), run.total()), (6, 6));
        assert_eq!(asked(&run), None);
    }

    #[test]
    fn the_first_fail_corrects_the_queue_at_start() {
        let mut run = started(2);
        assert_eq!(run.total(), 6);
        grade(&mut run, Answer::Fail);
        assert_eq!((run.done(), run.total()), (1, 1 + 3 + 1));
    }

    #[test]
    fn the_summary_counts_terms_from_the_state_before_and_after() {
        let mut run = started(2);
        grade(&mut run, Answer::Fail); // kho back to Day, lapses 1
        for _ in 0..3 {
            grade(&mut run, Answer::Pass); // ca 2.1 climbs
        }
        grade(&mut run, Answer::Pass); // the practice
        assert_eq!(asked(&run), None);
        let summary = run.summary();
        assert_eq!(summary.moved_up, 1);
        assert_eq!(summary.back_to_day, 1);
        assert_eq!(summary.next_due, Some(Date::parse("2026-10-06").unwrap()));
    }

    #[test]
    fn nothing_due_is_an_empty_run_with_no_key_and_no_count() {
        let session = Session::start(deck(), State::empty(), today()).unwrap();
        let mut run = ReviewRun::new(session, State::empty());
        assert_eq!(asked(&run), None);
        assert_eq!((run.done(), run.total()), (0, 0));
        assert_eq!(run.key(), "");
        let saved = Cell::new(false);
        run.grade(Answer::Pass, |_| saved.set(true));
        assert!(!saved.get(), "no question, nothing to save");
        assert_eq!(run.done(), 0);
    }

    #[test]
    fn grading_an_empty_run_saves_nothing_whatever_the_answer() {
        let session = Session::start(deck(), State::empty(), today()).unwrap();
        let mut run = ReviewRun::new(session, State::empty());
        let saves = Cell::new(0_usize);
        for answer in [Answer::Pass, Answer::Fail] {
            run.grade(answer, |_| saves.set(saves.get() + 1));
        }
        assert_eq!(saves.get(), 0, "no question, no save");
        assert_eq!(run.done(), 0);
        assert_eq!(run.key(), "");
    }

    /// A run on one started term whose facets `passed` are already
    /// passed: only the others are asked.
    fn run_on(term: &str, passed: &[Facet], stage: Stage) -> ReviewRun {
        let mut entry = Progress::new(TermId(term.to_string()), today());
        entry.stage = stage;
        for facet in passed {
            entry.passed.insert(*facet);
        }
        let mut before = State::empty();
        before.progress.push(entry);
        let session = Session::start(deck(), before.clone(), today()).unwrap();
        ReviewRun::new(session, before)
    }

    #[test]
    fn the_card_resolves_the_current_question_on_the_deck() {
        let run = started(1);
        let card = run.card().unwrap();
        assert_eq!(card.facet, Facet::Meaning);
        assert!(!card.practice);
        assert_eq!(card.term.lemma, "kho");
        assert_eq!(card.form.map(|form| form.label.as_str()), None);
    }

    #[test]
    fn a_forms_card_carries_the_form_asked_at_the_terms_stage() {
        let others = [Facet::Meaning, Facet::Recall, Facet::Context];
        let run = run_on("rūpa 1", &others, Stage::Quarter);
        let card = run.card().unwrap();
        assert_eq!(card.facet, Facet::Forms);
        assert_eq!(card.form.map(|form| form.label.as_str()), Some("nt acc pl"));
        let day = run_on("rūpa 1", &others, Stage::Day);
        let label = day
            .card()
            .and_then(|card| card.form)
            .map(|f| f.label.clone());
        assert_eq!(label.as_deref(), Some("nt nom sg"));
    }

    #[test]
    fn there_is_no_card_when_nothing_is_asked() {
        let session = Session::start(deck(), State::empty(), today()).unwrap();
        assert!(ReviewRun::new(session, State::empty()).card().is_none());
    }

    #[test]
    fn new_terms_come_after_the_session_and_the_run_goes_on() {
        let mut run = started(1);
        for _ in 0..3 {
            grade(&mut run, Answer::Pass);
        }
        assert_eq!(asked(&run), None);
        assert_eq!(run.new_left(), 49);
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        let added = run.add_new(2, |state| saved.borrow_mut().push(state.clone()));
        assert_eq!(added, 2);
        let saved = saved.into_inner();
        assert_eq!(saved.len(), 1, "saved once");
        assert_eq!(saved[0].progress.len(), 3, "kho and the two new terms");
        assert_eq!(asked(&run), pair("ca 2.1", Facet::Meaning, false));
        assert_eq!(run.key(), "ca 2.1|Meaning|3");
        assert_eq!((run.done(), run.total()), (3, 9));
        assert_eq!(run.new_left(), 47);
    }

    #[test]
    fn adding_nothing_saves_nothing() {
        let mut run = started(50);
        assert_eq!(run.new_left(), 0);
        let saved = Cell::new(false);
        assert_eq!(run.add_new(5, |_| saved.set(true)), 0);
        assert!(!saved.get());
    }
}
