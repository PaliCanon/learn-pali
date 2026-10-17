//! Every place that resolves a started term goes through
//! [`State::term`](crate::progress::State::term): a started added word is
//! asked, counted and brought forward like a deck term. One test per site.

use crate::added::AddError;
use crate::date::Date;
use crate::deck::samples::{deck as deck_of, indeclinable, noun};
use crate::deck::{Deck, Term, TermId};
use crate::facet::{Facet, FacetSet};
use crate::progress::{Progress, State};
use crate::session::{Answer, Session, nearest};
use crate::stage::Stage;

const TODAY: u32 = 20731;

fn day(days: u32) -> Date {
    Date::from_days(days)
}

fn id(text: &str) -> TermId {
    TermId(text.to_string())
}

/// Two deck terms, `sati 1.1` (four facets, rank 1) and `kho` (two, rank 2).
fn deck() -> &'static Deck {
    static DECK: std::sync::OnceLock<Deck> = std::sync::OnceLock::new();
    DECK.get_or_init(|| deck_of(vec![noun("sati 1.1", 1), indeclinable("kho", 2)]))
}

/// An added word of two facets, at the rank an added word has.
fn word(name: &str) -> Term {
    indeclinable(name, u32::MAX)
}

fn entry(term: &str, stage: Stage, due: u32) -> Progress {
    Progress {
        term: id(term),
        stage,
        passed: FacetSet::empty(),
        due: day(due),
        lapses: 0,
    }
}

fn state(progress: Vec<Progress>, added: Vec<Term>) -> State {
    State {
        progress,
        added,
        ..State::empty()
    }
}

fn start(deck: &Deck, state: State) -> Session<'_> {
    Session::start(deck, state, day(TODAY), 7).unwrap()
}

/// Answers every queue question Pass; returns the terms asked, in order.
fn pass_all(session: &mut Session<'_>) -> Vec<String> {
    let mut asked = Vec::new();
    while let Some(question) = session.next().cloned() {
        asked.push(question.term.0.clone());
        session
            .answer(&question.term, question.facet, Answer::Pass)
            .unwrap();
    }
    asked
}

fn names(ids: &[TermId]) -> Vec<&str> {
    ids.iter().map(|id| id.0.as_str()).collect()
}

// ---- the due queue, settle_complete, the answers ----

#[test]
fn a_started_added_word_that_is_due_is_asked_on_all_its_facets() {
    let state = state(
        vec![entry("sānuvajja", Stage::Day, TODAY)],
        vec![word("sānuvajja")],
    );
    let mut session = start(deck(), state);
    assert_eq!(session.remaining(), 2);
    let asked = pass_all(&mut session);
    assert_eq!(asked, vec!["sānuvajja", "sānuvajja"]);
}

#[test]
fn an_added_word_not_due_is_not_asked() {
    let state = state(
        vec![entry("sānuvajja", Stage::Week, TODAY + 3)],
        vec![word("sānuvajja")],
    );
    assert_eq!(start(deck(), state).remaining(), 0);
}

#[test]
fn answering_an_added_word_moves_it_up_or_back_like_a_deck_term() {
    let state = state(
        vec![entry("sānuvajja", Stage::Day, TODAY)],
        vec![word("sānuvajja")],
    );
    let mut session = start(deck(), state);
    pass_all(&mut session);
    let moved = session.snapshot();
    let progress = moved.get(&id("sānuvajja")).unwrap();
    assert_eq!(
        (progress.stage, progress.due, progress.lapses),
        (Stage::Week, day(TODAY + 7), 0)
    );

    let state = self::state(
        vec![entry("sānuvajja", Stage::Week, TODAY)],
        vec![word("sānuvajja")],
    );
    let mut session = start(deck(), state);
    let question = session.next().cloned().unwrap();
    session
        .answer(&question.term, question.facet, Answer::Fail)
        .unwrap();
    let failed = session.snapshot();
    let progress = failed.get(&id("sānuvajja")).unwrap();
    assert_eq!(
        (progress.stage, progress.due, progress.lapses),
        (Stage::Day, day(TODAY + 1), 1)
    );
}

#[test]
fn an_added_word_with_nothing_left_to_ask_is_promoted_at_the_start() {
    let mut done = entry("sānuvajja", Stage::Week, TODAY);
    done.passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
    let state = state(vec![done], vec![word("sānuvajja")]);
    let session = start(deck(), state);
    assert!(session.changed_at_start());
    assert_eq!(session.remaining(), 0);
    let promoted = session.snapshot();
    let progress = promoted.get(&id("sānuvajja")).unwrap();
    assert_eq!(progress.stage, Stage::Month);
}

