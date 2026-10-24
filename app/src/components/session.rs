//! A review session, shown in place of the home's content (one page, no
//! route): [`start_phase`] starts one on the stored state, synchronously,
//! [`practice_phase`] a practice ahead (every question practice, nothing in
//! the progress changes), and [`SessionView`] shows it: the session
//! header, the card, the grades, the summary, the pause when another tab
//! changed the progress.
//! Every way out (Stop, 'Back home') is a button that calls `on_home`: the
//! home shows its own content again and reads the store anew.
//!
//! # Keys
//!
//! [`ReviewView`]'s root is the ancestor of the card on which the grade keys
//! are bound (`y` / `→` Knew it, `n` / `←` Didn't know; the rule is
//! [`grade_key`]). The card's `revealed` state is the signal `RevealCard`
//! gives its `actions` closure: the closure stores that handle in a
//! `StoredValue` and the handler reads it at the keypress, so the gate sees
//! the state of that very tick. The keys bubble from the card, so they work
//! once focus is inside it (after the reveal, which moves it there, or
//! after a grade, which `RevealCard` returns to the card).
//!
//! # Hydrate only
//!
//! Reading the store, the browser date, the storage event and the editable
//! target of a key are browser glue (`store`, `today`, [`editable_target`]):
//! no-ops on the server, covered by the orchestrator's browser probe, not
//! by a native test.

use std::collections::HashMap;
use std::sync::LazyLock;

use learn_core::date::Date;
use learn_core::deck::{Blank, Deck, Example, Term, TermId};
use learn_core::facet::Facet;
use learn_core::progress::State;
use learn_core::session::{Answer, Session, nearest};
use learn_core::stage::Stage;
use learn_core::words::word_key;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos_i18n::{td, td_string};
use palicanon_ui_lib::Language;
use palicanon_ui_lib::{
    Button, Dialog, DialogSize, Disclosed, Disclosure, EntryAction, Pali, PaliSize, PaliText,
    RevealCard, SessionHeader, Size, Token, Variant, WordEntry, language_signal,
};

use crate::backend::Backend;
use crate::catalogue::Locale;
use crate::components::add_word::entry_action;
use crate::components::new_terms::NewTerms;
use crate::components::passage_badge::passage_badge;
use crate::deck::deck;
use crate::learn::home_state::BrokenKind;
use crate::learn::key::{GradeGate, grade_key};
use crate::learn::run::{Card, ReviewRun};
use crate::learn::summary::Summary;
use crate::learn::translation::{translation_for, translator_name};
use crate::learn::window::{MAX_CHARS, Segment, context_window, source_ref};
use crate::learn::words::{date_words, label_words, pos_words};
use crate::seed::browser_seed;
use crate::store::Load;
use crate::today::{browser_today, session_today};

/// The card's box: a definite height (the `RevealCard` contract, so its
/// body scrolls under the pinned grades), capped from `sm` up.
pub(crate) const CARD_BOX: &str =
    "h-[calc(100dvh-14rem)] min-h-72 w-full rounded-xl border border-line bg-card sm:h-[34rem]";
/// The small caption above a front (the facet's name).
const CAPTION: &str = "text-xs font-medium uppercase tracking-wide text-muted";
/// The answer of a revealed Context blank: the gold mark.
const MARK: &str = "rounded-sm bg-gold/20 px-0.5 text-ink";
/// The panel id of the reveal's example words (one card on the page).
const EXAMPLE_WORDS: &str = "pc-example-words";
/// The panel ids of the Context line's runs of text, one per run (a
/// `PaliText` needs a `'static` id; a window of [`MAX_CHARS`] holds far
/// fewer runs).
const LINE_WORDS: [&str; 8] = [
    "pc-line-words-0",
    "pc-line-words-1",
    "pc-line-words-2",
    "pc-line-words-3",
    "pc-line-words-4",
    "pc-line-words-5",
    "pc-line-words-6",
    "pc-line-words-7",
];
/// The practice badge.
const BADGE: &str = "w-fit rounded-full border border-gold px-3 py-1 text-xs text-gold-text";

/// Where a session is: on a stored state that cannot be used, or running.
#[derive(Clone, Copy)]
pub(crate) enum Phase {
    /// The stored progress cannot be read (nothing is started over it).
    Broken(BrokenKind),
    /// A session is running (or over).
    Running(RwSignal<ReviewRun>),
}

/// Reads the store and starts a session on what it holds: the signal of a
/// run is owned by the page (`owner`), so it outlives the callback that
/// made it. An empty store is a session with nothing due.
pub(crate) fn start_phase(owner: &Owner, backend: Backend) -> Phase {
    match backend.load() {
        Load::Ok(state) => begin(owner, backend, state),
        Load::Empty => begin(owner, backend, State::empty()),
        Load::Invalid { .. } => Phase::Broken(BrokenKind::Invalid),
        Load::NewerVersion { .. } => Phase::Broken(BrokenKind::NewerVersion),
        Load::Unreadable(_) => Phase::Broken(BrokenKind::Unreadable),
    }
}

/// A session on `state` for today (never before the state's last day),
/// saving first the promotions the start itself made.
fn begin(owner: &Owner, backend: Backend, state: State) -> Phase {
    let today = session_today(browser_today(), &state);
    let deck = backend.deck();
    match Session::start(deck, state.clone(), today, browser_seed()) {
        Ok(session) => {
            if session.changed_at_start() {
                backend.save(&session.snapshot());
            }
            Phase::Running(owner.with(|| RwSignal::new(ReviewRun::new(session, deck, state))))
        }
        Err(_) => Phase::Broken(BrokenKind::Invalid),
    }
}

/// Reads the store and starts a practice ahead of `n` terms (of stage
/// `only` when given) on it ([`practice_run`]); `None` when there is
/// nothing to practise (nothing stored, no started term in the batch).
pub(crate) fn practice_phase(
    owner: &Owner,
    backend: Backend,
    n: usize,
    only: Option<Stage>,
) -> Option<Phase> {
    match backend.load() {
        Load::Ok(state) => {
            let today = session_today(browser_today(), &state);
            let run = practice_run(backend.deck(), state, today, n, only, browser_seed())?;
            Some(Phase::Running(owner.with(|| RwSignal::new(run))))
        }
        Load::Empty => None,
        Load::Invalid { .. } => Some(Phase::Broken(BrokenKind::Invalid)),
        Load::NewerVersion { .. } => Some(Phase::Broken(BrokenKind::NewerVersion)),
        Load::Unreadable(_) => Some(Phase::Broken(BrokenKind::Unreadable)),
    }
}

/// A practice ahead on `state` of `deck` (the selected one): the `n` started terms with the nearest
/// next review ([`nearest`]), of stage `only` when given, every facet of
/// each asked as practice ([`Session::practice`]) in the order `seed` draws;
/// `None` when no term is picked.
pub(crate) fn practice_run(
    deck: &'static Deck,
    state: State,
    today: Date,
    n: usize,
    only: Option<Stage>,
    seed: u64,
) -> Option<ReviewRun> {
    let terms = nearest(&state, deck, only, n);
    if terms.is_empty() {
        return None;
    }
    let session = Session::practice(deck, &state, today, &terms, seed);
    Some(ReviewRun::practice(session, deck, state))
}

/// The stored progress cannot be used: the sentence and the way home.
#[component]
pub(crate) fn BrokenNotice(
    /// Why it cannot.
    kind: BrokenKind,
    /// 'Back home'.
    on_home: Callback<()>,
) -> impl IntoView {
    let language = language_signal();
    view! {
        <div class="flex flex-col items-start gap-4">
            <p class="text-ink">
                {move || {
                    let locale = Locale::from(language.get());
                    match kind {
                        BrokenKind::Invalid => td_string!(locale, broken_invalid),
                        BrokenKind::NewerVersion => td_string!(locale, broken_newer),
                        BrokenKind::Unreadable => td_string!(locale, broken_unreadable),
                    }
                }}
            </p>
            <Button variant=Variant::Primary on_click=move |_| on_home.run(())>
                {move || td_string!(Locale::from(language.get()), back_home)}
            </Button>
        </div>
    }
}

/// A session as the home shows it: the notice when the stored progress
/// cannot be used, else the run ([`ReviewView`]).
#[component]
pub(crate) fn SessionView(
    /// The session ([`start_phase`]).
    phase: Phase,
    /// True while the progress changed in another tab.
    #[prop(into)]
    paused: Signal<bool>,
    /// 'Reload the session' on the pause banner.
    #[prop(into)]
    on_reload: Callback<()>,
    /// Stop and 'Back home': the home shows its own content again.
    #[prop(into)]
    on_home: Callback<()>,
) -> impl IntoView {
    match phase {
        Phase::Broken(kind) => view! { <BrokenNotice kind=kind on_home=on_home /> }.into_any(),
        Phase::Running(run) => {
            view! { <ReviewView run=run paused=paused on_reload=on_reload on_home=on_home /> }
                .into_any()
        }
    }
}

