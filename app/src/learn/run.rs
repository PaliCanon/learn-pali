//! A review session as the review page drives it ([`ReviewRun`]): the
//! question shown, its presentation key, the grade saved before the next
//! question, the header's counts and the summary at the end.

use learn_core::added::AddError;
use learn_core::deck::{Deck, Form, Term, TermId};
use learn_core::facet::Facet;
use learn_core::forms::forms_question;
use learn_core::progress::State;
use learn_core::session::{Answer, Question, Session};

use crate::deck::pinned;
use crate::learn::home_state::{due_cards, new_left};
use crate::learn::key::presentation_key;
use crate::learn::summary::{Summary, summary};
use crate::store::Load;

/// Why [`ReviewRun::start_word`] did not add a word; the run is as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NotAdded {
    /// The word is already started.
    Already(AddError),
    /// The store did not take the save.
    NotSaved,
}

/// What the card of the current question shows, resolved on the deck or
/// the added words.
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

/// A [`Session`] on one deck, with what the page shows around it.
#[derive(Debug)]
pub(crate) struct ReviewRun {
    session: Session<'static>,
    /// The deck the session started on: the one selected then, whatever is
    /// selected later.
    deck: &'static Deck,
    before: State,
    current: Option<Question>,
    counter: u32,
    done: usize,
    /// Practice questions queued by a Fail and not answered yet (the
    /// session keeps them private): the header counts them (Q1).
    practice_pending: usize,
    /// A practice ahead ([`Session::practice`]): every question is
    /// practice, nothing in the progress changes.
    practice: bool,
}

impl ReviewRun {
    /// A run over `session` (just started); `before` is the state it
    /// started from, for the summary. The first question is taken at once.
    pub(crate) fn new(
        mut session: Session<'static>,
        deck: &'static Deck,
        before: State,
    ) -> ReviewRun {
        let current = session.next().cloned();
        ReviewRun {
            session,
            deck,
            before,
            current,
            counter: 0,
            done: 0,
            practice_pending: 0,
            practice: false,
        }
    }

    /// A run over a practice ahead (`session` from [`Session::practice`]):
    /// its header counts every question it asks, its end says 'Practice
    /// done'.
    pub(crate) fn practice(
        session: Session<'static>,
        deck: &'static Deck,
        before: State,
    ) -> ReviewRun {
        ReviewRun {
            practice: true,
            ..ReviewRun::new(session, deck, before)
        }
    }