#[test]
fn the_session_resolves_a_term_in_the_deck_else_among_the_added_words() {
    let session = start(deck(), state(vec![], vec![word("w1"), word("kho")]));
    assert_eq!(session.term(&id("w1")).unwrap().rank, u32::MAX);
    assert_eq!(session.term(&id("kho")).unwrap().rank, 2, "the deck's wins");
    assert!(session.term(&id("missing")).is_none());
}

// ---- nearest, practice ----

#[test]
fn nearest_takes_added_words_after_deck_terms_on_a_tie() {
    let state = state(
        vec![
            entry("sānuvajja", Stage::Week, TODAY + 2),
            entry("kho", Stage::Week, TODAY + 2),
            entry("sati 1.1", Stage::Week, TODAY + 9),
        ],
        vec![word("sānuvajja")],
    );
    let ids = nearest(&state, deck(), None, 5);
    assert_eq!(names(&ids), ["kho", "sānuvajja", "sati 1.1"]);
    assert_eq!(
        names(&nearest(&state, deck(), Some(Stage::Week), 2)),
        ["kho", "sānuvajja"]
    );
}

#[test]
fn a_practice_asks_an_added_word_as_practice_only() {
    let d = deck();
    let state = state(
        vec![entry("sānuvajja", Stage::Week, TODAY + 2)],
        vec![word("sānuvajja")],
    );
    let mut session = Session::practice(d, &state, day(TODAY), &[id("sānuvajja")], 3);
    assert_eq!(session.remaining(), 2);
    assert!(session.next().unwrap().practice);
}

// ---- request_new: the deck by rank, then the added words in order ----

#[test]
fn new_terms_come_from_the_deck_by_rank_then_the_added_words_in_order() {
    let d = deck();
    let mut session = start(d, state(vec![], vec![word("w1"), word("w2")]));
    assert_eq!(session.request_new(3), 3);
    let started = session.snapshot();
    let order: Vec<&str> = started.progress.iter().map(|p| p.term.0.as_str()).collect();
    assert_eq!(order, ["sati 1.1", "kho", "w1"]);
    assert_eq!(session.stats().new_left, 1);
    assert_eq!(session.request_new(5), 1);
    assert_eq!(session.snapshot().progress[3].term, id("w2"));
    assert_eq!(session.request_new(5), 0);
    assert_eq!(session.stats().new_left, 0);
}

#[test]
fn a_new_added_word_is_queued_with_its_facets() {
    let d = deck();
    let mut session = start(d, state(vec![], vec![word("w1")]));
    session.request_new(3);
    let asked = pass_all(&mut session);
    assert_eq!(asked.iter().filter(|t| *t == "w1").count(), 2);
    assert_eq!(asked.len(), 4 + 2 + 2);
}

#[test]
fn request_new_goes_on_into_the_added_words_when_the_deck_runs_out() {
    let d = deck();
    // sati is started: kho is the deck's last new term, then w1 and w2.
    let mut session = start(
        d,
        state(
            vec![entry("sati 1.1", Stage::Day, TODAY + 1)],
            vec![word("w1"), word("w2")],
        ),
    );
    assert_eq!(session.request_new(2), 2, "kho, then w1");
    let started: Vec<String> = session
        .snapshot()
        .progress
        .iter()
        .map(|progress| progress.term.0.clone())
        .collect();
    assert_eq!(started, vec!["sati 1.1", "kho", "w1"]);
    assert_eq!(session.request_new(9), 1, "only w2 is left");
    assert_eq!(session.request_new(9), 0, "nothing is left");
}

#[test]
fn an_added_word_the_deck_also_has_is_not_offered_twice() {
    let d = deck();
    let mut session = start(d, state(vec![], vec![word("kho"), word("w1")]));
    // kho is a deck term: new_left counts sati, kho, w1 once each.
    assert_eq!(session.stats().new_left, 3);
    assert_eq!(session.request_new(9), 3);
    assert_eq!(session.snapshot().progress.len(), 3);
}

#[test]
fn an_added_word_already_started_is_not_new() {
    let d = deck();
    let session = start(
        d,
        state(
            vec![entry("w1", Stage::Day, TODAY + 1)],
            vec![word("w1"), word("w2")],
        ),
    );
    assert_eq!(session.stats().new_left, 2 + 1);
}

// ---- advance ----

fn all_deck_started_late() -> Vec<Progress> {
    vec![
        entry("sati 1.1", Stage::Quarter, TODAY + 40),
        entry("kho", Stage::Quarter, TODAY + 41),
    ]
}