/// A session as shown: the header, the card and its grades, or the
/// summary once it is over.
#[component]
pub fn ReviewView(
    /// The session being run; grading updates it.
    run: RwSignal<ReviewRun>,
    /// True while the progress changed in another tab: the card inert, the
    /// grade keys off, the pause banner shown.
    #[prop(into)]
    paused: Signal<bool>,
    /// 'Reload the session' on the pause banner: the home starts a session
    /// again from the stored state.
    #[prop(into)]
    on_reload: Callback<()>,
    /// Stop and 'Back home': the home shows its own content again.
    #[prop(into)]
    on_home: Callback<()>,
) -> impl IntoView {
    let backend = Backend::current();
    // The example words' 'Add to practice' adds through this run.
    provide_context(run);
    let stop = Disclosed::new("learn-review-stop", RwSignal::new(false));
    let revealed = StoredValue::new(None::<Signal<bool>>);
    Effect::new(move |_| {
        if paused.get() {
            stop.set_open(false);
        }
    });
    let over = Memo::new(move |_| run.with(|run| run.current().is_none()));
    let on_key = move |ev: KeyboardEvent| {
        let gate = GradeGate {
            revealed: revealed
                .get_value()
                .is_some_and(|revealed| revealed.get_untracked()),
            dialog_open: stop.signal().get_untracked(),
            paused: paused.get_untracked(),
            editable_target: editable_target(&ev),
            modifier: ev.ctrl_key() || ev.alt_key() || ev.meta_key(),
            repeat: ev.repeat(),
        };
        if let Some(answer) = grade_key(&ev.key(), gate) {
            ev.prevent_default();
            give(run, backend, answer);
        }
    };
    view! {
        <div data-review="" class="flex flex-1 flex-col gap-4" on:keydown=on_key>
            {move || paused.get().then(|| view! { <PauseBanner on_reload=on_reload /> })}
            {move || {
                if over.get() {
                    view! { <Finished run=run on_home=on_home /> }.into_any()
                } else {
                    view! {
                        <Running
                            run=run
                            paused=paused
                            stop=stop
                            revealed=revealed
                            on_home=on_home
                        />
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// Grades the current question and saves the state before the next one.
fn give(run: RwSignal<ReviewRun>, backend: Backend, answer: Answer) {
    run.update(|run| run.grade(answer, |state| backend.save(state)));
}

/// Whether a key press comes from a field the learner types in (an input,
/// a textarea, a select, a `contenteditable`): the browser's, `false` on
/// the server.
fn editable_target(ev: &KeyboardEvent) -> bool {
    #[cfg(feature = "browser")]
    {
        use wasm_bindgen::JsCast;
        const EDITABLE: &str =
            "input, textarea, select, [contenteditable]:not([contenteditable=false])";
        ev.target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|element| element.closest(EDITABLE).ok().flatten().is_some())
    }
    #[cfg(not(feature = "browser"))]
    {
        let _ = ev;
        false
    }
}

/// The banner shown while the progress changed in another tab: not modal,
/// one button restarts the session from the stored state.
#[component]
fn PauseBanner(
    /// Called by the button.
    on_reload: Callback<()>,
) -> impl IntoView {
    let language = language_signal();
    view! {
        <div
            role="status"
            class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-gold/40 bg-gold/5 px-4 py-3 text-sm text-ink"
        >
            <span>{move || td_string!(Locale::from(language.get()), pause_banner)}</span>
            <Button variant=Variant::Chip on_click=move |_| on_reload.run(())>
                {move || td_string!(Locale::from(language.get()), reload_session)}
            </Button>
        </div>
    }
}

/// The session in progress: the header, the card, the stop Dialog.
#[component]
fn Running(
    /// The session.
    run: RwSignal<ReviewRun>,
    /// The progress changed elsewhere: the card is inert.
    paused: Signal<bool>,
    /// The stop Dialog's state.
    stop: Disclosed,
    /// Where the card leaves its `revealed` signal for the grade keys.
    revealed: StoredValue<Option<Signal<bool>>>,
    /// The stop Dialog's 'Stop'.
    on_home: Callback<()>,
) -> impl IntoView {
    view! {
        <RunHeader run=run stop=stop />
        <FacetCard run=run paused=paused revealed=revealed />
        <StopDialog run=run stop=stop on_home=on_home />
    }
}

/// `done / total cards` and the close button, which opens the stop Dialog.
#[component]
fn RunHeader(
    /// The session.
    run: RwSignal<ReviewRun>,
    /// Opened by the close button.
    stop: Disclosed,
) -> impl IntoView {
    let language = language_signal();
    move || {
        let locale = Locale::from(language.get());
        let (done, total) = run.with(|run| (run.done(), run.total()));
        view! {
            <SessionHeader
                done=done
                total=total
                unit=td_string!(locale, unit_cards)
                close_label=td_string!(locale, stop_label)
                on_close=Callback::new(move |()| stop.set_open(true))
            />
        }
    }
}

/// Whether `count` takes the singular sentence in `locale`: French counts
/// 0 and 1 as singular, English only 1 (Vietnamese has one form, so either
/// key says the same).
pub(crate) fn singular(locale: Locale, count: usize) -> bool {
    if matches!(locale, Locale::fr) {
        count <= 1
    } else {
        count == 1
    }
}

/// 'Your n answer(s) are kept.'
fn kept_text(locale: Locale, count: usize) -> AnyView {
    if singular(locale, count) {
        td!(locale, stop_body_1, count = count).into_any()
    } else {
        td!(locale, stop_body_n, count = count).into_any()
    }
}

/// 'Stop here?': keep going (closes), or stop (closes, then `on_home`;
/// every answer is already saved).
#[component]
fn StopDialog(
    /// The session, for the count of answers kept.
    run: RwSignal<ReviewRun>,
    /// What opens and closes the dialog.
    stop: Disclosed,
    /// Called by 'Stop'.
    on_home: Callback<()>,
) -> impl IntoView {
    let language = language_signal();
    let title =
        Signal::derive(move || td_string!(Locale::from(language.get()), stop_title).to_owned());
    view! {
        <Dialog
            controls=stop
            title=title
            size=DialogSize::Popup
            footer=move || {
                view! {
                    <Button variant=Variant::Chip on_click=move |_| stop.set_open(false)>
                        {move || td_string!(Locale::from(language.get()), keep_going)}
                    </Button>
                    <Button
                        variant=Variant::Primary
                        on_click=move |_| {
                            stop.set_open(false);
                            on_home.run(());
                        }
                    >
                        {move || td_string!(Locale::from(language.get()), stop)}
                    </Button>
                }
            }
        >
            <p class="text-sm leading-6 text-muted">
                {move || kept_text(Locale::from(language.get()), run.with(ReviewRun::done))}
            </p>
        </Dialog>
    }
}

/// The card: a `RevealCard` in a box of definite height, inert while
/// paused. It stays mounted from question to question (the key changes);
/// only a language change builds it again.
#[component]
fn FacetCard(
    /// The session.
    run: RwSignal<ReviewRun>,
    /// The progress changed elsewhere.
    paused: Signal<bool>,
    /// Receives the card's `revealed` signal for the grade keys.
    revealed: StoredValue<Option<Signal<bool>>>,
) -> impl IntoView {
    let language = language_signal();
    let backend = Backend::current();
    let key = Signal::derive(move || run.with(ReviewRun::key));
    // The card's `revealed`, handed out by `RevealCard` to `actions` (its
    // only channel): a signal, so the front reads it whatever the order
    // the slots are built in, and each new card (a language change) hands
    // its own.
    let handed = RwSignal::new(None::<Signal<bool>>);
    let shown = Signal::derive(move || handed.get().is_some_and(|signal| signal.get()));
    // The sign tapped on a choice card (071), with the key of the card it
    // was tapped on: another card reads untapped.
    let tapped = RwSignal::new(None::<(String, TermId)>);
    move || {
        let locale = Locale::from(language.get());
        let choices = run.with(ReviewRun::choices);
        view! {
            <div class=CARD_BOX inert=move || paused.get().then_some("")>
                {match choices {
                    Some(choices) => {
                        let answered = Signal::derive(move || {
                            tapped.with(|t| t.as_ref().is_some_and(|(k, _)| *k == key.get()))
                        });
                        revealed.set_value(None);
                        handed.set(Some(answered));
                        choice_card(run, backend, paused, tapped, choices, language.get())
                            .into_any()
                    }
                    None => {
                        // The grade keys stay shut on a choice card: the tap
                        // judges, a key must not (review 071).
                        view! {
                            <RevealCard
                                front=move || view! { <CardFront run=run revealed=shown /> }
                                reveal=move || view! { <CardReveal run=run /> }
                                actions=move |is_revealed| {
                                    revealed.set_value(Some(is_revealed));
                                    handed.set(Some(is_revealed));
                                    grade_buttons(run, backend, paused, is_revealed, locale)
                                }
                                key=key
                                reveal_label=td_string!(locale, review_show)
                            />
                        }
                            .into_any()
                    }
                }}
            </div>
        }
    }
}

/// A script deck's write card (071): the romanisation, four signs to tap;
/// the tap judges at once (the tapped sign pressed, the right one named),
/// and Next gives the grade. No 'Show the answer': the app judges.
fn choice_card(
    run: RwSignal<ReviewRun>,
    backend: Backend,
    paused: Signal<bool>,
    tapped: RwSignal<Option<(String, TermId)>>,
    choices: Vec<&'static Term>,
    language: Language,
) -> AnyView {
    let locale = Locale::from(language);
    let key = run.with(ReviewRun::key);
    let Some(card) = run.with(ReviewRun::card) else {
        return ().into_any();
    };
    let answer = card.term.id.clone();
    let (chosen, answered, right) = choice_signals(tapped, &key, &answer);
    let row = ChoiceRow {
        answer: answer.clone(),
        key: key.clone(),
        facet: card.facet,
        tapped,
        chosen,
        answered,
    };
    let buttons = choices
        .into_iter()
        .map(|term| choice_button(term, &row))
        .collect_view();
    let answer_sign: &'static str = choice_text(card.term, card.facet);
    view! {
        <div data-choice-card="">
            {front_view(card, language, answered)}
            <div
                data-choices=""
                role="group"
                aria-label=td_string!(locale, quiz_choices)
                class="flex flex-wrap gap-2 px-4"
            >
                {buttons}
            </div> {choice_verdict(answered, right, answer_sign, card.facet, locale)}
            {choice_next(run, backend, answered, right, paused, locale)}
        </div>
    }
    .into_any()
}

/// Once answered: right, or not that one and the sign.
fn choice_verdict(
    answered: Signal<bool>,
    right: Signal<bool>,
    answer_sign: &'static str,
    facet: Facet,
    locale: Locale,
) -> impl IntoView {
    // The live region is there from the start, empty, and filled on the
    // tap: a region created with its content is not announced (review 071).
    view! {
        <div data-verdict="" role="status" aria-live="polite" class="flex flex-col gap-2 px-4">
            <Show when=move || answered.get()>
                <p class=CAPTION>
                    {move || {
                        if right.get() {
                            td_string!(locale, quiz_right).to_string()
                        } else if facet == Facet::Recall {
                            td_string!(locale, quiz_wrong).to_string()
                        } else {
                            td_string!(locale, quiz_wrong_read).to_string()
                        }
                    }}
                </p>
                <Show when=move || !right.get()>
                    <Pali size=PaliSize::Display>{answer_sign}</Pali>
                </Show>
            </Show>
        </div>
    }
}

/// Moves the focus to Next once a sign is tapped (the four chips go
/// disabled, which would drop the focus to the body); the browser only.
fn focus_next() {
    #[cfg(feature = "browser")]
    {
        use wasm_bindgen::JsCast;
        if let Some(next) = document()
            .query_selector("[data-choice-next]")
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = next.focus();
        }
    }
}

/// Next: gives the grade the tap earned (Pass when right, Fail otherwise).
fn choice_next(
    run: RwSignal<ReviewRun>,
    backend: Backend,
    answered: Signal<bool>,
    right: Signal<bool>,
    paused: Signal<bool>,
    locale: Locale,
) -> impl IntoView {
    // The focus moves to Next once the DOM has enabled it: after the tap's
    // render, not inside the tap's handler (review 071, round 2).
    Effect::new(move |_| {
        if answered.get() {
            focus_next();
        }
    });
    view! {
        <div class="flex gap-3 p-4">
            <Button
                variant=Variant::Primary
                size=Size::Lg
                layout="flex-1"
                disabled=Signal::derive(move || !answered.get() || paused.get())
                on_click=move |_| {
                    give(run, backend, if right.get() { Answer::Pass } else { Answer::Fail });
                }
                {..}
                data-choice-next=""
            >
                {td_string!(locale, quiz_next)}
            </Button>
        </div>
    }
}

/// What a choice shows once the card is answered (072, user: « en vert
/// quand c'est bon, sinon en rouge et le bon en vert »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChoiceState {
    /// Not answered yet: a chip to tap.
    Open,
    /// The answer, tapped or not: green.
    Right,
    /// Tapped and not the answer: red.
    Wrong,
    /// Neither, once answered: dimmed.
    Other,
}

/// The classes of a choice in `state`: the whole look per state, nothing
/// toggled on one property (ui-components.md).
fn choice_classes(state: ChoiceState) -> &'static str {
    match state {
        ChoiceState::Open => {
            "inline-flex min-w-0 flex-1 cursor-pointer select-none items-center justify-center \
             gap-2 rounded-full border border-line bg-chip px-3 py-2 text-muted transition \
             hover:border-gold/60 hover:text-heading active:bg-chip-hover focus-visible:outline-2 \
             focus-visible:outline-offset-2 focus-visible:outline-gold disabled:cursor-default"
        }
        ChoiceState::Right => {
            "inline-flex min-w-0 flex-1 select-none items-center justify-center gap-2 rounded-full \
             border border-success bg-success/10 px-3 py-2 text-success transition disabled:cursor-default"
        }
        ChoiceState::Wrong => {
            "inline-flex min-w-0 flex-1 select-none items-center justify-center gap-2 rounded-full \
             border border-danger bg-danger/10 px-3 py-2 text-danger transition disabled:cursor-default"
        }
        ChoiceState::Other => {
            "inline-flex min-w-0 flex-1 select-none items-center justify-center gap-2 rounded-full \
             border border-line bg-chip px-3 py-2 text-muted opacity-50 transition disabled:cursor-default"
        }
    }
}

/// The state of the choice `id` on a card whose answer is `answer`.
fn choice_state(id: &TermId, answer: &TermId, chosen: Option<&TermId>) -> ChoiceState {
    match chosen {
        None => ChoiceState::Open,
        Some(_) if id == answer => ChoiceState::Right,
        Some(tapped) if tapped == id => ChoiceState::Wrong,
        Some(_) => ChoiceState::Other,
    }
}

/// The card's three signals from the tap: the sign chosen on this card
/// (by key), whether it is answered, whether the choice was right.
fn choice_signals(
    tapped: RwSignal<Option<(String, TermId)>>,
    key: &str,
    answer: &TermId,
) -> (Signal<Option<TermId>>, Signal<bool>, Signal<bool>) {
    let chosen = Signal::derive({
        let key = key.to_string();
        move || {
            tapped.with(|t| {
                t.as_ref()
                    .filter(|(k, _)| *k == key)
                    .map(|(_, id)| id.clone())
            })
        }
    });
    let answered = Signal::derive(move || chosen.with(Option::is_some));
    let right = Signal::derive({
        let answer = answer.clone();
        move || chosen.with(|c| c.as_ref() == Some(&answer))
    });
    (chosen, answered, right)
}

/// What a choice shows, and the verdict names: the sign on a write card
/// (Recall: the romanisation is the prompt), the romanisation on a read
/// card (Meaning: the sign is the prompt). In a script deck the item's
/// `meaning` is its romanisation (063).
fn choice_text(term: &'static Term, facet: Facet) -> &'static str {
    if facet == Facet::Recall {
        term.lemma.as_str()
    } else {
        term.meaning.en.as_str()
    }
}

/// What every choice of a card shares: the answer, the card's key and
/// facet, and the card's signals.
#[derive(Clone)]
struct ChoiceRow {
    answer: TermId,
    key: String,
    facet: Facet,
    tapped: RwSignal<Option<(String, TermId)>>,
    chosen: Signal<Option<TermId>>,
    answered: Signal<bool>,
}

/// One of the four choices: the sign on a write card, the romanisation on a
/// read card; a chip until the tap, then green (the answer), red (a wrong
/// tap) or dimmed; `aria-pressed` on the tapped one, `data-right` on the
/// answer once answered; disabled after the tap.
fn choice_button(term: &'static Term, row: &ChoiceRow) -> impl IntoView {
    let (tapped, chosen, answered, facet) = (row.tapped, row.chosen, row.answered, row.facet);
    let id = term.id.clone();
    let answer = row.answer.clone();
    let key = row.key.clone();
    let is_answer = id == answer;
    let state = Signal::derive({
        let (id, answer) = (id.clone(), answer.clone());
        move || chosen.with(|c| choice_state(&id, &answer, c.as_ref()))
    });
    let pressed = Signal::derive({
        let id = id.clone();
        move || chosen.with(|c| c.as_ref() == Some(&id))
    });
    let text = choice_text(term, facet);
    let shown = if facet == Facet::Recall {
        view! { <Pali size=PaliSize::Display>{text}</Pali> }.into_any()
    } else {
        view! { <span class="text-xl text-heading">{text}</span> }.into_any()
    };
    view! {
        <button
            type="button"
            class=move || choice_classes(state.get())
            aria-disabled=move || answered.get().then_some("true")
            aria-pressed=move || pressed.get().to_string()
            data-right=move || (is_answer && answered.get()).then_some("")
            on:click=move |_| {
                if !answered.get_untracked() {
                    tapped.set(Some((key.clone(), id.clone())));
                }
            }
        >
            {shown}
        </button>
    }
}

/// The two grade buttons, greyed until the reveal and while paused.
fn grade_buttons(
    run: RwSignal<ReviewRun>,
    backend: Backend,
    paused: Signal<bool>,
    revealed: Signal<bool>,
    locale: Locale,
) -> AnyView {
    let disabled = Signal::derive(move || !revealed.get() || paused.get());
    view! {
        <div class="flex gap-3 p-4">
            <Button
                variant=Variant::Chip
                size=Size::Lg
                layout="flex-1"
                disabled=disabled
                on_click=move |_| give(run, backend, Answer::Fail)
            >
                {td_string!(locale, grade_didnt)}
            </Button>
            <Button
                variant=Variant::Primary
                size=Size::Lg
                layout="flex-1"
                disabled=disabled
                on_click=move |_| give(run, backend, Answer::Pass)
            >
                {td_string!(locale, grade_knew)}
            </Button>
        </div>
    }
    .into_any()
}

/// The facet's name as the caption shows it.
fn facet_name(locale: Locale, facet: Facet) -> &'static str {
    match facet {
        Facet::Meaning => td_string!(locale, facet_meaning),
        Facet::Recall => td_string!(locale, facet_recall),
        Facet::Forms => td_string!(locale, facet_forms),
        Facet::Context => td_string!(locale, facet_context),
    }
}

/// The front of the current question, in the current language.
#[component]
fn CardFront(
    /// The session.
    run: RwSignal<ReviewRun>,
    /// Whether the card is revealed: the Context front fills its blanks.
    #[prop(into)]
    revealed: Signal<bool>,
) -> impl IntoView {
    let language = language_signal();
    move || {
        let language = language.get();
        run.with(ReviewRun::card)
            .map(|card| front_view(card, language, revealed))
    }
}

/// The practice badge (when it is one), the facet's name, then the body of
/// the facet.
fn front_view(card: Card, language: Language, revealed: Signal<bool>) -> impl IntoView {
    let locale = Locale::from(language);
    view! {
        <div class="flex flex-col gap-2 p-4">
            {card
                .practice
                .then(|| view! { <p class=BADGE>{td_string!(locale, practice_badge)}</p> })}
            <p class=CAPTION>{facet_name(locale, card.facet)}</p>
            {front_body(card, language, revealed)}
        </div>
    }
}

/// What each facet asks.
fn front_body(card: Card, language: Language, revealed: Signal<bool>) -> AnyView {
    match card.facet {
        Facet::Meaning => meaning_front(card, language).into_any(),
        Facet::Recall => recall_front(card, language).into_any(),
        Facet::Forms => forms_front(card, language).into_any(),
        Facet::Context => context_front(card, language, revealed).into_any(),
    }
}

/// Meaning: the lemma, the part of speech in words.
fn meaning_front(card: Card, language: Language) -> impl IntoView {
    view! {
        <Pali size=PaliSize::Display>{card.term.lemma.clone()}</Pali>
        <p class="text-sm text-muted">{pos_words(language, &card.term.pos.0)}</p>
    }
}

/// Recall: the English sense, the literal meaning in brackets, the part of
/// speech in words.
fn recall_front(card: Card, language: Language) -> impl IntoView {
    let term = card.term;
    view! {
        <p class="text-lg leading-8 text-ink">{term.meaning.en.clone()}</p>
        {term
            .meaning
            .literal
            .as_ref()
            .map(|literal| view! { <p class="text-sm text-muted">{format!("[{literal}]")}</p> })}
        <p class="text-sm text-muted">{pos_words(language, &term.pos.0)}</p>
    }
}

/// Forms: 'Give the <form in words> of <lemma>.'
fn forms_front(card: Card, language: Language) -> impl IntoView {
    let locale = Locale::from(language);
    let label = card
        .form
        .map(|form| label_words(language, &form.label))
        .unwrap_or_default();
    view! {
        <p class="text-lg leading-8 text-ink">
            {td_string!(locale, forms_before)}<strong class="font-semibold">{label}</strong>
            {td_string!(locale, forms_between)} <strong class="font-semibold">
                <Pali size=PaliSize::Body>{card.term.lemma.clone()}</Pali>
            </strong> {td_string!(locale, forms_after)}
        </p>
    }
}

/// Context: the windowed line, its blanks filled in place once
/// `revealed` (the reveal does not repeat the line), under it the line's
/// translation ([`translation_view`]), then where it comes from
/// (`Dhp 127 · <sutta>`).
fn context_front(card: Card, language: Language, revealed: Signal<bool>) -> impl IntoView {
    let example = card.term.example.as_ref();
    let locale = Locale::from(language);
    let action = example.and_then(|example| entry_action(example.clone(), locale));
    let line = example
        .and_then(|example| example.blank.as_ref())
        .map(|blank| {
            view! {
                <p class="text-lg leading-8 text-ink">
                    <Pali layout="whitespace-pre-line">
                        {context_line(blank, revealed, locale, action)}
                    </Pali>
                </p>
            }
        });
    let translation = example.and_then(|example| translation_view(example, language));
    let origin = example.map(|example| {
        view! {
            <p class="text-sm text-muted">
                {passage_badge(example, language)
                    .unwrap_or_else(|| source_ref(&example.source).into_any())} " · "
                <Pali>{example.sutta.clone()}</Pali>
            </p>
        }
    });
    view! {
        {line}
        {translation}
        {origin}
    }
}

/// The example's translation in the page's language, English when that
/// one is missing, with its translator after a middle dot (no em dash in
/// UI copy): italic and muted; nothing when the example has none. Shown on
/// the Context front and in the reveal's answer block.
fn translation_view(example: &Example, language: Language) -> Option<impl IntoView + use<>> {
    translation_for(example, language).map(|translation| {
        let credit = format!(" · {}", translator_name(&translation.translator));
        view! {
            <p class="mt-2 text-sm italic text-muted">
                {translation.text.clone()} <span class="not-italic">{credit}</span>
            </p>
        }
    })
}

/// The example's line, windowed ([`context_window`]), as runs of text and
/// its blank(s): `___` until `revealed`, then the answer in a gold `<mark>`
/// in the same place, and the words of the runs open their meaning
/// ([`line_text`], each word's panel offering `action`); the marked answer
/// is never a button.
fn context_line(
    blank: &Blank,
    revealed: Signal<bool>,
    locale: Locale,
    action: Option<EntryAction>,
) -> impl IntoView + use<> {
    let answer = blank.answer.clone();
    let mut ids = LINE_WORDS.into_iter();
    context_window(blank, MAX_CHARS)
        .into_iter()
        .map(|segment| match segment {
            Segment::Text(text) => line_text(ids.next(), text, revealed, locale, action),
            Segment::Blank => {
                let answer = answer.clone();
                (move || blank_view(revealed.get(), answer.clone())).into_any()
            }
        })
        .collect_view()
}

/// A run of the Context line: plain text until `revealed`, then a
/// [`PaliText`] under its own panel id; a run past [`LINE_WORDS`] stays
/// plain.
fn line_text(
    id: Option<&'static str>,
    text: String,
    revealed: Signal<bool>,
    locale: Locale,
    action: Option<EntryAction>,
) -> AnyView {
    let Some(id) = id else {
        return text.into_any();
    };
    (move || {
        if revealed.get() {
            example_words(id, text.clone(), locale, action).into_any()
        } else {
            text.clone().into_any()
        }
    })
    .into_any()
}

/// One blank: `___`, or the answer marked once revealed.
fn blank_view(revealed: bool, answer: String) -> AnyView {
    if revealed {
        view! { <mark class=MARK>{answer}</mark> }.into_any()
    } else {
        view! { <span aria-label="blank">"___"</span> }.into_any()
    }
}

/// The answer side of the current question, in the current language.
#[component]
fn CardReveal(
    /// The session.
    run: RwSignal<ReviewRun>,
) -> impl IntoView {
    let language = language_signal();
    move || {
        let language = language.get();
        run.with(ReviewRun::card)
            .map(|card| reveal_view(card, language))
    }
}

/// The practice note, the answer, then what the term says about itself.
/// Context gives its answer block ([`context_answer`]) in place of the
/// bare sense.
fn reveal_view(card: Card, language: Language) -> impl IntoView {
    let locale = Locale::from(language);
    let answer = if card.facet == Facet::Context {
        context_answer(card, language).into_any()
    } else {
        view! {
            {answer_view(card)}
            <p>{card.term.meaning.en.clone()}</p>
        }
        .into_any()
    };
    view! {
        <div class="flex flex-col gap-4 p-4 text-ink">
            {card
                .practice
                .then(|| {
                    view! { <p class="text-sm text-muted">{td_string!(locale, practice_note)}</p> }
                })} {answer} {details_view(card, language)}
        </div>
    }
}

/// Context's answer, under a hairline: the lemma in bold Pāli with its
/// part of speech in words, then the sense labelled 'Meaning', larger and
/// in the heading colour, and the literal sense under it, muted, when the
/// term has one.
fn context_answer(card: Card, language: Language) -> impl IntoView {
    let locale = Locale::from(language);
    let term = card.term;
    let literal = term
        .meaning
        .literal
        .as_ref()
        .map(|literal| view! { <p class="text-sm text-muted">{format!("[{literal}]")}</p> });
    view! {
        <div class="flex flex-col gap-1 border-t border-line pt-4 mt-4">
            <p>
                <strong class="font-semibold">
                    <Pali>{term.lemma.clone()}</Pali>
                </strong>
                " · "
                {pos_words(language, &term.pos.0)}
            </p>
            <p class=CAPTION>{td_string!(locale, reveal_meaning)}</p>
            <p class="text-lg text-heading">{term.meaning.en.clone()}</p>
            {literal}
        </div>
    }
}

/// The answer of the facets whose answer is not the meaning: the lemma
/// (Recall) or every value of the form asked (Forms).
fn answer_view(card: Card) -> impl IntoView {
    match card.facet {
        Facet::Recall => {
            Some(view! { <Pali size=PaliSize::Display>{card.term.lemma.clone()}</Pali> }.into_any())
        }
        Facet::Forms => card.form.map(|form| {
            view! { <Pali size=PaliSize::Lemma>{form.values.join(", ")}</Pali> }.into_any()
        }),
        Facet::Meaning | Facet::Context => None,
    }
}

/// A small caption over some content.
fn captioned(caption: &'static str, body: impl IntoView + 'static) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-1">
            <p class=CAPTION>{caption}</p>
            {body}
        </div>
    }
}

