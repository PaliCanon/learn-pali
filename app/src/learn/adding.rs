//! Adding a word to the practice outside a run, from the home or anywhere a
//! passage is read: [`add_word`] puts it in the stored progress at once, and
//! starts it (due today, as 'Add new terms' starts a deck term). In a run
//! the word goes through the session instead
//! ([`ReviewRun::start_word`](crate::learn::run::ReviewRun::start_word)), so
//! the next answer's snapshot keeps it.

use learn_core::added::AddError;
use learn_core::date::Date;
use learn_core::deck::{Deck, Term, TermId};
use learn_core::progress::State;
use leptos::prelude::*;

use crate::backend::Backend;
use crate::learn::run::{NotAdded, ReviewRun};
use crate::store::Load;
use crate::today::{browser_today, session_today};

/// Why a word was not added; nothing is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The stored progress is broken, or the store did not keep the word
    /// for no reason it tells: nothing is written over it.
    Unusable,
    /// The stored progress was saved by a newer version of the app: it is
    /// read-only here.
    Newer,
    /// The store cannot be read or written now.
    Unreadable,
    /// The word is already started.
    Add(AddError),
}

impl Refused {
    /// Why the store, in the state `load`, does not take a word.
    pub(crate) fn of(load: &Load) -> Refused {
        match load {
            Load::NewerVersion { .. } => Refused::Newer,
            Load::Unreadable(_) => Refused::Unreadable,
            Load::Invalid { .. } | Load::Ok(_) | Load::Empty => Refused::Unusable,
        }
    }
}

/// The state of `load` with `term` added and started on `today`, to be
/// saved.
fn with_word(load: Load, deck: &Deck, term: Term, today: Date) -> Result<State, Refused> {
    let mut state = match load {
        Load::Ok(state) => state,
        Load::Empty => State::empty(),
        Load::Invalid { .. } | Load::NewerVersion { .. } | Load::Unreadable(_) => {
            return Err(Refused::of(&load));
        }
    };
    state.start_word(deck, term, today).map_err(Refused::Add)?;
    Ok(state)
}

/// Whether the store took `id` as a started word: its memory holds its
/// progress. A write it will retry ([`SaveError::Transient`]) counts as
/// accepted (its memory has the word and it writes it again); one it
/// refuses (a newer file, an unreadable store) leaves its memory without
/// it, and the cause is what the store answers.
///
/// [`SaveError::Transient`]: crate::backend::SaveError::Transient
fn kept(backend: Backend, id: &TermId) -> Result<(), Refused> {
    match backend.load() {
        Load::Ok(state) if state.get(id).is_some() => Ok(()),
        other => Err(Refused::of(&other)),
    }
}

/// The day a word added outside a run starts on: the one a review starts
/// with ([`session_today`]): the browser's date, never before the state's
/// last day. A review pins it for its whole run
/// ([`Session::today`](learn_core::session::Session::today)), so a word
/// starts on the same day in a run and outside it.
fn start_day(load: &Load) -> Date {
    match load {
        Load::Ok(state) => session_today(browser_today(), state),
        _ => browser_today(),
    }
}

/// Adds `term` to the stored progress (outside a run), starts it and saves
/// it at once, so the home's due count includes it.
///
/// The load, the add and the save run in this one call, so a sync answer
/// cannot come in between: the cycle that merges next reads the store's
/// memory, this word included.
///
/// # Errors
/// [`Refused`], with nothing kept: the stored progress cannot take it, or
/// the store did not take the save, and says why.
pub(crate) fn add_word(backend: Backend, term: Term) -> Result<(), Refused> {
    let id = term.id.clone();
    let load = backend.load();
    let today = start_day(&load);
    let state = with_word(load, backend.deck(), term, today)?;
    backend.save(&state);
    kept(backend, &id)
}

/// The run when it is still there: a review that was rebuilt (a sync, a
/// reload) disposed the signal a word panel captured when it opened, and
/// then the stored progress is what counts.
pub(crate) fn live(run: Option<RwSignal<ReviewRun>>) -> Option<RwSignal<ReviewRun>> {
    run.filter(|run| run.try_with_untracked(|_| ()).is_some())
}

/// Adds `term` where the word was pressed: through the run when a review
/// is going (the next answer's snapshot keeps it), else, and when the run
/// was disposed (a rebuilt review, also between one step and the next), to
/// the stored progress ([`add_word`]).
///
/// The run is updated without notifying: a notification would rebuild the
/// card, closing the word panel the learner is reading; the word is queued
/// after the cards already waiting and the header's count follows the next
/// answer.
///
/// # Errors
/// [`Refused`], with nothing kept.
pub(crate) fn add_through(
    run: Option<RwSignal<ReviewRun>>,
    backend: Backend,
    term: Term,
) -> Result<(), Refused> {
    let id = term.id.clone();
    // One step: a run disposed at any moment answers `None` and the stored
    // progress takes the word, never a refusal.
    let through = run.and_then(|run| {
        let cause = std::cell::RefCell::new(None);
        run.try_update_untracked(|run| {
            run.start_word(term.clone(), |state| {
                backend.save(state);
                let kept = kept(backend, &id);
                let took = kept.is_ok();
                *cause.borrow_mut() = kept.err();
                took
            })
        })
        .map(|result| {
            result.map_err(|refused| match refused {
                NotAdded::Already(error) => Refused::Add(error),
                NotAdded::NotSaved => cause.take().unwrap_or(Refused::Unusable),
            })
        })
    });
    through.unwrap_or_else(|| add_word(backend, term))
}