#[test]
fn an_added_word_not_started_blocks_advance() {
    let d = deck();
    let mut session = start(d, state(all_deck_started_late(), vec![word("w1")]));
    assert_eq!(session.advance(3), 0, "w1 is still to start");
}

#[test]
fn advance_waits_until_every_added_word_is_started_like_the_decks_own() {
    let d = deck();
    let mut progress = all_deck_started_late();
    progress.push(entry("w1", Stage::Month, TODAY + 3));
    // w2 is added and not started: a new term to learn first, as a deck
    // term not started would be.
    let mut session = start(d, state(progress.clone(), vec![word("w1"), word("w2")]));
    assert_eq!(session.advance(3), 0);
    assert_eq!(session.request_new(1), 1, "w2 is started");
    pass_all(&mut session);
    // The queue is empty and nothing is unstarted: the brought-forward
    // practice is allowed again.
    assert_eq!(session.advance(3), 3);
}

#[test]
fn an_added_word_in_a_long_stage_is_brought_forward() {
    let d = deck();
    let mut progress = all_deck_started_late();
    progress.push(entry("w1", Stage::Month, TODAY + 3));
    let mut session = start(d, state(progress, vec![word("w1")]));
    assert_eq!(session.advance(1), 1);
    assert_eq!(session.snapshot().get(&id("w1")).unwrap().due, day(TODAY));
    assert_eq!(session.remaining(), 2);
}

#[test]
fn an_added_word_due_is_something_to_do_so_advance_is_refused() {
    let d = deck();
    let mut progress = all_deck_started_late();
    progress.push(entry("w1", Stage::Month, TODAY));
    let mut session = start(d, state(progress, vec![word("w1")]));
    // The queue holds w1's two questions: nothing to bring forward.
    assert_eq!(session.advance(3), 0);
}

// ---- stats ----

#[test]
fn stats_count_a_started_added_word_like_a_deck_term() {
    let d = deck();
    let mut late = entry("w1", Stage::Month, TODAY);
    late.lapses = 2;
    let session = start(
        d,
        state(
            vec![
                entry("kho", Stage::Week, TODAY + 1),
                late,
                entry("ghost", Stage::Day, TODAY),
            ],
            vec![word("w1")],
        ),
    );
    let stats = session.stats();
    assert_eq!(stats.per_stage, [0, 1, 1, 0, 0, 0]);
    assert_eq!(stats.due_today, 1);
    assert_eq!(stats.lapses_total, 2);
    assert_eq!(stats.orphans, 1, "only the ghost");
    assert_eq!(stats.new_left, 1, "sati 1.1");
}

// ---- start_word: through the session, started like add_new ----

#[test]
fn adding_a_word_outside_the_deck_keeps_and_starts_it_due_today() {
    let mut session = start(deck(), State::empty());
    assert_eq!(session.start_word(word("x")), Ok(()));
    let snapshot = session.snapshot();
    assert_eq!(snapshot.added, vec![word("x")]);
    let progress = snapshot.get(&id("x")).expect("started");
    assert_eq!(
        (progress.stage, progress.due, progress.lapses),
        (Stage::Day, day(TODAY), 0)
    );
    assert_eq!(session.remaining(), 2, "its two facets are queued");
    assert_eq!(session.term(&id("x")), Some(&word("x")));
}

#[test]
fn a_session_says_which_words_are_started() {
    let mut session = start(deck(), state(vec![entry("kho", Stage::Day, TODAY)], vec![]));
    assert!(session.is_started(&id("kho")));
    assert!(
        !session.is_started(&id("sati 1.1")),
        "a deck term not started"
    );
    assert!(!session.is_started(&id("x")));
    session.start_word(word("x")).unwrap();
    assert!(session.is_started(&id("x")));
    session.withdraw_word(&id("x"));
    assert!(!session.is_started(&id("x")));
}

#[test]
fn adding_a_deck_term_not_started_starts_it_and_adds_nothing() {
    let mut session = start(deck(), State::empty());
    assert_eq!(session.start_word(word("kho")), Ok(()));
    let snapshot = session.snapshot();
    assert_eq!(snapshot.added.len(), 0, "the deck teaches it");
    let progress = snapshot.get(&id("kho")).expect("started");
    assert_eq!((progress.stage, progress.due), (Stage::Day, day(TODAY)));
    assert_eq!(session.remaining(), 2, "the deck's term, its facets");
}

#[test]
fn a_started_deck_term_is_refused_and_the_state_is_unchanged() {
    let held = state(vec![entry("kho", Stage::Week, TODAY + 3)], vec![]);
    let mut session = start(deck(), held.clone());
    assert_eq!(
        session.start_word(word("kho")),
        Err(AddError::AlreadyStarted(id("kho")))
    );
    assert_eq!(session.snapshot().progress, held.progress);
    assert_eq!(session.remaining(), 0);
}