/// The literal meaning, the construction, the root, the example (and its
/// translation, [`translation_view`]): each only when the term has it.
/// Context shows neither the literal (its answer block has it) nor the
/// example (its front's line is filled in place, its translation under it).
fn details_view(card: Card, language: Language) -> impl IntoView {
    let locale = Locale::from(language);
    let term = card.term;
    let context = card.facet == Facet::Context;
    let literal = term
        .meaning
        .literal
        .as_ref()
        .filter(|_| !context)
        .map(|literal| {
            captioned(
                td_string!(locale, reveal_literal),
                view! { <p>{literal.clone()}</p> },
            )
        });
    let construction = term.construction.as_ref().map(|construction| {
        let body = view! { <Pali>{construction.clone()}</Pali> };
        captioned(td_string!(locale, reveal_construction), body)
    });
    let root = term.root.as_ref().map(|root| {
        let body = view! {
            <p>
                <Pali>{root.key.clone()}</Pali>
                {format!(" ({})", root.meaning)}
            </p>
        };
        captioned(td_string!(locale, reveal_root), body)
    });
    let example = term.example.as_ref().filter(|_| !context).map(|example| {
        let action = entry_action(example.clone(), locale);
        let body = view! {
            <p>{example_words(EXAMPLE_WORDS, example.text.clone(), locale, action)}</p>
            {translation_view(example, language)}
            {passage_badge(example, language).map(|badge| view! { <p class="mt-2">{badge}</p> })}
        };
        captioned(td_string!(locale, reveal_example), body)
    });
    view! {
        {literal}
        {construction}
        {root}
        {example}
    }
}