#[cfg(test)]
mod tests {
    use learn_core::added::AddError;
    use learn_core::deck::TermId;
    use learn_core::progress::State;
    use leptos::prelude::Owner;

    use super::{Refused, add_word};
    use crate::deck::deck;
    use crate::learn::home_state::new_left;
    use crate::store::Load;
    use crate::test_support::{
        FakeFiles, FakeStore, Outcome, added_word, backend_of, progress_fixture,
    };
    use crate::today::{browser_today, session_today};

    fn run_on(load: Load, test: impl FnOnce(&FakeStore, crate::backend::Backend)) {
        Owner::new().with(|| {
            let store = FakeStore::new(load);
            let backend = backend_of(&store, &FakeFiles::default());
            test(&store, backend);
        });
    }

    /// The day an add outside a run starts the word on.
    fn day(state: &State) -> learn_core::date::Date {
        session_today(browser_today(), state)
    }

    #[test]
    fn a_word_added_on_an_empty_progress_is_saved_at_once_and_started_today() {
        run_on(Load::Empty, |store, backend| {
            assert_eq!(add_word(backend, added_word()), Ok(()));
            let saved = store.saved();
            assert_eq!(saved.len(), 1);
            assert_eq!(saved[0].added, vec![added_word()]);
            let progress = saved[0].get(&added_word().id).expect("started");
            assert_eq!(progress.due, day(&saved[0]));
            assert_eq!(new_left(&backend.load(), deck()), deck().terms.len());
        });
    }

    #[test]
    fn a_word_added_keeps_the_progress_and_starts_beside_it() {
        let held = progress_fixture();
        let (started, new_before) = (
            held.progress.clone(),
            new_left(&Load::Ok(held.clone()), deck()),
        );
        run_on(Load::Ok(held), |store, backend| {
            add_word(backend, added_word()).unwrap();
            let saved = store.saved();
            assert_eq!(saved[0].progress[..started.len()], started[..]);
            assert_eq!(saved[0].progress.len(), started.len() + 1);
            assert_eq!(saved[0].added, vec![added_word()]);
            assert_eq!(new_left(&backend.load(), deck()), new_before);
        });
    }

    #[test]
    fn a_deck_term_not_started_is_started_and_not_added() {
        run_on(Load::Ok(State::empty()), |store, backend| {
            let mut deck_term = added_word();
            deck_term.id = TermId("kho".into());
            assert_eq!(add_word(backend, deck_term), Ok(()));
            let saved = store.saved();
            assert_eq!(saved[0].added.len(), 0);
            assert!(saved[0].get(&TermId("kho".into())).is_some());
        });
    }

    #[test]
    fn a_word_already_started_is_refused_and_nothing_is_saved() {
        run_on(Load::Ok(State::empty()), |store, backend| {
            let mut deck_term = added_word();
            deck_term.id = TermId("kho".into());
            add_word(backend, deck_term.clone()).unwrap();
            assert_eq!(
                add_word(backend, deck_term),
                Err(Refused::Add(AddError::AlreadyStarted(TermId("kho".into()))))
            );
            add_word(backend, added_word()).unwrap();
            assert_eq!(
                add_word(backend, added_word()),
                Err(Refused::Add(AddError::AlreadyStarted(added_word().id)))
            );
            assert_eq!(store.saved().len(), 2);
        });
    }

    #[test]
    fn a_broken_or_unreadable_store_takes_nothing_and_says_why() {
        for (load, cause) in [
            (Load::Invalid { raw: "{".into() }, Refused::Unusable),
            (Load::NewerVersion { raw: "{}".into() }, Refused::Newer),
            (Load::Unreadable("denied".into()), Refused::Unreadable),
        ] {
            run_on(load, |store, backend| {
                assert_eq!(add_word(backend, added_word()), Err(cause.clone()));
                assert_eq!(store.saved().len(), 0, "{cause:?}");
            });
        }
    }

    #[test]
    fn a_save_the_store_refuses_is_not_an_add_and_says_why() {
        for (refusal, cause) in [
            (Outcome::Newer, Refused::Newer),
            (Outcome::Unreadable, Refused::Unreadable),
        ] {
            run_on(Load::Empty, |store, backend| {
                store.script(&[refusal]);
                assert_eq!(add_word(backend, added_word()), Err(cause), "{refusal:?}");
                assert_eq!(new_left(&backend.load(), deck()), 0, "{refusal:?}");
            });
        }
    }