#[test]
fn a_word_started_twice_is_refused_and_kept_once() {
    let mut session = start(deck(), State::empty());
    session.start_word(word("x")).unwrap();
    let refused = session.start_word(indeclinable("x", u32::MAX));
    assert_eq!(refused, Err(AddError::AlreadyStarted(id("x"))));
    let snapshot = session.snapshot();
    assert_eq!(snapshot.added, vec![word("x")]);
    assert_eq!(snapshot.progress.len(), 1);
    assert_eq!(session.remaining(), 2);
}

#[test]
fn an_added_word_never_started_is_started_without_being_added_again() {
    let held = state(vec![], vec![word("x")]);
    let mut session = start(deck(), held);
    assert_eq!(session.start_word(word("x")), Ok(()));
    let snapshot = session.snapshot();
    assert_eq!(snapshot.added, vec![word("x")]);
    assert!(snapshot.get(&id("x")).is_some());
}

#[test]
fn a_word_taken_back_leaves_the_session_as_before() {
    let mut session = start(deck(), State::empty());
    session.start_word(word("x")).unwrap();
    session.start_word(word("y")).unwrap();
    session.start_word(word("kho")).unwrap();
    assert!(session.withdraw_word(&id("x")));
    let snapshot = session.snapshot();
    assert_eq!(snapshot.added, vec![word("y")]);
    assert_eq!(snapshot.get(&id("x")), None);
    assert_eq!(session.term(&id("x")), None);
    assert_eq!(session.remaining(), 4, "y and kho keep their facets");
    assert!(
        !session.withdraw_word(&id("x")),
        "nothing left to take back"
    );
    assert!(
        session.withdraw_word(&id("kho")),
        "a deck term is unstarted"
    );
    assert_eq!(session.snapshot().get(&id("kho")), None);
    assert_eq!(session.snapshot().added, vec![word("y")]);
    assert_eq!(
        session.start_word(word("x")),
        Ok(()),
        "it can be added again"
    );
}

#[test]
fn a_word_added_during_a_run_survives_the_next_answers_snapshot() {
    let mut session = start(deck(), state(vec![entry("kho", Stage::Day, TODAY)], vec![]));
    session.start_word(word("x")).unwrap();
    let question = session.next().cloned().unwrap();
    session
        .answer(&question.term, question.facet, Answer::Pass)
        .unwrap();
    let snapshot = session.snapshot();
    assert_eq!(snapshot.added, vec![word("x")]);
    assert!(snapshot.get(&id("x")).is_some(), "and stays started");
}

#[test]
fn a_word_started_in_a_run_is_asked_on_all_its_facets_then_leaves_the_new_terms() {
    let mut session = start(deck(), State::empty());
    session.start_word(word("x")).unwrap();
    assert_eq!(pass_all(&mut session), vec!["x", "x"]);
    assert_eq!(session.snapshot().new_terms(deck()).len(), 2, "sati, kho");
    assert_eq!(session.request_new(10), 2, "x is not started again");
}

#[test]
fn the_new_terms_queue_is_the_deck_then_the_added_words_in_the_order_added() {
    // Words added by an older build are in the state without progress.
    let held = state(vec![], vec![word("z"), word("a")]);
    let mut session = start(deck(), held);
    assert_eq!(session.request_new(10), 4);
    let order: Vec<String> = pass_all(&mut session);
    let mut firsts: Vec<&str> = Vec::new();
    for term in &order {
        if firsts.last() != Some(&term.as_str()) {
            firsts.push(term);
        }
    }
    assert_eq!(firsts, vec!["sati 1.1", "kho", "z", "a"]);
}

#[test]
fn the_state_starts_a_word_and_refuses_a_started_one() {
    let mut held = State::empty();
    held.start_word(deck(), word("x"), day(TODAY)).unwrap();
    assert_eq!(held.added.len(), 1);
    assert_eq!(held.progress.len(), 1);
    assert_eq!(
        held.start_word(deck(), word("x"), day(TODAY)),
        Err(AddError::AlreadyStarted(id("x")))
    );
    held.start_word(deck(), word("sati 1.1"), day(TODAY))
        .unwrap();
    assert_eq!(
        held.start_word(deck(), word("sati 1.1"), day(TODAY)),
        Err(AddError::AlreadyStarted(id("sati 1.1")))
    );
    assert_eq!((held.added.len(), held.progress.len()), (1, 2));
}