/// An example passage whose words open their meaning: a [`PaliText`]
/// over the deck's words ([`passage_entries`]), its "+ n more" line in the
/// page's language, each entry's panel offering `action` when given
/// ([`entry_action`]). `id` names the panel, unique on the page.
fn example_words(
    id: &'static str,
    text: String,
    locale: Locale,
    action: Option<EntryAction>,
) -> impl IntoView + use<> {
    let live = LiveWords::new(passage_entries(&text), locale);
    let more_label = Callback::new(move |count: usize| more_words(locale, count));
    // The prop is optional, not an `Option`: two arms, the same look.
    match action {
        Some(action) => view! {
            <PaliText
                id=id
                text=text
                entries=live.entries
                more_label=more_label
                entry_action=action
                layout="whitespace-pre-line"
                lookup=live.lookup
                empty_label=live.label
            />
        }
        .into_any(),
        None => view! {
            <PaliText
                id=id
                text=text
                entries=live.entries
                more_label=more_label
                layout="whitespace-pre-line"
                lookup=live.lookup
                empty_label=live.label
            />
        }
        .into_any(),
    }
}

/// The words of one line, tappable everywhere (065): the deck's compiled
/// entries first; a word the deck does not know is looked up by its exact
/// form through the server (`lookup_word`) when opened, and its senses
/// join the map in place; the panel says looking up, then no entry.
#[derive(Clone, Copy)]
struct LiveWords {
    entries: RwSignal<HashMap<String, Vec<WordEntry>>>,
    label: Signal<String>,
    lookup: Callback<String>,
}

impl LiveWords {
    fn new(known: HashMap<String, Vec<WordEntry>>, locale: Locale) -> LiveWords {
        let entries = RwSignal::new(known);
        let state = RwSignal::new(LookupState::Pending);
        let label = Signal::derive(move || lookup_label(locale, state.get()));
        // The host's dictionary (068), read here, in the component.
        let words = StoredValue::new_local(crate::backend::Words::current());
        let lookup = Callback::new(move |form: String| {
            let key = form.to_lowercase();
            if entries.with_untracked(|map| map.contains_key(&key)) {
                return;
            }
            let Some(dictionary) = words.try_get_value().flatten() else {
                state.set(LookupState::Unavailable);
                return;
            };
            state.set(LookupState::Pending);
            dictionary.lookup(
                form,
                Box::new(move |answer| {
                    let senses = match answer {
                        crate::dictionary::LookupAnswer::Senses(senses) => senses,
                        crate::dictionary::LookupAnswer::Unreachable => {
                            state.try_set(LookupState::Unreachable);
                            return;
                        }
                    };
                    if senses.is_empty() {
                        state.try_set(LookupState::None);
                        return;
                    }
                    let found: Vec<WordEntry> =
                        senses.into_iter().map(|s| live_entry(&s, locale)).collect();
                    entries.try_update(|map| {
                        map.insert(key, found);
                    });
                }),
            );
        });
        LiveWords {
            entries,
            label,
            lookup,
        }
    }
}

/// Where a live lookup stands, for the panel's line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LookupState {
    Pending,
    None,
    Unreachable,
    /// This host has no dictionary (068).
    Unavailable,
}

fn lookup_label(locale: Locale, state: LookupState) -> String {
    match state {
        LookupState::Pending => td_string!(locale, lookup_pending),
        LookupState::None => td_string!(locale, lookup_none),
        LookupState::Unreachable => td_string!(locale, lookup_error),
        LookupState::Unavailable => td_string!(locale, search_unavailable),
    }
    .to_string()
}

/// A live sense as the panel shows it: the lemma, the part of speech, the
/// meaning in the page's language when the DPD has it, else English.
fn live_entry(sense: &crate::dictionary::Sense, locale: Locale) -> WordEntry {
    let meaning = match locale {
        Locale::fr => sense.fr.clone().unwrap_or_else(|| sense.en.clone()),
        _ => sense.en.clone(),
    };
    WordEntry {
        headword: sense.lemma.clone(),
        tag: Some(sense.pos.clone()),
        meaning,
    }
}

static DICTIONARY: LazyLock<HashMap<String, Vec<WordEntry>>> = LazyLock::new(|| {
    deck()
        .words
        .iter()
        .map(|(key, entries)| (key.clone(), entries.iter().map(ui_entry).collect()))
        .collect()
});

/// A passage's entries keyed as [`PaliText`] looks a word up (as written,
/// lower-cased), each found in the deck by its [`word_key`] (ṁ, curly
/// apostrophes and quotes normalised); a word the deck lacks is absent.
fn passage_entries(text: &str) -> HashMap<String, Vec<WordEntry>> {
    palicanon_ui_lib::words(text)
        .into_iter()
        .filter_map(|token| match token {
            Token::Word(word) => DICTIONARY
                .get(&word_key(&word))
                .map(|entries| (word.to_lowercase(), entries.clone())),
            Token::Other(_) => None,
        })
        .collect()
}

/// A deck entry as `PaliText` shows it: its tag is the part of speech,
/// then the form's grammar when the DPD gives it (`aor · aor 3rd sg`).
fn ui_entry(entry: &learn_core::words::WordEntry) -> WordEntry {
    let tag = match &entry.grammar {
        Some(grammar) => format!("{} · {grammar}", entry.pos),
        None => entry.pos.clone(),
    };
    WordEntry {
        headword: entry.headword.clone(),
        tag: Some(tag),
        meaning: entry.meaning.clone(),
    }
}

/// The line under a capped entry list: '+ 2 more'.
fn more_words(locale: Locale, count: usize) -> String {
    td_string!(locale, words_more, count = count).to_string()
}

/// The end: the summary of a session, or 'Nothing due today' when none
/// started (Q6), then the way home and the new-terms control; for a
/// practice ahead, [`practice_done`].
#[component]
fn Finished(
    /// The session, over.
    run: RwSignal<ReviewRun>,
    /// 'Back home'.
    on_home: Callback<()>,
) -> impl IntoView {
    let language = language_signal();
    let backend = Backend::current();
    let add = Callback::new(move |count: usize| {
        run.update(|run| {
            run.add_new(count, |state| backend.save(state));
        });
    });
    move || {
        let language = language.get();
        let locale = Locale::from(language);
        if run.with_untracked(ReviewRun::is_practice) {
            return practice_done(locale, on_home);
        }
        let (nothing, summary, new_left) =
            run.with_untracked(|run| (run.done() == 0, run.summary(), run.new_left()));
        let heading = if nothing {
            view! { <h2 class="font-serif text-xl text-heading">{td_string!(locale, summary_nothing_due)}</h2> }
            .into_any()
        } else {
            summary_view(summary, language).into_any()
        };
        let more = view! {
            <section class="flex w-full flex-col items-center gap-3 border-t border-line pt-5">
                <p class="text-sm text-muted">{td_string!(locale, new_more_today)}</p>
                <NewTerms on_add=add new_left=new_left add_variant=Variant::Accent />
            </section>
        };
        done_card(locale, on_home, heading, Some(more.into_any()))
    }
}

/// The card every end of a session shares, centred under the brand, in
/// width and in the height left under it (the area grows): the
/// `heading` (with its counts), the one Primary 'Back home', then the
/// `more` section when there is one.
fn done_card(
    locale: Locale,
    on_home: Callback<()>,
    heading: AnyView,
    more: Option<AnyView>,
) -> AnyView {
    view! {
        <div data-session-done-area="" class="flex flex-1 items-center justify-center">
            <div
                data-session-done=""
                class="mx-auto flex w-full max-w-md flex-col items-center gap-6 rounded-xl border border-line bg-card p-6 text-center"
            >
                {heading}
                <Button variant=Variant::Primary size=Size::Lg on_click=move |_| on_home.run(())>
                    {td_string!(locale, back_home)}
                </Button>
                {more}
            </div>
        </div>
    }
    .into_any()
}

/// The end of a practice ahead: 'Practice done' and 'Back home'; nothing
/// moved, so no summary lines and no new terms.
fn practice_done(locale: Locale, on_home: Callback<()>) -> AnyView {
    let heading = view! {
        <h2 class="font-serif text-xl text-heading">{td_string!(locale, summary_practice_done)}</h2>
    };
    done_card(locale, on_home, heading.into_any(), None)
}

/// 'Session done', the terms that moved up and those that start again
/// tomorrow, the next due date; no score.
fn summary_view(summary: Summary, language: Language) -> impl IntoView {
    let locale = Locale::from(language);
    let up = summary.moved_up;
    let again = summary.back_to_day;
    let moved = if singular(locale, up) {
        td!(locale, summary_moved_up_1, count = count_numeral(up)).into_any()
    } else {
        td!(locale, summary_moved_up_n, count = count_numeral(up)).into_any()
    };
    let back = if singular(locale, again) {
        td!(locale, summary_again_1, count = count_numeral(again)).into_any()
    } else {
        td!(locale, summary_again_n, count = count_numeral(again)).into_any()
    };
    let next = summary
        .next_due
        .map(|date| td!(locale, summary_next_due, date = date_words(language, date)));
    view! {
        <div class="flex flex-col items-center gap-3">
            <h2 class="font-serif text-xl text-heading">{td_string!(locale, summary_title)}</h2>
            <ul class="flex flex-col gap-1 text-ink">
                <li>{moved}</li>
                <li>{back}</li>
                {next.map(|next| view! { <li class="pt-2 text-sm text-muted">{next}</li> })}
            </ul>
        </div>
    }
}

