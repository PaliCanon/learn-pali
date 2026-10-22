//! 'Add to practice', under each entry of a passage's word panel: the slot
//! `entry_action` of ui-lib's `PaliText` (ui-lib 0.3.1).
//!
//! The panel calls [`entry_action`]'s callback once per shown entry when it
//! opens, untracked: it decides there whether the headword is already
//! started (a deck term or an added word with progress: then it shows
//! nothing) and otherwise returns a view that reads its own signal, so the
//! pressed button turns into a muted 'Added' line (or the reason of a
//! refusal) while the panel stays open. Pressing starts the word, due today,
//! as 'Add new terms' starts a deck term: a word outside the deck is added
//! and started, a deck term not started is only started.
//!
//! In a review the word goes through the run (provided as context by
//! [`ReviewView`](crate::components::session::ReviewView)), so the next
//! answer's snapshot keeps it; with no run, or when the run was disposed
//! since the panel captured it (a rebuilt review), the stored progress
//! decides and takes it
//! ([`add_word`](crate::learn::adding::add_word)), and the home is told to
//! read the store again. The line says 'Added' only once the store took the
//! save (a write the store will retry counts as taken: its memory has the
//! word); a refusal is said with its cause beside the button, to try again,
//! and the word is not counted. The press reads the store again before
//! adding ([`standing`]): a word started meanwhile is 'Added', never started
//! twice. With no
//! store handle, or a stored progress that is broken, unreadable or saved by
//! a newer version (in a run too), there is no button.

use learn_core::added::AddError;
use learn_core::deck::{Deck, Example, Term, TermId};
use learn_core::words::{WordEntry as DeckEntry, word_key};
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, EntryAction, Size, Variant, WordEntry};

use crate::backend::Backend;
use crate::catalogue::Locale;
use crate::components::data_block::ProgressData;
use crate::learn::adding::{Refused, add_through, live};
use crate::learn::run::ReviewRun;
use crate::store::Load;

/// The action of a word panel over `passage`, in `locale`; `None` without a
/// store handle (the fragment's provider), so the panel looks as before.
pub(crate) fn entry_action(passage: Example, locale: Locale) -> Option<EntryAction> {
    let backend = use_context::<Backend>()?;
    let run = use_context::<RwSignal<ReviewRun>>();
    let moved = use_context::<ProgressData>();
    Some(Callback::new(move |(form, entry): (String, WordEntry)| {
        let Some(found) = lookup(backend.deck(), &form, &entry.headword) else {
            return ().into_any();
        };
        if !offered(run, backend, &TermId(found.headword.clone())) {
            return ().into_any();
        }
        add_slot(run, backend, moved, found, passage.clone(), form, locale)
    }))
}

/// The dictionary entry to build the word from: among the entries the
/// selected deck's dictionary (a deck with none offers no button) holds for the pressed `form`'s key, the one whose
/// headword is `headword`. A headword is offered only from the pressed
/// form's own entries: a headword the panel shows that the dictionary does
/// not list under that form (no entry, or another headword) gets no button,
/// because there is nothing to build the word from.
fn lookup(deck: &'static Deck, form: &str, headword: &str) -> Option<&'static DeckEntry> {
    deck.words
        .get(&word_key(form))?
        .iter()
        .find(|entry| entry.headword == headword)
}

/// Where `id` stands for the practice, read from the store and the run.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Standing {
    /// The stored progress can take the word, which is not started.
    Open,
    /// The word is started (in the run, or in the store).
    Started,
    /// The stored progress cannot take a word, and why.
    Unusable(Refused),
}

/// Reads where `id` stands now: a broken, unreadable or newer stored
/// progress takes nothing (in a run too); else the word is started when the
/// run says so or the stored progress does. Read when the panel opens and
/// again on the press, so a word started elsewhere in between (another tab,
/// a sync) is not started twice.
fn standing(run: Option<RwSignal<ReviewRun>>, backend: Backend, id: &TermId) -> Standing {
    let stored = backend.load();
    if matches!(
        stored,
        Load::Invalid { .. } | Load::NewerVersion { .. } | Load::Unreadable(_)
    ) {
        return Standing::Unusable(Refused::of(&stored));
    }
    let in_run = live(run).and_then(|run| run.try_with_untracked(|run| run.started(id)));
    let in_store = matches!(&stored, Load::Ok(state) if state.get(id).is_some());
    if in_run == Some(true) || in_store {
        Standing::Started
    } else {
        Standing::Open
    }
}