    #[test]
    fn a_save_the_store_refuses_in_a_run_says_why_and_the_run_is_as_it_was() {
        use learn_core::date::Date;
        use learn_core::session::Session;
        use leptos::prelude::{RwSignal, With};

        use super::add_through;
        use crate::learn::run::ReviewRun;

        for (refusal, cause) in [
            (Outcome::Newer, Refused::Newer),
            (Outcome::Unreadable, Refused::Unreadable),
        ] {
            run_on(Load::Ok(progress_fixture()), |store, backend| {
                let state = progress_fixture();
                let today = state
                    .last_today
                    .unwrap_or(Date::parse("2026-10-05").unwrap());
                let session = Session::start(deck(), state.clone(), today, 0).unwrap();
                let run = RwSignal::new(ReviewRun::new(session, deck(), state));
                store.script(&[refusal]);
                assert_eq!(
                    add_through(Some(run), backend, added_word()),
                    Err(cause),
                    "{refusal:?}"
                );
                assert!(
                    !run.with(|run| run.started(&added_word().id)),
                    "{refusal:?}"
                );
            });
        }
    }

    #[test]
    fn a_run_disposed_is_no_refusal_the_stored_progress_takes_the_word() {
        use learn_core::date::Date;
        use learn_core::session::Session;
        use leptos::prelude::RwSignal;

        use super::add_through;
        use crate::learn::run::ReviewRun;

        run_on(Load::Ok(progress_fixture()), |store, backend| {
            let state = progress_fixture();
            let today = state
                .last_today
                .unwrap_or(Date::parse("2026-10-05").unwrap());
            let owner = Owner::new();
            let run = owner.with(|| {
                let session = Session::start(deck(), state.clone(), today, 0).unwrap();
                RwSignal::new(ReviewRun::new(session, deck(), state))
            });
            owner.cleanup();
            assert_eq!(add_through(Some(run), backend, added_word()), Ok(()));
            assert_eq!(store.saved()[0].added, vec![added_word()]);
            assert!(store.saved()[0].get(&added_word().id).is_some());
        });
    }

    #[test]
    fn a_word_starts_on_the_same_day_in_a_run_and_outside_it() {
        use learn_core::date::Date;
        use learn_core::session::Session;
        use leptos::prelude::RwSignal;

        use super::add_through;
        use crate::learn::run::ReviewRun;

        // The last day is far ahead of the browser's: the later wins (Q2).
        let mut ahead = progress_fixture();
        ahead.last_today = Some(Date::parse("2099-01-01").unwrap());
        let due_of = |store: &FakeStore| store.saved()[0].get(&added_word().id).unwrap().due;
        run_on(Load::Ok(ahead.clone()), |store, backend| {
            add_word(backend, added_word()).unwrap();
            assert_eq!(due_of(store), ahead.last_today.unwrap(), "outside a run");
        });
        run_on(Load::Ok(ahead.clone()), |store, backend| {
            let today = session_today(browser_today(), &ahead);
            let session = Session::start(deck(), ahead.clone(), today, 0).unwrap();
            let run = RwSignal::new(ReviewRun::new(session, deck(), ahead.clone()));
            add_through(Some(run), backend, added_word()).unwrap();
            assert_eq!(due_of(store), ahead.last_today.unwrap(), "in a run");
        });
    }
    #[test]
    fn going_home_after_an_add_in_a_run_keeps_the_word_without_an_answer() {
        use learn_core::date::Date;
        use learn_core::session::Session;
        use leptos::prelude::RwSignal;

        use super::add_through;
        use crate::learn::run::ReviewRun;

        run_on(Load::Ok(progress_fixture()), |store, backend| {
            let state = progress_fixture();
            let today = state
                .last_today
                .unwrap_or(Date::parse("2026-10-05").unwrap());
            let session = Session::start(deck(), state.clone(), today, 0).unwrap();
            let run = RwSignal::new(ReviewRun::new(session, deck(), state));
            add_through(Some(run), backend, added_word()).unwrap();
            // The learner leaves without grading: the home reads the store.
            let home = backend.load();
            let Load::Ok(kept) = &home else {
                panic!("the store holds a progress, got {home:?}");
            };
            assert_eq!(kept.added.len(), 1, "the word is kept");
            assert!(kept.get(&added_word().id).is_some(), "and started");
            assert_eq!(store.saved().len(), 1, "saved at once, nothing since");
            assert_eq!(
                new_left(&home, deck()),
                new_left(&Load::Ok(progress_fixture()), deck()),
                "and not among the new terms"
            );
        });
    }

    #[test]
    fn a_write_that_will_be_retried_is_accepted() {
        run_on(Load::Empty, |store, backend| {
            store.script(&[Outcome::Transient]);
            assert_eq!(add_word(backend, added_word()), Ok(()));
            assert_eq!(store.saved()[0].added, vec![added_word()]);
        });
    }
}