    /// True for a practice ahead ([`ReviewRun::practice`]).
    pub(crate) fn is_practice(&self) -> bool {
        self.practice
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
        let term = pinned(self.session.term(&question.term)?);
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

    /// Adds `term` to the practice through the session and starts it, as
    /// [`ReviewRun::add_new`] starts a deck term (due today, its questions
    /// queued), so the next answer's snapshot keeps it, and gives the state
    /// to `save` at once (not called when the word is refused). `save` says
    /// whether the store took it; when it did not, the word is taken back
    /// and not counted.
    ///
    /// # Errors
    /// [`NotAdded`]: the word is already started, or the store did not take
    /// the save.
    pub(crate) fn start_word(
        &mut self,
        term: Term,
        save: impl FnOnce(&State) -> bool,
    ) -> Result<(), NotAdded> {
        let id = term.id.clone();
        self.session.start_word(term).map_err(NotAdded::Already)?;
        if save(&self.session.snapshot()) {
            Ok(())
        } else {
            self.session.withdraw_word(&id);
            Err(NotAdded::NotSaved)
        }
    }

    /// True when `id` is started: it has progress, deck term or added word.
    pub(crate) fn started(&self, id: &TermId) -> bool {
        self.session.is_started(id)
    }

    /// The terms not started yet, the deck's and the added words: what 'Add
    /// new terms' may add.
    pub(crate) fn new_left(&self) -> usize {
        new_left(&Load::Ok(self.session.snapshot()), self.deck)
    }

    /// The answers given so far.
    pub(crate) fn done(&self) -> usize {
        self.done
    }

    /// The header's total (Q1): done + the snapshot's due cards + the
    /// practice still pending, recomputed after each answer (a Fail drops
    /// the term's other queued cards, so the queue at start would
    /// overcount). A practice ahead counts done + what is left to ask.
    pub(crate) fn total(&self) -> usize {
        if self.practice {
            return self.done + self.session.remaining();
        }
        let snapshot = self.session.snapshot();
        self.done + due_cards(self.deck, &snapshot, self.session.today()) + self.practice_pending
    }

    /// What the session changed, from the state it started from to the
    /// snapshot ([`crate::learn::summary::summary`]).
    pub(crate) fn summary(&self) -> Summary {
        summary(&self.before, &self.session.snapshot(), self.deck)
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

    use super::{NotAdded, ReviewRun};
    use crate::deck::deck;

    /// `(term, facet, practice)`.
    type Asked = (String, Facet, bool);

    fn today() -> Date {
        Date::parse("2026-10-05").unwrap()
    }

    /// A run on the embedded deck over `n` new terms: `kho` and `ca 2.1`
    /// are indeclinables, so each asks Meaning, Recall, Context (3 cards).
    fn started(n: usize) -> ReviewRun {
        let mut session = Session::start(deck(), State::empty(), today(), 0).unwrap();
        session.request_new(n);
        ReviewRun::new(session, deck(), State::empty())
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
        let session = Session::start(deck(), State::empty(), today(), 0).unwrap();
        let mut run = ReviewRun::new(session, deck(), State::empty());
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
        let session = Session::start(deck(), State::empty(), today(), 0).unwrap();
        let mut run = ReviewRun::new(session, deck(), State::empty());
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
        let session = Session::start(deck(), before.clone(), today(), 0).unwrap();
        ReviewRun::new(session, deck(), before)
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
        let session = Session::start(deck(), State::empty(), today(), 0).unwrap();
        assert!(
            ReviewRun::new(session, deck(), State::empty())
                .card()
                .is_none()
        );
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

    #[test]
    fn a_word_added_in_the_run_is_saved_started_and_kept_by_the_next_answer() {
        let mut run = started(1);
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        let word = crate::test_support::added_word();
        let (left, total) = (run.new_left(), run.total());
        run.start_word(word.clone(), |state| {
            saved.borrow_mut().push(state.clone());
            true
        })
        .unwrap();
        {
            let saved = saved.borrow();
            assert_eq!(saved.len(), 1, "saved at once");
            assert_eq!(saved[0].added, vec![word.clone()]);
            let progress = saved[0].get(&word.id).expect("started");
            assert_eq!((progress.stage, progress.due), (Stage::Day, today()));
        }
        assert!(run.started(&word.id));
        assert_eq!(run.new_left(), left, "it is not a new term any more");
        assert_eq!(run.total(), total + 2, "its two facets are due today");
        run.grade(Answer::Pass, |state| saved.borrow_mut().push(state.clone()));
        let saved = saved.into_inner();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[1].added, vec![word.clone()], "the snapshot keeps it");
        assert!(saved[1].get(&word.id).is_some(), "and keeps it started");
    }

    #[test]
    fn a_deck_term_not_started_is_started_by_the_run_and_adds_nothing() {
        let mut run = started(1);
        let mut deck_term = crate::test_support::added_word();
        deck_term.id = TermId("rūpa 1".into());
        assert!(!run.started(&deck_term.id), "rūpa 1 is not started yet");
        let left = run.new_left();
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        run.start_word(deck_term.clone(), |state| {
            saved.borrow_mut().push(state.clone());
            true
        })
        .unwrap();
        let saved = saved.into_inner();
        assert_eq!(saved[0].added.len(), 0, "the deck teaches it");
        assert!(saved[0].get(&deck_term.id).is_some());
        assert!(run.started(&deck_term.id));
        assert_eq!(run.new_left(), left - 1);
    }

    #[test]
    fn a_word_already_started_saves_nothing() {
        let mut run = started(1);
        let saved = Cell::new(0);
        let first = run.card().expect("a card").term.id.clone();
        let mut deck_term = crate::test_support::added_word();
        deck_term.id = first;
        assert!(
            run.start_word(deck_term, |_| {
                saved.set(saved.get() + 1);
                true
            })
            .is_err()
        );
        let word = crate::test_support::added_word();
        run.start_word(word.clone(), |_| {
            saved.set(saved.get() + 1);
            true
        })
        .unwrap();
        assert!(
            run.start_word(word, |_| {
                saved.set(saved.get() + 1);
                true
            })
            .is_err()
        );
        assert_eq!(saved.get(), 1);
    }

    #[test]
    fn a_word_started_in_the_run_is_asked_in_this_run() {
        let mut run = started(1);
        let word = crate::test_support::added_word();
        run.start_word(word.clone(), |_| true).unwrap();
        let mut seen = Vec::new();
        while let Some((term, _, _)) = asked(&run) {
            seen.push(term);
            grade(&mut run, Answer::Pass);
        }
        assert_eq!(
            seen.iter().filter(|term| **term == word.id.0).count(),
            2,
            "both facets, after the deck's own card"
        );
    }

    /// A run over a state holding the started added word, due today.
    fn on_added_word() -> ReviewRun {
        let before = crate::test_support::added_started(Stage::Day, today());
        let session = Session::start(deck(), before.clone(), today(), 0).unwrap();
        ReviewRun::new(session, deck(), before)
    }

    #[test]
    fn the_card_of_an_added_word_carries_its_term() {
        let run = on_added_word();
        let card = run.card().unwrap();
        assert_eq!(card.term.lemma, "sānuvajja");
        assert_eq!(card.term.meaning.en, "blameworthy");
        assert_eq!(card.facet, Facet::Meaning);
        // The same word again is the same term: nothing leaks twice.
        assert!(std::ptr::eq(card.term, run.card().unwrap().term));
    }

    #[test]
    fn an_added_word_with_its_example_is_asked_in_context_with_the_blank() {
        use learn_core::deck::{Blank, Example};
        let mut word = crate::test_support::added_word();
        word.example = Some(Example {
            text: "natthi sānuvajjo loke".into(),
            source: "AN 1".into(),
            sutta: "an1".into(),
            blank: Some(Blank {
                before: "natthi ".into(),
                answer: "sānuvajjo".into(),
                after: " loke".into(),
            }),
            translations: std::collections::BTreeMap::new(),
        });
        let mut before = State::empty();
        before.added.push(word.clone());
        before
            .progress
            .push(Progress::new(word.id.clone(), today()));
        let session = Session::start(deck(), before.clone(), today(), 0).unwrap();
        let mut run = ReviewRun::new(session, deck(), before);
        let mut facets = Vec::new();
        while let Some(card) = run.card() {
            if card.facet == Facet::Context {
                let blank = card.term.example.as_ref().unwrap().blank.as_ref().unwrap();
                assert_eq!(blank.answer, "sānuvajjo");
                assert_eq!(card.term.example.as_ref().unwrap().source, "AN 1");
            }
            facets.push(card.facet);
            grade(&mut run, Answer::Pass);
        }
        facets.sort();
        assert_eq!(facets, vec![Facet::Meaning, Facet::Recall, Facet::Context]);
    }

    #[test]
    fn an_added_word_is_asked_and_saved_like_a_deck_term() {
        let mut run = on_added_word();
        assert_eq!(run.total(), 2);
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        run.grade(Answer::Pass, |state| saved.borrow_mut().push(state.clone()));
        run.grade(Answer::Pass, |state| saved.borrow_mut().push(state.clone()));
        let saved = saved.into_inner();
        assert_eq!(saved.last().unwrap().progress[0].stage, Stage::Week);
        assert_eq!(saved.last().unwrap().added.len(), 1, "the word stays");
        assert!(run.card().is_none());
    }

    #[test]
    fn new_terms_count_and_start_the_added_words_after_the_deck() {
        let mut before = State::empty();
        before.added.push(crate::test_support::added_word());
        let session = Session::start(deck(), before.clone(), today(), 0).unwrap();
        let mut run = ReviewRun::new(session, deck(), before);
        assert_eq!(run.new_left(), 51);
        assert_eq!(run.add_new(51, |_| ()), 51);
        assert_eq!(run.new_left(), 0);
    }

    #[test]
    fn a_word_the_store_refused_is_taken_back_and_not_counted() {
        let mut run = started(1);
        let word = crate::test_support::added_word();
        let (left, total) = (run.new_left(), run.total());
        let saved: RefCell<Vec<State>> = RefCell::new(Vec::new());
        assert_eq!(
            run.start_word(word.clone(), |state| {
                saved.borrow_mut().push(state.clone());
                false
            }),
            Err(NotAdded::NotSaved)
        );
        assert!(!run.started(&word.id));
        assert_eq!((run.new_left(), run.total()), (left, total));
        run.grade(Answer::Pass, |state| saved.borrow_mut().push(state.clone()));
        let saved = saved.into_inner();
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[1].added.len(), 0, "the next answer does not carry it");
        assert_eq!(saved[1].get(&word.id), None);
        // It can be added again once the store takes it.
        assert_eq!(run.start_word(word, |_| true), Ok(()));
    }
}