/// Whether the button is offered for `id`: the stored progress can take a
/// word and `id` is not started yet ([`standing`]).
fn offered(run: Option<RwSignal<ReviewRun>>, backend: Backend, id: &TermId) -> bool {
    standing(run, backend, id) == Standing::Open
}

/// Builds the term of the word pressed (`entry`, in `passage`, pressed as
/// `form`) and adds it ([`add_through`]). Outside a run, once the store has
/// it, `moved` says the stored progress changed: the home reads it again.
fn add_pressed(
    run: Option<RwSignal<ReviewRun>>,
    backend: Backend,
    moved: Option<ProgressData>,
    entry: &DeckEntry,
    passage: &Example,
    form: &str,
) -> Result<(), Refused> {
    // The panel was opened a while ago: ask again before touching the store.
    match standing(run, backend, &TermId(entry.headword.clone())) {
        Standing::Started => return Ok(()),
        Standing::Unusable(why) => return Err(why),
        Standing::Open => {}
    }
    let term: Term = learn_core::added::added_term(entry, passage, form);
    add_through(run, backend, term)?;
    if live(run).is_none()
        && let Some(data) = moved
    {
        data.moved();
    }
    Ok(())
}

/// What the slot shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    /// The button.
    Ready,
    /// The word was added.
    Added,
    /// The word was refused.
    Refused(Refused),
}

/// The line that says why a word was not added.
fn reason(locale: Locale, refused: &Refused) -> &'static str {
    match refused {
        Refused::Unusable => td_string!(locale, add_refused_unusable),
        Refused::Newer => td_string!(locale, add_refused_newer),
        Refused::Unreadable => td_string!(locale, add_refused_unreadable),
        Refused::Add(_) => td_string!(locale, add_refused_there),
    }
}

/// A muted line under the entry, announced to screen readers.
fn status_line(text: &'static str) -> impl IntoView {
    view! {
        <p role="status" class="text-sm text-muted">
            {text}
        </p>
    }
}

/// What the slot shows after a press gave `result`. A word that is already
/// started (a second press, a double click, the same word added from another
/// entry) is in the practice: it ends on 'Added' too.
fn settled(result: Result<(), Refused>) -> Slot {
    match result {
        Ok(()) | Err(Refused::Add(AddError::AlreadyStarted(_))) => Slot::Added,
        Err(refused) => Slot::Refused(refused),
    }
}

/// A press on the slot: runs `add` unless the word is already added (a
/// second press before the line replaced the button does nothing), and
/// settles the slot on its result. A refused slot keeps its button, so the
/// learner can try again.
fn press_slot(slot: RwSignal<Slot>, add: impl FnOnce() -> Result<(), Refused>) {
    if slot.get_untracked() == Slot::Added {
        return;
    }
    slot.set(settled(add()));
}

/// The slot of one entry: the button, then the 'Added' line (or the reason
/// of a refusal) in its place.
fn add_slot(
    run: Option<RwSignal<ReviewRun>>,
    backend: Backend,
    moved: Option<ProgressData>,
    entry: &'static DeckEntry,
    passage: Example,
    form: String,
    locale: Locale,
) -> AnyView {
    let slot = RwSignal::new(Slot::Ready);
    let press = Callback::new(move |_| {
        press_slot(slot, || {
            add_pressed(run, backend, moved, entry, &passage, &form)
        });
    });
    (move || shown(&slot.get(), press, locale)).into_any()
}

/// The 'Add to practice' button.
fn button(press: Callback<leptos::ev::MouseEvent>, locale: Locale) -> impl IntoView {
    view! {
        <Button variant=Variant::Ghost size=Size::Sm on_click=press>
            {td_string!(locale, add_word)}
        </Button>
    }
}