/// A count in the summary: a larger serif numeral beside its words. A
/// closure, so the view can be cloned into the translation.
fn count_numeral(count: usize) -> impl IntoView + Clone {
    move || {
        view! {
            <span data-count="" class="font-serif text-2xl text-heading">
                {count}
            </span>
        }
    }
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
pub(crate) mod tests {
    use learn_core::date::Date;
    use learn_core::deck::TermId;
    use learn_core::facet::Facet;
    use learn_core::progress::{Progress, State};
    use learn_core::session::{Answer, Session};
    use learn_core::stage::Stage;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::deck::deck;

    fn today() -> Date {
        Date::parse("2026-10-05").unwrap()
    }

    /// A run on the Devanāgarī deck with `sign` started and `passed` passed
    /// (071).
    fn script_run(sign: &str, passed: &[Facet]) -> ReviewRun {
        let deck = crate::deck::devanagari();
        let mut entry = Progress::new(TermId(sign.to_string()), today());
        for facet in passed {
            entry.passed.insert(*facet);
        }
        let mut before = State::empty();
        before.progress.push(entry);
        let session = Session::start(deck, before.clone(), today(), 0).unwrap();
        ReviewRun::new(session, deck, before)
    }

    #[test]
    fn a_script_write_card_is_four_signs_to_tap_and_next_with_no_reveal() {
        let html = render(script_run("क", &[Facet::Meaning]));
        assert!(html.contains("data-choice-card"), "{html}");
        let start = html.find("data-choices").expect("the row");
        let row = &html[start
            ..html[start..]
                .find("data-verdict")
                .map_or(html.len(), |i| start + i)];
        assert!(row.matches("<button").count() >= 4, "four signs: {row}");
        assert!(row.contains(">क<"), "the answer among them: {row}");
        assert!(row.contains(r#"aria-pressed="false""#), "{row}");
        assert!(
            !row.contains("data-right"),
            "nothing named before a tap: {row}"
        );
        assert!(
            !html.contains("Show the answer"),
            "no reveal on a choice card: {html}"
        );
        assert!(
            html.contains(r#"role="status""#),
            "a live region from the start: {html}"
        );
        assert!(
            !html.contains("Not that one") && !html.contains(">Right.<"),
            "the region is empty before a tap: {html}"
        );
        let next = tag_before(&html, ">Next<");
        assert!(next.contains("disabled"), "Next waits for the tap: {next}");
        assert!(
            !row.contains("aria-disabled"),
            "the choices are live before the tap: {row}"
        );
        assert!(
            next.contains("data-choice-next"),
            "the focus target: {next}"
        );
        assert!(!html.contains("Knew it"), "no grade buttons: {html}");
    }

    #[test]
    fn a_script_read_card_offers_four_romanisations_to_tap() {
        let html = render(script_run("क", &[]));
        assert!(
            html.contains("data-choice-card"),
            "072: the read card chooses too: {html}"
        );
        assert!(!html.contains("Show the answer"), "{html}");
        let start = html.find("data-choices").expect("the row");
        let row = &html[start
            ..html[start..]
                .find("data-verdict")
                .map_or(html.len(), |i| start + i)];
        assert!(
            row.contains(">ka<"),
            "the answer's romanisation among the four: {row}"
        );
        assert!(!row.contains(">क<"), "a read card offers no sign: {row}");
        assert_eq!(row.matches("<button").count(), 4, "{row}");
    }

    #[test]
    fn a_choice_is_a_chip_then_green_red_or_dimmed() {
        let (a, b, c) = (TermId("अ".into()), TermId("आ".into()), TermId("इ".into()));
        assert_eq!(choice_state(&a, &a, None), ChoiceState::Open);
        assert_eq!(
            choice_state(&a, &a, Some(&a)),
            ChoiceState::Right,
            "tapped right: green"
        );
        assert_eq!(
            choice_state(&a, &a, Some(&b)),
            ChoiceState::Right,
            "the answer, not tapped: green"
        );
        assert_eq!(
            choice_state(&b, &a, Some(&b)),
            ChoiceState::Wrong,
            "tapped wrong: red"
        );
        assert_eq!(
            choice_state(&c, &a, Some(&b)),
            ChoiceState::Other,
            "the rest: dimmed"
        );
        assert!(choice_classes(ChoiceState::Right).contains("border-success"));
        assert!(choice_classes(ChoiceState::Wrong).contains("border-danger"));
        assert!(
            choice_classes(ChoiceState::Open).contains("hover:"),
            "only the open chip hovers"
        );
        for state in [ChoiceState::Right, ChoiceState::Wrong, ChoiceState::Other] {
            assert!(!choice_classes(state).contains("hover:"), "{state:?}");
            assert!(
                !choice_classes(state).contains("cursor-pointer"),
                "{state:?}"
            );
        }
    }

    #[test]
    fn the_quiz_strings_are_in_the_three_locales_without_an_em_dash() {
        for language in [Language::En, Language::Fr, Language::Vi] {
            let locale = Locale::from(language);
            for s in [
                td_string!(locale, quiz_right).to_string(),
                td_string!(locale, quiz_wrong).to_string(),
                td_string!(locale, quiz_wrong_read).to_string(),
                td_string!(locale, quiz_next).to_string(),
                td_string!(locale, quiz_choices).to_string(),
            ] {
                assert_ne!(s.len(), 0);
                assert!(!s.contains('\u{2014}'), "{s}");
            }
        }
    }

    /// A run on one started term with `passed` facets already passed.
    pub(crate) fn run_on(term: &str, passed: &[Facet], stage: Stage) -> ReviewRun {
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

    /// `kho` (an indeclinable) asks Meaning, Recall and Context.
    fn kho(asked: Facet) -> ReviewRun {
        let others: Vec<Facet> = [Facet::Meaning, Facet::Recall, Facet::Context]
            .into_iter()
            .filter(|facet| *facet != asked)
            .collect();
        run_on("kho", &others, Stage::Day)
    }

    /// `rūpa 1` (a neuter noun with forms) asks only `asked`.
    fn rupa(asked: Facet, stage: Stage) -> ReviewRun {
        let others: Vec<Facet> = Facet::ALL
            .into_iter()
            .filter(|facet| *facet != asked)
            .collect();
        run_on("rūpa 1", &others, stage)
    }

    pub(crate) fn nothing_due() -> ReviewRun {
        let session = Session::start(deck(), State::empty(), today(), 0).unwrap();
        ReviewRun::new(session, deck(), State::empty())
    }

    fn render_with(language: Language, run: ReviewRun, paused: bool) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            crate::test_support::provide_fakes(Load::Empty);
            let run = RwSignal::new(run);
            let paused = RwSignal::new(paused);
            let none = Callback::new(|()| ());
            view! { <ReviewView run=run paused=paused on_reload=none on_home=none /> }.to_html()
        })
    }

    pub(crate) fn render(run: ReviewRun) -> String {
        render_with(Language::En, run, false)
    }

    /// The card's front alone, told whether the card is revealed.
    fn front_with(language: Language, run: ReviewRun, revealed: bool) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            let run = RwSignal::new(run);
            view! { <CardFront run=run revealed=Signal::stored(revealed) /> }.to_html()
        })
    }

    /// The opening tag of the element right before `needle`.
    fn tag_before<'a>(html: &'a str, needle: &str) -> &'a str {
        let at = html.find(needle).expect(html);
        let start = html[..at].rfind('<').expect(html);
        &html[start..at]
    }

    /// The text of `html`: tags dropped, the few entities Leptos writes
    /// decoded.
    fn text_of(html: &str) -> String {
        let mut text = String::new();
        let mut in_tag = false;
        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => text.push(c),
                _ => {}
            }
        }
        text.replace("&#x27;", "'").replace("&amp;", "&")
    }

    /// The card's front and its reveal: the reveal region is the element
    /// `RevealCard` renders `hidden` with `tabindex="-1"` until the reveal.
    fn split(html: &str) -> (&str, &str) {
        let start = html.find(r#"<div hidden tabindex="-1""#).expect(html);
        (&html[..start], &html[start..])
    }

    /// The opening tag of the first `<tag` whose element text holds `label`.
    fn button_tag<'a>(html: &'a str, label: &str) -> &'a str {
        let at = html.find(label).expect(html);
        let start = html[..at].rfind("<button").expect(html);
        &html[start..html[start..].find('>').expect(html) + start]
    }

    fn after_grade(mut run: ReviewRun, answers: &[Answer]) -> ReviewRun {
        for answer in answers {
            run.grade(*answer, |_| ());
        }
        run
    }

    // ---- T3: the four fronts ----

    #[test]
    fn the_meaning_front_shows_the_lemma_in_pali_and_the_part_of_speech_in_words() {
        for (language, pos, facet) in [
            (Language::En, "indeclinable", "Meaning"),
            (Language::Fr, "indéclinable", "Sens"),
            (Language::Vi, "bất biến từ", "Nghĩa"),
        ] {
            let html = render_with(language, kho(Facet::Meaning), false);
            let (front, _) = split(&html);
            assert!(front.contains(r#"lang="pi""#), "{front}");
            assert!(front.contains(">kho<"), "{front}");
            assert!(text_of(front).contains(pos), "{language:?}: {front}");
            assert!(text_of(front).contains(facet), "{language:?}: {front}");
        }
        let html = render(run_on(
            "sati 1.1",
            &[Facet::Recall, Facet::Forms, Facet::Context],
            Stage::Day,
        ));
        assert!(text_of(split(&html).0).contains("feminine noun"), "{html}");
        let html = render_with(
            Language::Fr,
            run_on(
                "sati 1.1",
                &[Facet::Recall, Facet::Forms, Facet::Context],
                Stage::Day,
            ),
            false,
        );
        assert!(text_of(split(&html).0).contains("nom féminin"), "{html}");
    }

    #[test]
    fn the_recall_front_shows_the_sense_and_the_literal_but_not_the_lemma() {
        let html = render(kho(Facet::Recall));
        let (front, _) = split(&html);
        assert!(
            text_of(front).contains("indeed; surely; certainly; truly"),
            "{front}"
        );
        assert!(
            !text_of(front).contains("kho"),
            "the answer is not asked: {front}"
        );
        let html = render(rupa(Facet::Recall, Stage::Day));
        let front = text_of(split(&html).0);
        assert!(front.contains("matter; material thing"), "{front}");
        assert!(front.contains("[form]"), "the literal in brackets: {front}");
        assert!(front.contains("neuter noun"), "{front}");
        assert!(!front.contains("rūpa"), "{front}");
    }

    #[test]
    fn the_forms_front_asks_the_label_in_words_about_the_lemma() {
        for (language, sentence) in [
            (Language::En, "Give the neuter accusative plural of rūpa."),
            (
                Language::Fr,
                "Quelle est la forme neutre accusatif pluriel de rūpa\u{a0}?",
            ),
            (
                Language::Vi,
                "Dạng trung tính đối cách số nhiều của rūpa là gì?",
            ),
        ] {
            let html = render_with(language, rupa(Facet::Forms, Stage::Quarter), false);
            let (front, _) = split(&html);
            assert!(text_of(front).contains(sentence), "{language:?}: {front}");
            assert!(front.contains(r#"lang="pi""#), "the lemma is Pāli: {front}");
        }
    }

    #[test]
    fn the_lemma_is_bold_on_the_forms_front() {
        let html = render(rupa(Facet::Forms, Stage::Quarter));
        let (front, _) = split(&html);
        assert!(
            front.contains(r#"<strong class="font-semibold"><span lang="pi"#),
            "the lemma must be wrapped in <strong class=\"font-semibold\">: {front}"
        );
    }

    #[test]
    fn the_forms_front_follows_the_stage_of_the_term() {
        let html = render(rupa(Facet::Forms, Stage::Day));
        let front = text_of(split(&html).0);
        assert!(
            front.contains("Give the neuter nominative singular of rūpa."),
            "{front}"
        );
    }

    #[test]
    fn the_context_front_shows_the_line_with_a_blank_and_where_it_comes_from() {
        let html = render(kho(Facet::Context));
        let (front, _) = split(&html);
        assert!(
            front.contains(r#"<span aria-label="blank">___</span>"#),
            "{front}"
        );
        let text = text_of(front);
        assert!(text.contains("aññataro bhikkhu there"), "{text}");
        assert!(text.contains("AN 6.61 · majjhesuttaṃ"), "{text}");
        assert!(text.contains("Context"), "{text}");
        assert!(!front.contains("<mark"), "the answer is not shown: {front}");
        assert!(front.contains(r#"lang="pi""#), "{front}");
    }

    #[test]
    fn the_context_front_links_its_reference_to_suttacentral_in_the_page_language() {
        let en = front_with(Language::En, kho(Facet::Context), false);
        assert!(
            en.contains(r#"href="https://suttacentral.net/an6.61/en/sujato""#),
            "{en}"
        );
        assert!(en.contains(r#"target="_blank""#), "{en}");
        assert!(text_of(&en).contains("AN 6.61 · majjhesuttaṃ"), "{en}");
        let fr = front_with(Language::Fr, kho(Facet::Context), false);
        assert!(
            fr.contains(r#"title="Lire sur SuttaCentral (en anglais)""#),
            "{fr}"
        );
        let vi = front_with(Language::Vi, kho(Facet::Context), false);
        assert!(vi.contains("suttacentral.net/an6.61/vi/minh_chau"), "{vi}");
    }

    #[test]
    fn the_example_of_a_reveal_carries_the_same_link() {
        let html = render(rupa(Facet::Meaning, Stage::Day));
        let (_, reveal) = split(&html);
        assert!(
            reveal.contains(r#"href="https://suttacentral.net/"#),
            "{reveal}"
        );
        assert!(reveal.contains(r#"rel="noopener""#), "{reveal}");
    }

    // ---- T3: the reveal ----

    #[test]
    fn the_reveal_shows_meaning_literal_construction_root_and_example() {
        let html = render(rupa(Facet::Meaning, Stage::Day));
        let (_, reveal) = split(&html);
        let text = text_of(reveal);
        assert!(text.contains("matter; material thing"), "{text}");
        assert!(text.contains("Literally"), "{text}");
        assert!(text.contains("form"), "{text}");
        assert!(text.contains("Construction"), "{text}");
        assert!(text.contains("√rūp + a"), "{text}");
        assert!(text.contains("Root"), "{text}");
        assert!(text.contains("√rūp"), "{text}");
        assert!(text.contains("form, produce"), "{text}");
        assert!(text.contains("Example"), "{text}");
        assert!(text.contains("sutavā ariyasāvako"), "{text}");
        assert!(reveal.contains(r#"lang="pi""#), "{reveal}");
        assert!(
            !reveal.contains("<mark"),
            "only Context marks the answer: {reveal}"
        );
    }

    #[test]
    fn the_recall_reveal_gives_the_lemma_and_the_forms_reveal_the_values() {
        let html = render(kho(Facet::Recall));
        let (_, reveal) = split(&html);
        assert!(reveal.contains(">kho<"), "{reveal}");
        let html = render(rupa(Facet::Forms, Stage::Quarter));
        let (_, reveal) = split(&html);
        let text = text_of(reveal);
        assert!(
            text.contains("rūpāni, rūpe"),
            "every value of the label: {text}"
        );
    }

    #[test]
    fn the_context_front_before_the_reveal_holds_its_blanks_and_not_the_answer() {
        let front = front_with(Language::En, kho(Facet::Context), false);
        assert!(
            front.contains(r#"<span aria-label="blank">___</span>"#),
            "{front}"
        );
        assert!(!front.contains("<mark"), "{front}");
        assert!(!text_of(&front).contains("kho"), "{front}");
        assert!(
            text_of(&front).contains(" · Bhikkhu Sujato"),
            "translation with credit appears on front: {front}"
        );
    }

    #[test]
    fn the_context_front_fills_its_blanks_in_place_once_revealed() {
        let front = front_with(Language::En, kho(Facet::Context), true);
        assert!(
            front.contains(r#"<mark class="rounded-sm bg-gold/20 px-0.5 text-ink">kho</mark>"#),
            "{front}"
        );
        assert!(!front.contains("___"), "{front}");
        let text = text_of(&front);
        assert!(text.contains("phasso kho, āvuso"), "{text}");
        assert!(text.contains("AN 6.61 · majjhesuttaṃ"), "{text}");
    }

    #[test]
    fn the_context_reveal_gives_the_answer_block_and_does_not_repeat_the_line() {
        let html = render(kho(Facet::Context));
        let (_, reveal) = split(&html);
        let text = text_of(reveal);
        assert!(!text.contains("Example"), "{text}");
        assert!(!reveal.contains("<mark"), "{reveal}");
        assert!(!text.contains("phasso"), "the line is not repeated: {text}");
        assert!(
            reveal.contains("border-t border-line pt-4 mt-4"),
            "{reveal}"
        );
        assert!(
            reveal.contains(r#"<strong class="font-semibold"><span lang="pi""#),
            "the lemma in bold Pāli: {reveal}"
        );
        assert!(text.contains("kho · indeclinable"), "{text}");
        assert!(text.contains("Meaning"), "{text}");
        let sense = tag_before(reveal, "indeed; surely; certainly; truly");
        assert!(sense.contains("text-lg text-heading"), "{sense}");
    }

    #[test]
    fn the_context_reveal_shows_the_literal_once_muted_under_the_sense() {
        let html = render(rupa(Facet::Context, Stage::Day));
        let (_, reveal) = split(&html);
        let literal = tag_before(reveal, "[form]");
        assert!(literal.contains("text-muted"), "{literal}");
        let text = text_of(reveal);
        assert!(!text.contains("Literally"), "{text}");
        assert!(!text.contains("Example"), "{text}");
        assert!(text.contains("Construction"), "{text}");
        assert!(
            text.find("matter; material thing").unwrap() < text.find("[form]").unwrap(),
            "{text}"
        );
    }

    #[test]
    fn the_context_reveal_labels_the_meaning_in_every_language() {
        for (language, label, pos) in [
            (Language::En, "Meaning", "indeclinable"),
            (Language::Fr, "Sens", "indéclinable"),
            (Language::Vi, "Nghĩa", "bất biến từ"),
        ] {
            let html = render_with(language, kho(Facet::Context), false);
            let (_, reveal) = split(&html);
            let text = text_of(reveal);
            assert!(text.contains(label), "{language:?}: {text}");
            assert!(
                text.contains(&format!("kho · {pos}")),
                "{language:?}: {text}"
            );
        }
    }

    #[test]
    fn a_missing_part_of_the_term_is_left_out() {
        let html = render(kho(Facet::Meaning));
        let (_, reveal) = split(&html);
        let text = text_of(reveal);
        assert!(!text.contains("Literally"), "{text}");
        assert!(!text.contains("Construction"), "{text}");
        assert!(!text.contains("Root"), "{text}");
        assert!(text.contains("Example"), "{text}");
    }

    // ---- 025: the example's translation ----

    /// `yo` (the Vakkali example, translated in English and French) asks
    /// only `asked`.
    fn yo(asked: Facet) -> ReviewRun {
        let others: Vec<Facet> = Facet::ALL
            .into_iter()
            .filter(|facet| *facet != asked)
            .collect();
        run_on("yo", &others, Stage::Day)
    }

    const EN_LINE: &str = "Enough, Vakkali! Why would you want to see this rotting body?";

    #[test]
    fn the_meaning_reveal_gives_the_translation_under_the_example() {
        for (language, line, name) in [
            (Language::En, EN_LINE, "Bhikkhu Sujato"),
            (Language::Fr, "Assez, Vakkali", "Christelle"),
        ] {
            let html = render_with(language, yo(Facet::Meaning), false);
            let (_, reveal) = split(&html);
            let tag = tag_before(reveal, line);
            assert!(
                tag.starts_with(r#"<p class="mt-2 text-sm italic text-muted">"#),
                "{language:?}: {tag}"
            );
            let credit = format!(r#"<span class="not-italic"> · {name}</span>"#);
            assert!(reveal.contains(&credit), "{language:?}: {reveal}");
            let paragraph = &reveal[reveal.find(line).unwrap()..];
            let paragraph = &paragraph[..paragraph.find("</p>").unwrap()];
            assert!(!paragraph.contains('—'), "no em dash: {paragraph}");
            let text = text_of(reveal);
            let pali = text.find("dhammaṃ passati").expect(&text);
            assert!(pali < text.find(line).expect(&text), "{text}");
        }
    }

    #[test]
    fn a_language_without_its_translation_gets_the_english_one() {
        let html = render_with(Language::Vi, yo(Facet::Meaning), false);
        let (_, reveal) = split(&html);
        let text = text_of(reveal);
        assert!(text.contains(EN_LINE), "{text}");
        assert!(text.contains(" · Bhikkhu Sujato"), "{text}");
        assert!(!text.contains("— Bhikkhu Sujato"), "{text}");
    }

    #[test]
    fn the_context_front_gives_the_translation_under_the_line_even_before_revealed() {
        let hidden = front_with(Language::En, yo(Facet::Context), false);
        assert!(
            text_of(&hidden).contains("Enough"),
            "translation appears before reveal: {hidden}"
        );
        let shown = front_with(Language::En, yo(Facet::Context), true);
        let text = text_of(&shown);
        let line = text.find("passati").expect(&text);
        let translation = text.find(EN_LINE).expect(&text);
        let origin = text.find("SN 22.87").expect(&text);
        assert!(line < translation && translation < origin, "{text}");
        assert!(text.contains(" · Bhikkhu Sujato"), "{text}");
        assert!(!text.contains("— Bhikkhu Sujato"), "{text}");
        let html = render(yo(Facet::Context));
        let (_, reveal) = split(&html);
        let reveal_text = text_of(reveal);
        // Translation should appear exactly once in total (on front, not repeated on reveal)
        let count = reveal_text.matches("Enough").count();
        assert_eq!(count, 0, "translation not repeated on reveal: {reveal}");
    }

    #[test]
    fn an_example_without_a_translation_shows_none() {
        let mut example = deck()
            .term(&TermId("yo".to_string()))
            .and_then(|term| term.example.clone())
            .unwrap();
        example.translations.clear();
        for language in Language::ALL {
            assert!(translation_view(&example, language).is_none());
        }
    }

    // ---- T3: practice ----

    #[test]
    fn a_practice_question_shows_its_badge_and_its_note() {
        let failed = after_grade(kho(Facet::Meaning), &[Answer::Fail]);
        let html = render(failed);
        let (front, reveal) = split(&html);
        assert!(
            text_of(front).contains("Practice · to see again"),
            "{front}"
        );
        assert!(
            text_of(reveal).contains("This answer won't change your stage."),
            "{reveal}"
        );
        let plain = render(kho(Facet::Meaning));
        assert!(!plain.contains("Practice"), "{plain}");
        assert!(!plain.contains("change your stage"), "{plain}");
    }

    #[test]
    fn the_practice_badge_comes_before_the_facet_label() {
        let html = render(after_grade(kho(Facet::Meaning), &[Answer::Fail]));
        let text = text_of(split(&html).0);
        let badge = text.find("Practice").expect(&text);
        let label = text.find("Meaning").expect(&text);
        assert!(badge < label, "{text}");
    }

    // ---- T3: header, grades, stop ----

    #[test]
    fn the_header_counts_done_over_total_in_the_unit_of_the_language() {
        let run = run_on("kho", &[], Stage::Day);
        assert!(text_of(&render(run)).contains("0 / 3 cards"));
        let run = after_grade(run_on("kho", &[], Stage::Day), &[Answer::Pass]);
        assert!(text_of(&render(run)).contains("1 / 3 cards"));
        let html = render_with(Language::Fr, run_on("kho", &[], Stage::Day), false);
        assert!(text_of(&html).contains("0 / 3 cartes"), "{html}");
        let html = render_with(Language::Vi, run_on("kho", &[], Stage::Day), false);
        assert!(text_of(&html).contains("0 / 3 thẻ"), "{html}");
    }

    #[test]
    fn the_total_follows_a_fail_that_drops_the_other_cards() {
        let run = after_grade(run_on("kho", &[], Stage::Day), &[Answer::Fail]);
        // 1 done + 0 due + 1 practice pending.
        assert!(text_of(&render(run)).contains("1 / 2 cards"));
    }

    #[test]
    fn the_grade_buttons_are_disabled_before_the_reveal_didnt_first() {
        let html = render(kho(Facet::Meaning));
        let didnt = button_tag(&html, "Didn't know");
        let knew = button_tag(&html, "Knew it");
        assert!(didnt.contains("disabled"), "{didnt}");
        assert!(knew.contains("disabled"), "{knew}");
        assert!(html.find("Didn't know") < html.find("Knew it"));
        assert!(html.contains("Show the answer"), "{html}");
    }

    #[test]
    fn the_stop_dialog_is_in_the_markup_with_its_two_buttons() {
        let html = render(kho(Facet::Meaning));
        assert!(html.contains(r#"id="learn-review-stop""#), "{html}");
        assert!(html.contains(r#"role="dialog""#), "{html}");
        let text = text_of(&html);
        assert!(text.contains("Stop here?"), "{text}");
        assert!(text.contains("Your 0 answers are kept."), "{text}");
        assert!(text.contains("Keep going"), "{text}");
        let at = html.find(">Stop<").expect(&html);
        let stop = &html[html[..at].rfind('<').expect(&html)..=at];
        assert!(stop.starts_with("<button"), "Stop is a button: {stop}");
        assert!(html.contains(r#"aria-label="Stop the session""#), "{html}");
    }

    /// 'Stop here?' is a popup, a small centred card on the phone as well
    /// (ui-lib `DialogSize::Popup`), never the full-screen sheet.
    #[test]
    fn the_stop_dialog_is_a_popup_not_a_sheet() {
        let html = render(kho(Facet::Meaning));
        let panel = crate::test_support::tag_around(&html, r#"id="learn-review-stop""#);
        assert!(panel.contains(r#"role="dialog""#), "{panel}");
        let classes: Vec<&str> = panel
            .split(r#"class=""#)
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect(panel)
            .split_whitespace()
            .collect();
        assert!(classes.contains(&"max-w-md"), "{panel}");
        assert!(!classes.contains(&"h-full"), "{panel}");
        assert!(!classes.contains(&"sm:max-w-md"), "{panel}");
        let widths = classes.iter().filter(|c| c.contains("max-w-")).count();
        assert_eq!(widths, 1, "exactly one max-w- utility: {panel}");
    }

    #[test]
    fn the_stop_dialog_says_how_many_answers_are_kept() {
        let one = render(after_grade(run_on("kho", &[], Stage::Day), &[Answer::Pass]));
        assert!(text_of(&one).contains("Your 1 answer is kept."), "{one}");
        let two = render(after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Pass, Answer::Pass],
        ));
        assert!(text_of(&two).contains("Your 2 answers are kept."), "{two}");
    }

    // ---- T3: pause ----

    #[test]
    fn a_pause_shows_the_banner_and_the_reload_button() {
        let html = render_with(Language::En, kho(Facet::Meaning), true);
        let text = text_of(&html);
        assert!(text.contains("Progress changed in another tab"), "{text}");
        assert!(text.contains("Reload the session"), "{text}");
        assert!(html.contains("inert"), "the card is inert: {html}");
        let html = render(kho(Facet::Meaning));
        assert!(!text_of(&html).contains("Progress changed"), "{html}");
        assert!(!text_of(&html).contains("Reload the session"), "{html}");
        assert!(!html.contains("inert"), "{html}");
    }

    // ---- T3: the summary ----

    #[test]
    fn the_summary_counts_terms_and_gives_the_next_due_date() {
        let run = after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Pass, Answer::Pass, Answer::Pass],
        );
        let html = render(run);
        let text = text_of(&html);
        assert!(text.contains("Session done"), "{text}");
        assert!(text.contains("1 term moved up"), "{text}");
        assert!(text.contains("0 start again tomorrow"), "{text}");
        assert!(text.contains("Next due 12 October 2026"), "{text}");
        assert!(text.contains("Back home"), "{text}");
        assert!(
            html.contains("data-new-terms"),
            "'Add new terms' is offered: {html}"
        );
        assert!(
            !html.contains(r#"role="dialog""#),
            "no stop dialog once over: {html}"
        );
        assert!(!text.contains("cards"), "no counter once over: {text}");
    }

    #[test]
    fn the_summary_uses_the_singular_and_the_plural_forms() {
        let run = after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Fail, Answer::Pass],
        );
        let text = text_of(&render(run));
        assert!(text.contains("0 terms moved up"), "{text}");
        assert!(text.contains("1 starts again tomorrow"), "{text}");
        assert!(text.contains("Next due 6 October 2026"), "{text}");
        let html = render_with(
            Language::Fr,
            after_grade(
                run_on("kho", &[], Stage::Day),
                &[Answer::Pass, Answer::Pass, Answer::Pass],
            ),
            false,
        );
        let text = text_of(&html);
        assert!(text.contains("Séance terminée"), "{text}");
        assert!(text.contains("1 terme a avancé"), "{text}");
        assert!(
            text.contains("0 recommence demain"),
            "fr: zero is singular: {text}"
        );
        assert!(
            text.contains("Prochaine révision le 12 octobre 2026"),
            "{text}"
        );
    }

    #[test]
    fn a_session_with_two_terms_moved_says_terms() {
        let mut session = Session::start(deck(), State::empty(), today(), 0).unwrap();
        session.request_new(2);
        let mut run = ReviewRun::new(session, deck(), State::empty());
        for _ in 0..6 {
            run.grade(Answer::Pass, |_| ());
        }
        let text = text_of(&render(run));
        assert!(text.contains("2 terms moved up"), "{text}");
    }

    #[test]
    fn nothing_due_says_so_with_back_home_and_new_terms_and_no_summary() {
        let html = render(nothing_due());
        let text = text_of(&html);
        assert!(text.contains("Nothing due today"), "{text}");
        assert!(text.contains("Back home"), "{text}");
        assert!(html.contains("data-new-terms"), "{html}");
        assert!(!text.contains("Session done"), "{text}");
        assert!(!text.contains("moved up"), "{text}");
        assert!(!text.contains("start again"), "{text}");
        assert!(!text.contains("Next due"), "{text}");
        let back = button_tag(&html, "Back home");
        assert!(back.starts_with("<button"), "{back}");
    }

    // ---- 058: the end of a session as a centred card ----

    /// The number of buttons in `html` whose class list holds the token.
    fn buttons_with(html: &str, token: &str) -> usize {
        html.split("<button")
            .skip(1)
            .filter(|chunk| {
                chunk[..chunk.find('>').expect(html)]
                    .split(['"', ' '])
                    .any(|class| class == token)
            })
            .count()
    }

    /// The opening tag of the done card.
    fn card_tag(html: &str) -> &str {
        let at = html.find(r#"data-session-done=""#).expect(html);
        let start = html[..at].rfind("<div").expect(html);
        &html[start..html[start..].find('>').expect(html) + start]
    }

    fn session_done() -> String {
        render(after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Pass, Answer::Pass, Answer::Pass],
        ))
    }

    #[test]
    fn the_end_of_a_session_is_one_centred_card() {
        for html in [session_done(), render(nothing_due())] {
            assert_eq!(html.matches(r#"data-session-done=""#).count(), 1, "{html}");
            let card = card_tag(&html);
            for class in [
                "mx-auto",
                "max-w-md",
                "w-full",
                "text-center",
                "rounded-xl",
                "border",
                "border-line",
                "bg-card",
            ] {
                assert!(
                    card.split(['"', ' ']).any(|c| c == class),
                    "{class}: {card}"
                );
            }
            assert!(
                !html.contains("items-start"),
                "nothing left-aligned: {html}"
            );
        }
    }

    /// The opening tag of the first `<div` before `marker` that holds it.
    fn div_with<'a>(html: &'a str, marker: &str) -> &'a str {
        let at = html.find(marker).expect(html);
        let start = html[..at].rfind("<div").expect(html);
        &html[start..html[start..].find('>').expect(html) + start]
    }

    /// 058 (user check, phone): the card sits in the middle of the height
    /// under the brand: the review column grows, the done area takes what
    /// is left and centres the card, with no fixed height.
    #[test]
    fn the_done_card_is_centred_in_the_height_under_the_brand() {
        let mut practice = practice_run(deck(), ahead_state(), today(), 1, None, 0).expect("kho");
        drain(&mut practice, Answer::Fail);
        for html in [session_done(), render(nothing_due()), render(practice)] {
            let area = div_with(&html, "data-session-done-area");
            for class in ["flex", "flex-1", "items-center", "justify-center"] {
                assert!(
                    area.split(['"', ' ']).any(|c| c == class),
                    "{class}: {area}"
                );
            }
            let column = div_with(&html, "data-review");
            for class in ["flex", "flex-1", "flex-col"] {
                assert!(
                    column.split(['"', ' ']).any(|c| c == class),
                    "{class}: {column}"
                );
            }
            for tag in [area, column] {
                assert!(!tag.contains("min-h-"), "no fixed height: {tag}");
                assert!(!tag.contains("h-["), "no fixed height: {tag}");
            }
            let at = |needle: &str| html.find(needle).expect(&html);
            assert!(
                at("data-session-done-area") < at("data-session-done=\"\""),
                "{html}"
            );
        }
    }

    #[test]
    fn the_counts_show_their_number_in_an_emphasised_element() {
        let html = render(after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Fail, Answer::Pass],
        ));
        for (number, words) in [("0", " terms moved up"), ("1", " starts again tomorrow")] {
            let wanted = format!("{number}</span>{words}");
            let at = html
                .find(&wanted)
                .unwrap_or_else(|| panic!("{wanted}: {html}"));
            let start = html[..at].rfind("<span").expect(&html);
            let tag = &html[start..at];
            assert!(tag.contains("data-count"), "{tag}");
            for class in ["font-serif", "text-2xl", "text-heading"] {
                assert!(tag.split(['"', ' ']).any(|c| c == class), "{class}: {tag}");
            }
        }
        assert!(text_of(&html).contains("Next due 6 October 2026"), "{html}");
    }

    #[test]
    fn back_home_is_the_only_primary_button_and_add_is_secondary() {
        for html in [session_done(), render(nothing_due())] {
            assert_eq!(buttons_with(&html, "bg-primary"), 1, "{html}");
            let back = button_tag(&html, "Back home");
            assert!(back.split(['"', ' ']).any(|c| c == "bg-primary"), "{back}");
            let add = button_tag(&html, ">Add<");
            assert!(
                add.split(['"', ' ']).any(|c| c == "border-gold/40"),
                "{add}"
            );
            assert!(!add.split(['"', ' ']).any(|c| c == "bg-primary"), "{add}");
        }
    }

    #[test]
    fn learn_more_today_titles_the_new_terms_section() {
        for (language, said) in [
            (Language::En, "Learn more today"),
            (Language::Fr, "Apprendre davantage aujourd’hui"),
            (Language::Vi, "Học thêm hôm nay"),
        ] {
            let html = render_with(language, nothing_due(), false);
            assert!(text_of(&html).contains(said), "{language:?}: {html}");
            assert!(html.find(said) < html.find("data-new-terms"), "{html}");
        }
    }

    #[test]
    fn a_practice_done_uses_the_same_card_with_no_new_terms() {
        let mut run = practice_run(deck(), ahead_state(), today(), 1, None, 0).expect("kho");
        drain(&mut run, Answer::Fail);
        let html = render(run);
        assert_eq!(html.matches(r#"data-session-done=""#).count(), 1, "{html}");
        assert!(card_tag(&html).contains("mx-auto"), "{html}");
        assert_eq!(buttons_with(&html, "bg-primary"), 1, "{html}");
        assert!(!html.contains("data-new-terms"), "{html}");
        assert!(!text_of(&html).contains("Learn more today"), "{html}");
    }

    /// The session runs in place on the home: every way back (Stop, Back
    /// home on the summary, on nothing due, on a broken progress) is a
    /// button that switches the home back, never a link to `/`.
    #[test]
    fn the_ways_home_are_buttons_not_links() {
        let done = after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Pass, Answer::Pass, Answer::Pass],
        );
        for html in [
            render(kho(Facet::Meaning)),
            render(done),
            render(nothing_due()),
            broken(BrokenKind::Invalid),
        ] {
            assert!(!html.contains(r#"href="/""#), "{html}");
            assert!(!html.contains(r#"href="/review""#), "{html}");
        }
    }

    /// [`SessionView`] on a progress that cannot be used.
    fn broken(kind: BrokenKind) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            crate::test_support::provide_fakes(Load::Empty);
            let none = Callback::new(|()| ());
            let paused = Signal::stored(false);
            view! { <SessionView phase=Phase::Broken(kind) paused=paused on_reload=none on_home=none /> }
            .to_html()
        })
    }

    /// A store that could not read starts no session and writes nothing:
    /// the phase is Broken, and the store saw no save.
    #[test]
    fn an_unreadable_store_starts_no_session_and_saves_nothing() {
        Owner::new().with(|| {
            let (store, _files) =
                crate::test_support::provide_fakes(Load::Unreadable("denied".into()));
            let backend = Backend::current();
            let owner = Owner::current().expect("an owner");
            assert!(matches!(
                start_phase(&owner, backend),
                Phase::Broken(BrokenKind::Unreadable)
            ));
            assert!(matches!(
                practice_phase(&owner, backend, 5, None),
                Some(Phase::Broken(BrokenKind::Unreadable))
            ));
            assert_eq!(store.saved().len(), 0);
        });
    }

    /// The store's guard is the contract: neither a save nor the exit
    /// write reaches the file while its load answered Unreadable.
    #[test]
    fn the_stores_guard_keeps_the_file_untouched_while_unreadable() {
        Owner::new().with(|| {
            let (store, _files) =
                crate::test_support::provide_fakes(Load::Unreadable("denied".into()));
            let backend = Backend::current();
            backend.save(&State::empty());
            store.exit_write();
            assert_eq!(store.written().len(), 0);
            assert_eq!(backend.load(), Load::Unreadable("denied".into()));
        });
    }

    #[test]
    fn a_progress_that_cannot_be_used_says_why_with_a_way_home() {
        for (kind, said) in [
            (BrokenKind::Invalid, "could not be read: it is damaged"),
            (BrokenKind::NewerVersion, "saved by a newer version"),
            (
                BrokenKind::Unreadable,
                "could not be read. Nothing was changed",
            ),
        ] {
            let html = broken(kind);
            assert!(html.contains(said), "{html}");
            let back = button_tag(&html, "Back home");
            assert!(back.starts_with("<button"), "{back}");
            assert!(!html.contains("data-review"), "{html}");
        }
    }

    /// HL: the home holds the one centred column; the session sets none.
    #[test]
    fn the_session_sets_no_column_of_its_own() {
        let html = render(nothing_due());
        assert_eq!(html.matches("sm:max-w-[40rem]").count(), 0, "{html}");
    }

    /// HL: the summary's headings use the quieter scale, `text-xl`.
    #[test]
    fn the_summary_headings_use_the_quieter_scale() {
        let done = render(after_grade(
            run_on("kho", &[], Stage::Day),
            &[Answer::Pass, Answer::Pass, Answer::Pass],
        ));
        for (html, heading) in [
            (done, "Session done"),
            (render(nothing_due()), "Nothing due today"),
        ] {
            let at = html.find(heading).expect(&html);
            let start = html[..at].rfind("<h2").expect(&html);
            let tag = &html[start..at];
            assert!(tag.split(['"', ' ']).any(|c| c == "text-xl"), "{tag}");
            assert!(!tag.contains("text-2xl"), "{tag}");
        }
    }

    #[test]
    fn nothing_due_is_said_in_every_language() {
        for (language, said) in [
            (Language::Fr, "Rien à revoir aujourd’hui"),
            (Language::Vi, "Hôm nay không có gì cần ôn"),
        ] {
            let text = text_of(&render_with(language, nothing_due(), false));
            assert!(text.contains(said), "{language:?}: {text}");
        }
    }

    // ---- practice ahead ----

    /// Three started terms, nothing due: `kho` and `evaṃ 1` in Week,
    /// `hoti 1` in Month; `last_today` already today.
    fn ahead_state() -> State {
        let mut state = State::empty();
        for (term, stage, days) in [
            ("hoti 1", Stage::Month, 4),
            ("evaṃ 1", Stage::Week, 6),
            ("kho", Stage::Week, 3),
        ] {
            let mut entry = Progress::new(TermId(term.to_string()), today() + days);
            entry.stage = stage;
            state.progress.push(entry);
        }
        state.last_today = Some(today());
        state
    }

    /// Grades every question with `answer`: what was asked, `(term,
    /// practice)`, and every state saved.
    fn drain(run: &mut ReviewRun, answer: Answer) -> (Vec<(String, bool)>, Vec<State>) {
        let (mut asked, mut saved) = (Vec::new(), Vec::new());
        while let Some(question) = run.current().cloned() {
            asked.push((question.term.0.clone(), question.practice));
            run.grade(answer, |state| saved.push(state.clone()));
        }
        (asked, saved)
    }

    /// The terms a session asked, sorted and once each: the order of the
    /// cards is the seed's, the terms are the practice's.
    fn terms_of(asked: &[(String, bool)]) -> Vec<&str> {
        let mut terms: Vec<&str> = asked.iter().map(|(term, _)| term.as_str()).collect();
        terms.sort_unstable();
        terms.dedup();
        terms
    }

    #[test]
    fn practice_ahead_asks_the_nearest_terms_of_the_stage_all_as_practice() {
        let state = ahead_state();
        for answer in [Answer::Pass, Answer::Fail] {
            let mut run = practice_run(deck(), state.clone(), today(), 5, Some(Stage::Week), 0)
                .expect("a run");
            let (asked, saved) = drain(&mut run, answer);
            assert_eq!(terms_of(&asked), ["evaṃ 1", "kho"], "Week only");
            assert!(asked.iter().all(|(_, practice)| *practice), "{asked:?}");
            assert!(
                saved.iter().all(|saved| *saved == state),
                "{answer:?}: the progress never changes"
            );
        }
    }

    #[test]
    fn practice_ahead_takes_the_batch_and_every_stage_when_none_is_selected() {
        let mut run = practice_run(deck(), ahead_state(), today(), 2, None, 0).expect("a run");
        let (asked, _) = drain(&mut run, Answer::Pass);
        assert_eq!(terms_of(&asked), ["hoti 1", "kho"]);
    }

    /// The seed sets the order, and only the order: every seed asks the
    /// same cards, two seeds do not ask them in the same sequence.
    #[test]
    fn the_seed_orders_a_practice_ahead_and_nothing_else() {
        let ask = |seed| {
            let mut run =
                practice_run(deck(), ahead_state(), today(), 5, None, seed).expect("a run");
            drain(&mut run, Answer::Pass).0
        };
        let mut base = ask(0);
        assert!((1..20).any(|seed| ask(seed) != base), "some seed reorders");
        assert_eq!(ask(3), ask(3), "the same seed, the same order");
        base.sort();
        for seed in 1..20 {
            let mut asked = ask(seed);
            asked.sort();
            assert_eq!(asked, base, "{seed}");
        }
    }

    #[test]
    fn practice_ahead_with_nothing_to_ask_starts_nothing() {
        assert!(practice_run(deck(), ahead_state(), today(), 5, Some(Stage::Year), 0).is_none());
        assert!(practice_run(deck(), State::empty(), today(), 5, None, 0).is_none());
        assert!(practice_run(deck(), ahead_state(), today(), 0, None, 0).is_none());
    }

    /// The header counts every question of the practice (`kho`: Meaning,
    /// Recall, Context).
    #[test]
    fn the_practice_header_counts_every_question() {
        let run = practice_run(deck(), ahead_state(), today(), 1, None, 0).expect("kho");
        assert!(run.is_practice());
        let text = text_of(&render(run));
        assert!(text.contains("0 / 3 cards"), "{text}");
    }

    /// The end of a practice: 'Practice done' and 'Back home'; nothing
    /// moved, so no 'moved up', no 'start again', no next date, no new
    /// terms.
    #[test]
    fn a_practice_ends_with_practice_done_and_back_home_only() {
        for (language, done) in [
            (Language::En, "Practice done"),
            (Language::Fr, "Entraînement terminé"),
            (Language::Vi, "Đã luyện xong"),
        ] {
            let mut run = practice_run(deck(), ahead_state(), today(), 1, None, 0).expect("kho");
            drain(&mut run, Answer::Fail);
            let html = render_with(language, run, false);
            let text = text_of(&html);
            assert!(text.contains(done), "{language:?}: {text}");
            let heading = &html[html[..html.find(done).expect(&html)]
                .rfind('<')
                .expect(&html)..];
            assert!(heading.starts_with("<h2"), "{heading}");
            let back = td_string!(Locale::from(language), back_home);
            assert!(button_tag(&html, back).starts_with("<button"), "{html}");
            for absent in [
                td_string!(Locale::from(language), summary_title),
                td_string!(Locale::from(language), summary_nothing_due),
            ] {
                assert!(!text.contains(absent), "{absent}: {text}");
            }
            assert!(!html.contains("<li"), "no summary lines: {html}");
            assert!(!html.contains("data-new-terms"), "{html}");
        }
    }

    // ---- 026: tap a word of the example ----

    /// The opening tags of the word buttons whose panel id starts with `id`.
    fn word_buttons<'a>(html: &'a str, id: &str) -> Vec<&'a str> {
        let controls = format!(r#"aria-controls="{id}"#);
        html.match_indices("<button")
            .map(|(at, _)| &html[at..=at + html[at..].find('>').expect(html)])
            .filter(|tag| tag.contains(&controls))
            .collect()
    }

    #[test]
    fn the_meaning_reveal_makes_each_example_word_a_closed_disclosure() {
        let html = render(yo(Facet::Meaning));
        let (front, reveal) = split(&html);
        let tags = word_buttons(reveal, EXAMPLE_WORDS);
        // alaṃ vakkali kiṃ te iminā pūtikāyena diṭṭhena yo kho vakkali
        // dhammaṃ passati so maṃ passati yo maṃ passati so dhammaṃ passati
        assert_eq!(tags.len(), 21, "{reveal}");
        for tag in &tags {
            assert!(tag.contains(r#"aria-expanded="false""#), "{tag}");
        }
        for word in ["alaṃ", "vakkali", "pūtikāyena", "passati"] {
            assert!(
                reveal.contains(&format!(">{word}</button>")),
                "{word}: {reveal}"
            );
        }
        for mark in [",</button>", "?</button>", ", </button>"] {
            assert!(!reveal.contains(mark), "punctuation stays plain: {reveal}");
        }
        assert!(
            text_of(reveal).contains("alaṃ, vakkali, kiṃ te"),
            "{reveal}"
        );
        assert_eq!(word_buttons(front, "pc-").len(), 0, "{front}");
    }

    #[test]
    fn the_recall_reveal_makes_the_example_words_buttons_too() {
        let html = render(kho(Facet::Recall));
        let (_, reveal) = split(&html);
        assert!(reveal.contains(">phasso</button>"), "{reveal}");
    }

    #[test]
    fn a_live_sense_shows_its_lemma_pos_and_the_pages_meaning() {
        let sense = crate::dictionary::Sense {
            lemma: "dhamma 1.01".into(),
            pos: "masc".into(),
            en: "nature; character".into(),
            fr: Some("nature".into()),
        };
        let en = live_entry(&sense, Locale::from(Language::En));
        assert_eq!(en.headword, "dhamma 1.01");
        assert_eq!(en.tag.as_deref(), Some("masc"));
        assert_eq!(en.meaning, "nature; character");
        assert_eq!(
            live_entry(&sense, Locale::from(Language::Fr)).meaning,
            "nature"
        );
        let no_fr = crate::dictionary::Sense { fr: None, ..sense };
        assert_eq!(
            live_entry(&no_fr, Locale::from(Language::Fr)).meaning,
            "nature; character",
            "English when the DPD has no French"
        );
        let vietnamese = Locale::from(Language::Vi);
        assert_eq!(live_entry(&no_fr, vietnamese).meaning, "nature; character");
    }

    #[test]
    fn the_lookup_label_follows_the_state_in_every_language() {
        for language in [Language::En, Language::Fr, Language::Vi] {
            let locale = Locale::from(language);
            let pending = lookup_label(locale, LookupState::Pending);
            let none = lookup_label(locale, LookupState::None);
            let error = lookup_label(locale, LookupState::Unreachable);
            let unavailable = lookup_label(locale, LookupState::Unavailable);
            assert!(!pending.is_empty() && !none.is_empty() && !error.is_empty());
            assert_ne!(unavailable.len(), 0);
            assert_ne!(pending, none);
            assert!(!pending.contains('\u{2014}') && !none.contains('\u{2014}'));
        }
        assert_eq!(
            lookup_label(Locale::from(Language::En), LookupState::None),
            "No entry for this word."
        );
    }

    #[test]
    fn the_context_front_has_no_word_button_until_revealed_and_never_on_the_answer() {
        let hidden = front_with(Language::En, kho(Facet::Context), false);
        assert_eq!(word_buttons(&hidden, "pc-").len(), 0, "{hidden}");
        let shown = front_with(Language::En, kho(Facet::Context), true);
        assert!(shown.contains(">phasso</button>"), "{shown}");
        assert!(shown.contains(">āvuso</button>"), "{shown}");
        assert!(
            shown.contains(r#"<mark class="rounded-sm bg-gold/20 px-0.5 text-ink">kho</mark>"#),
            "{shown}"
        );
        assert!(!shown.contains(">kho</button>"), "{shown}");
        let before = word_buttons(&shown, "pc-line-words-0").len();
        let after = word_buttons(&shown, "pc-line-words-1").len();
        assert!(before > 0 && after > 0, "{shown}");
    }

    /// 065: every word is a button; a word the deck does not know is looked
    /// up live when opened (its panel says so), a variant spelling is found
    /// in the deck's entries.
    #[test]
    fn a_word_the_deck_does_not_know_is_a_button_too_and_a_variant_spelling_is_found() {
        let html = Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            let text = "Alaṁ, zzzq sampādethā’ti.".to_string();
            view! { {example_words("pc-test", text, Locale::from(Language::En), None)} }.to_html()
        });
        assert!(html.contains(">Alaṁ</button>"), "{html}");
        assert!(html.contains(">sampādethā’ti</button>"), "{html}");
        assert!(
            html.contains(">zzzq</button>"),
            "tappable, looked up live: {html}"
        );
    }

    #[test]
    fn an_entry_is_tagged_with_its_part_of_speech_then_its_grammar() {
        let with = learn_core::words::WordEntry {
            headword: "mākāsi".to_string(),
            pos: "aor".to_string(),
            grammar: Some("aor 3rd sg".to_string()),
            meaning: "do not make".to_string(),
        };
        let shown = ui_entry(&with);
        assert_eq!(shown.tag.as_deref(), Some("aor · aor 3rd sg"));
        assert_eq!(
            (shown.headword.as_str(), shown.meaning.as_str()),
            ("mākāsi", "do not make")
        );
        let without = learn_core::words::WordEntry {
            grammar: None,
            ..with
        };
        assert_eq!(ui_entry(&without).tag.as_deref(), Some("aor"));
    }

    #[test]
    fn the_more_line_is_in_the_pages_language() {
        for (language, line) in [
            (Language::En, "+ 2 more"),
            (Language::Fr, "+ 2 de plus"),
            (Language::Vi, "+ 2 nữa"),
        ] {
            assert_eq!(more_words(Locale::from(language), 2), line, "{language:?}");
        }
    }
}