/// What `slot` shows: the button, the 'Added' line, or the reason.
fn shown(slot: &Slot, press: Callback<leptos::ev::MouseEvent>, locale: Locale) -> AnyView {
    match slot {
        Slot::Ready => button(press, locale).into_any(),
        Slot::Added => status_line(td_string!(locale, word_added)).into_any(),
        Slot::Refused(refused) => view! {
            {status_line(reason(locale, refused))}
            {button(press, locale)}
        }
        .into_any(),
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use learn_core::added::AddError;
    use learn_core::date::Date;
    use learn_core::deck::{Example, TermId};
    use learn_core::progress::{Progress, State};
    use learn_core::session::Session;
    use learn_core::stage::Stage;
    use leptos::prelude::*;
    use palicanon_ui_lib::{Language, WordEntry};

    use super::{Slot, add_pressed, entry_action, lookup, settled, shown};
    use crate::backend::Backend;
    use crate::catalogue::Locale;
    use crate::components::data_block::ProgressData;
    use crate::deck::deck;
    use crate::learn::adding::Refused;
    use crate::learn::run::ReviewRun;
    use crate::store::Load;
    use crate::test_support::{Outcome, added_started, added_word, provide_fakes};

    fn today() -> Date {
        Date::parse("2026-10-05").unwrap()
    }

    /// A passage whose second word is `sānuvajjo`, the form of an added-able
    /// headword the deck does not teach.
    fn passage() -> Example {
        Example {
            text: "natthi sānuvajjo loke".into(),
            source: "AN 1".into(),
            sutta: "Test".into(),
            blank: None,
            translations: std::collections::BTreeMap::new(),
        }
    }

    fn run_with(state: State) -> ReviewRun {
        let session = Session::start(deck(), state.clone(), today(), 0).unwrap();
        ReviewRun::new(session, deck(), state)
    }

    fn started(id: &str) -> State {
        let mut state = State::empty();
        state
            .progress
            .push(Progress::new(TermId(id.into()), today()));
        state
    }

    /// The entry the panel passes: only the headword matters to the slot.
    fn entry(headword: &str) -> WordEntry {
        WordEntry {
            headword: headword.into(),
            tag: None,
            meaning: String::new(),
        }
    }

    /// The slot's markup for the pressed `form` and the entry `headword`,
    /// in a run over `run` (or none) over a store holding `load`.
    fn slot(run: Option<State>, load: Load, language: Language, form: &str, head: &str) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            provide_fakes(load);
            if let Some(state) = run {
                provide_context(RwSignal::new(run_with(state)));
            }
            let action = entry_action(passage(), Locale::from(language)).expect("an action");
            action.run((form.into(), entry(head))).to_html()
        })
    }

    fn en(run: Option<State>, form: &str, head: &str) -> String {
        slot(run, Load::Empty, Language::En, form, head)
    }

    #[test]
    fn a_headword_the_practice_lacks_gets_the_button_in_every_language() {
        for (language, label) in [
            (Language::En, "Add to practice"),
            (Language::Fr, "Ajouter à la pratique"),
            (Language::Vi, "Thêm vào luyện tập"),
        ] {
            let html = slot(
                Some(State::empty()),
                Load::Empty,
                language,
                "sānuvajjo",
                "sānuvajja",
            );
            assert!(html.contains("<button"), "{language:?}: {html}");
            assert!(html.contains(&format!(">{label}</button>")), "{html}");
        }
    }

    #[test]
    fn the_button_is_a_ghost_of_the_small_size_with_no_class_of_its_own() {
        let html = en(Some(State::empty()), "sānuvajjo", "sānuvajja");
        let classes = palicanon_ui_lib::button::classes(
            palicanon_ui_lib::Variant::Ghost,
            palicanon_ui_lib::Size::Sm,
            "",
        );
        assert!(html.contains(&format!(r#"class="{classes}""#)), "{html}");
    }

    #[test]
    fn a_deck_term_gets_the_button_until_it_is_started() {
        // kho is started; rūpa 1 is a deck term nobody started.
        let held = started("kho");
        let there = en(Some(held.clone()), "kho", "kho");
        assert!(!there.contains("<button"), "{there}");
        let waiting = en(Some(held), "rūpaṃ", "rūpa 1");
        assert!(waiting.contains(">Add to practice</button>"), "{waiting}");
    }

    #[test]
    fn an_added_word_gets_the_button_until_it_is_started() {
        let started = added_started(Stage::Day, today());
        let there = en(Some(started), "sānuvajjo", "sānuvajja");
        assert!(!there.contains("<button"), "{there}");
        // A word an older build left added and unstarted can be started.
        let mut waiting = State::empty();
        waiting.added.push(added_word());
        let html = en(Some(waiting), "sānuvajjo", "sānuvajja");
        assert!(html.contains(">Add to practice</button>"), "{html}");
    }

    #[test]
    fn no_run_reads_the_stored_progress_and_a_broken_one_gets_no_button() {
        let ok = slot(None, Load::Empty, Language::En, "sānuvajjo", "sānuvajja");
        assert!(ok.contains(">Add to practice</button>"), "{ok}");
        let held = added_started(Stage::Day, today());
        let there = slot(None, Load::Ok(held), Language::En, "sānuvajjo", "sānuvajja");
        assert!(!there.contains("<button"), "{there}");
        for load in [
            Load::Invalid { raw: "{".into() },
            Load::NewerVersion { raw: "{}".into() },
            Load::Unreadable("denied".into()),
        ] {
            let html = slot(None, load, Language::En, "sānuvajjo", "sānuvajja");
            assert!(!html.contains("<button"), "{html}");
        }
    }

    #[test]
    fn a_form_the_deck_has_no_entry_for_gets_no_button() {
        let html = en(Some(State::empty()), "zzzq", "zzzq");
        assert!(!html.contains("<button"), "{html}");
    }

    #[test]
    fn the_added_line_is_said_in_every_language() {
        for (language, added) in [
            (Language::En, "Added"),
            (Language::Fr, "Ajouté"),
            (Language::Vi, "Đã thêm"),
        ] {
            let locale = Locale::from(language);
            let html = Owner::new().with(|| {
                let press = Callback::new(|_| ());
                shown(&Slot::Added, press, locale).to_html()
            });
            assert!(html.contains(&format!(">{added}</p>")), "{html}");
            assert!(html.contains(r#"role="status""#), "{html}");
            assert!(!html.contains("<button"), "{html}");
        }
    }

    #[test]
    fn the_refusals_are_said_in_every_language() {
        let there = Refused::Add(AddError::AlreadyStarted(TermId("x".into())));
        for (language, [unusable, newer, unreadable, already]) in [
            (
                Language::En,
                [
                    "Your progress cannot take it now.",
                    "Your progress was saved by a newer version. Update to add words.",
                    "Your progress cannot be read or saved now.",
                    "Already in your practice.",
                ],
            ),
            (
                Language::Fr,
                [
                    "Votre progression ne peut pas l’accepter pour l’instant.",
                    "Votre progression a été enregistrée par une version plus récente. Mettez à jour pour ajouter des mots.",
                    "Votre progression ne peut pas être lue ni enregistrée pour l’instant.",
                    "Déjà dans votre pratique.",
                ],
            ),
            (
                Language::Vi,
                [
                    "Tiến trình của bạn chưa thể nhận từ này.",
                    "Tiến trình của bạn được lưu bởi một phiên bản mới hơn. Hãy cập nhật để thêm từ.",
                    "Tiến trình của bạn chưa thể đọc hoặc lưu lúc này.",
                    "Đã có trong phần luyện tập của bạn.",
                ],
            ),
        ] {
            let locale = Locale::from(language);
            assert_eq!(super::reason(locale, &Refused::Unusable), unusable);
            assert_eq!(super::reason(locale, &Refused::Newer), newer);
            assert_eq!(super::reason(locale, &Refused::Unreadable), unreadable);
            assert_eq!(super::reason(locale, &there), already);
        }
    }

    #[test]
    fn a_refusal_keeps_the_button_to_try_again_beside_its_reason() {
        for refused in [Refused::Unusable, Refused::Newer, Refused::Unreadable] {
            let html = Owner::new().with(|| {
                let press = Callback::new(|_| ());
                shown(&Slot::Refused(refused.clone()), press, Locale::en).to_html()
            });
            assert!(html.contains(r#"role="status""#), "{refused:?}: {html}");
            assert!(
                html.contains(">Add to practice</button>"),
                "{refused:?}: {html}"
            );
        }
    }

    #[test]
    fn a_refused_press_can_be_pressed_again_and_ends_on_added() {
        use std::cell::Cell;

        use super::press_slot;

        Owner::new().with(|| {
            let slot = RwSignal::new(Slot::Ready);
            let calls = Cell::new(0);
            let attempt = |result: Result<(), Refused>| {
                press_slot(slot, || {
                    calls.set(calls.get() + 1);
                    result
                });
            };
            attempt(Err(Refused::Unreadable));
            assert_eq!(slot.get_untracked(), Slot::Refused(Refused::Unreadable));
            attempt(Ok(()));
            assert_eq!(slot.get_untracked(), Slot::Added, "the retry took");
            attempt(Ok(()));
            assert_eq!(calls.get(), 2, "once Added, a press does nothing");
        });
    }

    #[test]
    fn pressing_adds_the_word_built_from_the_entry_and_the_passage_through_the_run() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            let backend = Backend::current();
            let run = RwSignal::new(run_with(started("kho")));
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").expect("the deck's entry");
            let press = || add_pressed(Some(run), backend, None, entry, &passage(), "sānuvajjo");
            assert_eq!(press(), Ok(()));
            let saved = store.saved();
            assert_eq!(saved.len(), 1, "saved at once");
            let word = &saved[0].added[0];
            assert_eq!(word.id, TermId("sānuvajja".into()));
            assert_eq!(word.meaning.en, "blameworthy (to)");
            let blank = word.example.as_ref().unwrap().blank.as_ref().unwrap();
            assert_eq!(
                (
                    blank.before.as_str(),
                    blank.answer.as_str(),
                    blank.after.as_str()
                ),
                ("natthi ", "sānuvajjo", " loke")
            );
            assert_eq!(saved[0].progress.len(), 2, "kho, and the word started");
            run.with(|run| assert!(run.started(&word.id)));
            // A second press finds the word started: it is in the practice
            // ('Added'), and nothing is written again.
            assert_eq!(settled(press()), Slot::Added);
            assert_eq!(store.saved().len(), 1, "a second press saves nothing");
        });
    }

    #[test]
    fn the_next_answer_keeps_the_word_added() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            let backend = Backend::current();
            let run = RwSignal::new(run_with(started("kho")));
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            add_pressed(Some(run), backend, None, entry, &passage(), "sānuvajjo").unwrap();
            run.update(|run| {
                run.grade(learn_core::session::Answer::Pass, |state| {
                    backend.save(state);
                });
            });
            let last = store.saved().pop().unwrap();
            assert_eq!(last.added.len(), 1, "the answer's snapshot holds the word");
        });
    }

    #[test]
    fn without_a_run_the_word_goes_straight_to_the_store() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            let backend = Backend::current();
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            assert_eq!(
                add_pressed(None, backend, None, entry, &passage(), "sānuvajjo"),
                Ok(())
            );
            assert_eq!(store.saved()[0].added.len(), 1);
            assert_eq!(store.saved()[0].progress.len(), 1, "and started");
        });
    }

    #[test]
    fn a_form_not_in_the_passage_still_adds_the_word_without_a_blank() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            let elsewhere = Example {
                text: "natthi loke".into(),
                ..passage()
            };
            add_pressed(
                None,
                Backend::current(),
                None,
                entry,
                &elsewhere,
                "sānuvajjo",
            )
            .unwrap();
            let word = &store.saved()[0].added[0];
            assert_eq!(word.example.as_ref().unwrap().blank, None);
        });
    }

    #[test]
    fn a_broken_unreadable_or_newer_store_gets_no_button_inside_a_run_too() {
        for load in [
            Load::Invalid { raw: "{".into() },
            Load::NewerVersion { raw: "{}".into() },
            Load::Unreadable("denied".into()),
        ] {
            let html = slot(
                Some(State::empty()),
                load,
                Language::En,
                "sānuvajjo",
                "sānuvajja",
            );
            assert!(!html.contains("<button"), "{html}");
        }
        let usable = slot(
            Some(State::empty()),
            Load::Ok(State::empty()),
            Language::En,
            "sānuvajjo",
            "sānuvajja",
        );
        assert!(usable.contains(">Add to practice</button>"), "{usable}");
    }

    #[test]
    fn a_refused_save_says_so_and_the_word_is_not_counted_in_the_run() {
        for refusal in [Outcome::Newer, Outcome::Unreadable] {
            Owner::new().with(|| {
                let (store, _) = provide_fakes(Load::Empty);
                store.script(&[refusal]);
                let backend = Backend::current();
                let run = RwSignal::new(run_with(started("kho")));
                let before = run.with(ReviewRun::new_left);
                let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
                let result = add_pressed(Some(run), backend, None, entry, &passage(), "sānuvajjo");
                let cause = match refusal {
                    Outcome::Newer => Refused::Newer,
                    _ => Refused::Unreadable,
                };
                assert_eq!(result, Err(cause.clone()), "{refusal:?}");
                assert_eq!(settled(result), Slot::Refused(cause));
                run.with(|run| {
                    assert!(!run.started(&TermId("sānuvajja".into())), "{refusal:?}");
                    assert_eq!(run.new_left(), before, "{refusal:?}: not counted");
                });
            });
        }
    }

    #[test]
    fn a_refused_save_outside_a_run_says_so_and_counts_nothing() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            store.script(&[Outcome::Unreadable]);
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            let result = add_pressed(
                None,
                Backend::current(),
                None,
                entry,
                &passage(),
                "sānuvajjo",
            );
            assert_eq!(result, Err(Refused::Unreadable));
            assert_eq!(
                crate::learn::home_state::new_left(&store_load(&store), deck()),
                0
            );
        });
    }

    fn store_load(store: &crate::test_support::FakeStore) -> Load {
        crate::backend::ProgressStore::load(store).map(|book| book.words().clone())
    }

    #[test]
    fn a_second_press_on_the_same_entry_ends_on_added() {
        let there = Refused::Add(AddError::AlreadyStarted(TermId("x".into())));
        assert_eq!(settled(Ok(())), Slot::Added);
        assert_eq!(settled(Err(there)), Slot::Added);
        assert_eq!(
            settled(Err(Refused::Unusable)),
            Slot::Refused(Refused::Unusable)
        );
    }

    #[test]
    fn an_add_outside_a_run_tells_the_home_to_read_the_store_again() {
        Owner::new().with(|| {
            provide_fakes(Load::Empty);
            let data = ProgressData::current();
            let writes = data.writes().0;
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            let press = || {
                add_pressed(
                    None,
                    Backend::current(),
                    Some(data),
                    entry,
                    &passage(),
                    "sānuvajjo",
                )
            };
            assert_eq!(writes.get_untracked(), 0);
            assert_eq!(press(), Ok(()));
            assert_eq!(writes.get_untracked(), 1, "the home reads the store again");
            assert_eq!(press(), Ok(()), "already started: in the practice");
            assert_eq!(writes.get_untracked(), 1, "and moves nothing");
        });
    }

    #[test]
    fn an_add_in_a_run_does_not_move_the_stores_writes() {
        Owner::new().with(|| {
            provide_fakes(Load::Empty);
            let data = ProgressData::current();
            let run = RwSignal::new(run_with(started("kho")));
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            let result = add_pressed(
                Some(run),
                Backend::current(),
                Some(data),
                entry,
                &passage(),
                "sānuvajjo",
            );
            assert_eq!(result, Ok(()));
            assert_eq!(
                data.writes().0.get_untracked(),
                0,
                "the home is not mounted"
            );
        });
    }

    /// A run that was built and then disposed, as a rebuilt review leaves
    /// the one a word panel captured.
    fn disposed_run(state: State) -> RwSignal<ReviewRun> {
        let owner = Owner::new();
        let run = owner.with(|| RwSignal::new(run_with(state)));
        owner.cleanup();
        assert!(run.try_with_untracked(|_| ()).is_none(), "disposed");
        run
    }

    /// Ten deck terms started, as a profile with some practice holds.
    fn ten_started() -> State {
        let mut state = State::empty();
        for term in deck().terms.iter().take(10) {
            state.progress.push(Progress::new(term.id.clone(), today()));
        }
        state
    }

    #[test]
    fn a_disposed_run_still_offers_the_button_from_the_store() {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            provide_fakes(Load::Ok(ten_started()));
            provide_context(disposed_run(ten_started()));
            let action = entry_action(passage(), Locale::en).expect("an action");
            let html = action
                .run(("sānuvajjo".into(), entry("sānuvajja")))
                .to_html();
            assert!(html.contains(">Add to practice</button>"), "{html}");
        });
    }

    #[test]
    fn a_word_started_in_the_store_gets_no_button_when_the_run_is_gone() {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            provide_fakes(Load::Ok(added_started(Stage::Day, today())));
            provide_context(disposed_run(State::empty()));
            let action = entry_action(passage(), Locale::en).expect("an action");
            let html = action
                .run(("sānuvajjo".into(), entry("sānuvajja")))
                .to_html();
            assert!(!html.contains("<button"), "{html}");
        });
    }

    #[test]
    fn pressing_with_a_disposed_run_adds_to_the_store_and_tells_the_home() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Ok(ten_started()));
            let data = ProgressData::current();
            let run = disposed_run(ten_started());
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            let result = add_pressed(
                Some(run),
                Backend::current(),
                Some(data),
                entry,
                &passage(),
                "sānuvajjo",
            );
            assert_eq!(result, Ok(()));
            let saved = store.saved();
            assert_eq!(saved.len(), 1);
            assert_eq!(saved[0].progress.len(), 11, "ten, and the word started");
            assert_eq!(saved[0].added.len(), 1);
            assert_eq!(data.writes().0.get_untracked(), 1, "the home reads again");
        });
    }

    #[test]
    fn a_profile_with_progress_gets_the_button_in_a_run_and_without() {
        for run in [Some(ten_started()), None] {
            let html = slot(
                run,
                Load::Ok(ten_started()),
                Language::En,
                "sānuvajjo",
                "sānuvajja",
            );
            assert!(html.contains(">Add to practice</button>"), "{html}");
        }
    }

    #[test]
    fn a_word_started_elsewhere_meanwhile_is_not_started_twice() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            let backend = Backend::current();
            let run = RwSignal::new(run_with(started("kho")));
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            let id = TermId("sānuvajja".into());
            // The panel opened with the word not started.
            assert!(super::offered(Some(run), backend, &id));
            // Another tab started it, then the store says so.
            let mut elsewhere = started("kho");
            elsewhere.added.push(added_word());
            elsewhere.progress.push(Progress::new(id.clone(), today()));
            store.changed_to(Load::Ok(elsewhere));
            let result = add_pressed(Some(run), backend, None, entry, &passage(), "sānuvajjo");
            assert_eq!(settled(result), Slot::Added, "it is in the practice");
            assert_eq!(store.saved().len(), 0, "and nothing is written over it");
            run.with(|run| assert!(!run.started(&id), "the run did not start it again"));
        });
    }

    #[test]
    fn a_store_that_became_unusable_meanwhile_refuses_the_press_with_its_cause() {
        Owner::new().with(|| {
            let (store, _) = provide_fakes(Load::Empty);
            let backend = Backend::current();
            let entry = lookup(deck(), "sānuvajjo", "sānuvajja").unwrap();
            store.changed_to(Load::NewerVersion { raw: "{}".into() });
            let result = add_pressed(None, backend, None, entry, &passage(), "sānuvajjo");
            assert_eq!(result, Err(Refused::Newer));
            assert_eq!(store.saved().len(), 0);
        });
    }
}
