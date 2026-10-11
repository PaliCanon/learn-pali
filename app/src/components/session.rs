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

use learn_core::date::Date;
use learn_core::deck::Blank;
use learn_core::facet::Facet;
use learn_core::progress::State;
use learn_core::session::{Answer, Session, nearest};
use learn_core::stage::Stage;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos_i18n::{td, td_string};
use palicanon_ui_lib::Language;
use palicanon_ui_lib::{
    Button, Dialog, Disclosed, Disclosure, Pali, PaliSize, RevealCard, SessionHeader, Size,
    Variant, language_signal,
};

use crate::catalogue::Locale;
use crate::components::new_terms::NewTerms;
use crate::deck::deck;
use crate::learn::home_state::BrokenKind;
use crate::learn::key::{GradeGate, grade_key};
use crate::learn::run::{Card, ReviewRun};
use crate::learn::summary::Summary;
use crate::learn::window::{MAX_CHARS, Segment, context_window, source_ref};
use crate::learn::words::{date_words, label_words, pos_words};
use crate::seed::browser_seed;
use crate::store::{Load, load, save};
use crate::today::{browser_today, session_today};

/// The card's box: a definite height (the `RevealCard` contract, so its
/// body scrolls under the pinned grades), capped from `sm` up.
pub(crate) const CARD_BOX: &str =
    "h-[calc(100dvh-14rem)] min-h-72 w-full rounded-xl border border-line bg-card sm:h-[34rem]";
/// The small caption above a front (the facet's name).
const CAPTION: &str = "text-xs font-medium uppercase tracking-wide text-muted";
/// The answer of a revealed Context blank: the gold mark.
const MARK: &str = "rounded-sm bg-gold/20 px-0.5 text-ink";
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
pub(crate) fn start_phase(owner: &Owner) -> Phase {
    match load() {
        Load::Ok(state) => begin(owner, state),
        Load::Empty => begin(owner, State::empty()),
        Load::Invalid { .. } => Phase::Broken(BrokenKind::Invalid),
        Load::NewerVersion { .. } => Phase::Broken(BrokenKind::NewerVersion),
    }
}

/// A session on `state` for today (never before the state's last day),
/// saving first the promotions the start itself made.
fn begin(owner: &Owner, state: State) -> Phase {
    let today = session_today(browser_today(), &state);
    match Session::start(deck(), state.clone(), today, browser_seed()) {
        Ok(session) => {
            if session.changed_at_start() {
                save(&session.snapshot());
            }
            Phase::Running(owner.with(|| RwSignal::new(ReviewRun::new(session, state))))
        }
        Err(_) => Phase::Broken(BrokenKind::Invalid),
    }
}

/// Reads the store and starts a practice ahead of `n` terms (of stage
/// `only` when given) on it ([`practice_run`]); `None` when there is
/// nothing to practise (nothing stored, no started term in the batch).
pub(crate) fn practice_phase(owner: &Owner, n: usize, only: Option<Stage>) -> Option<Phase> {
    match load() {
        Load::Ok(state) => {
            let today = session_today(browser_today(), &state);
            let run = practice_run(state, today, n, only, browser_seed())?;
            Some(Phase::Running(owner.with(|| RwSignal::new(run))))
        }
        Load::Empty => None,
        Load::Invalid { .. } => Some(Phase::Broken(BrokenKind::Invalid)),
        Load::NewerVersion { .. } => Some(Phase::Broken(BrokenKind::NewerVersion)),
    }
}

/// A practice ahead on `state`: the `n` started terms with the nearest
/// next review ([`nearest`]), of stage `only` when given, every facet of
/// each asked as practice ([`Session::practice`]) in the order `seed` draws;
/// `None` when no term is picked.
pub(crate) fn practice_run(
    state: State,
    today: Date,
    n: usize,
    only: Option<Stage>,
    seed: u64,
) -> Option<ReviewRun> {
    let terms = nearest(&state, deck(), only, n);
    if terms.is_empty() {
        return None;
    }
    let session = Session::practice(deck(), &state, today, &terms, seed);
    Some(ReviewRun::practice(session, state))
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
            give(run, answer);
        }
    };
    view! {
        <div data-review="" class="flex flex-col gap-4" on:keydown=on_key>
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
fn give(run: RwSignal<ReviewRun>, answer: Answer) {
    run.update(|run| run.grade(answer, save));
}

/// Whether a key press comes from a field the learner types in (an input,
/// a textarea, a select, a `contenteditable`): the browser's, `false` on
/// the server.
fn editable_target(ev: &KeyboardEvent) -> bool {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        const EDITABLE: &str =
            "input, textarea, select, [contenteditable]:not([contenteditable=false])";
        ev.target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|element| element.closest(EDITABLE).ok().flatten().is_some())
    }
    #[cfg(not(feature = "hydrate"))]
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
            layout="sm:max-w-md"
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
    let key = Signal::derive(move || run.with(ReviewRun::key));
    // The card's `revealed`, handed out by `RevealCard` to `actions` (its
    // only channel): a signal, so the front reads it whatever the order
    // the slots are built in, and each new card (a language change) hands
    // its own.
    let handed = RwSignal::new(None::<Signal<bool>>);
    let shown = Signal::derive(move || handed.get().is_some_and(|signal| signal.get()));
    move || {
        let locale = Locale::from(language.get());
        view! {
            <div class=CARD_BOX inert=move || paused.get().then_some("")>
                <RevealCard
                    front=move || view! { <CardFront run=run revealed=shown /> }
                    reveal=move || view! { <CardReveal run=run /> }
                    actions=move |is_revealed| {
                        revealed.set_value(Some(is_revealed));
                        handed.set(Some(is_revealed));
                        grade_buttons(run, paused, is_revealed, locale)
                    }
                    key=key
                    reveal_label=td_string!(locale, review_show)
                />
            </div>
        }
    }
}

/// The two grade buttons, greyed until the reveal and while paused.
fn grade_buttons(
    run: RwSignal<ReviewRun>,
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
                on_click=move |_| give(run, Answer::Fail)
            >
                {td_string!(locale, grade_didnt)}
            </Button>
            <Button
                variant=Variant::Primary
                size=Size::Lg
                layout="flex-1"
                disabled=disabled
                on_click=move |_| give(run, Answer::Pass)
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
        Facet::Context => context_front(card, revealed).into_any(),
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
/// `revealed` (the reveal does not repeat the line), then where it comes
/// from (`Dhp 127 · <sutta>`).
fn context_front(card: Card, revealed: Signal<bool>) -> impl IntoView {
    let example = card.term.example.as_ref();
    let line = example
        .and_then(|example| example.blank.as_ref())
        .map(|blank| {
            view! {
                <p class="text-lg leading-8 text-ink">
                    <Pali layout="whitespace-pre-line">{context_line(blank, revealed)}</Pali>
                </p>
            }
        });
    // The line's translation goes here, under the filled line: in italics,
    // muted, with its translator, once the deck's `Example` carries one
    // (task 004). Nothing until then.
    let origin = example.map(|example| {
        view! {
            <p class="text-sm text-muted">
                {source_ref(&example.source)} " · " <Pali>{example.sutta.clone()}</Pali>
            </p>
        }
    });
    view! {
        {line}
        {origin}
    }
}

/// The example's line, windowed ([`context_window`]), as runs of text and
/// its blank(s): `___` until `revealed`, then the answer in a gold `<mark>`
/// in the same place.
fn context_line(blank: &Blank, revealed: Signal<bool>) -> impl IntoView + use<> {
    let answer = blank.answer.clone();
    context_window(blank, MAX_CHARS)
        .into_iter()
        .map(|segment| match segment {
            Segment::Text(text) => text.into_any(),
            Segment::Blank => {
                let answer = answer.clone();
                (move || blank_view(revealed.get(), answer.clone())).into_any()
            }
        })
        .collect_view()
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
                })} {answer} {details_view(card, locale)}
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

/// The literal meaning, the construction, the root, the example: each only
/// when the term has it. Context shows neither the literal (its answer
/// block has it) nor the example (its front's line is filled in place).
fn details_view(card: Card, locale: Locale) -> impl IntoView {
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
        let body = view! {
            <p>
                <Pali layout="whitespace-pre-line">{example.text.clone()}</Pali>
            </p>
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
    let add = Callback::new(move |count: usize| {
        run.update(|run| {
            run.add_new(count, save);
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
        view! {
            <div class="flex flex-col items-start gap-6">
                {heading}
                <Button variant=Variant::Primary size=Size::Lg on_click=move |_| on_home.run(())>
                    {td_string!(locale, back_home)}
                </Button> <NewTerms on_add=add new_left=new_left />
            </div>
        }
        .into_any()
    }
}

/// The end of a practice ahead: 'Practice done' and 'Back home'; nothing
/// moved, so no summary lines and no new terms.
fn practice_done(locale: Locale, on_home: Callback<()>) -> AnyView {
    view! {
        <div class="flex flex-col items-start gap-6">
            <h2 class="font-serif text-xl text-heading">
                {td_string!(locale, summary_practice_done)}
            </h2>
            <Button variant=Variant::Primary size=Size::Lg on_click=move |_| on_home.run(())>
                {td_string!(locale, back_home)}
            </Button>
        </div>
    }
    .into_any()
}

/// 'Session done', the terms that moved up and those that start again
/// tomorrow, the next due date; no score.
fn summary_view(summary: Summary, language: Language) -> impl IntoView {
    let locale = Locale::from(language);
    let up = summary.moved_up;
    let again = summary.back_to_day;
    let moved = if singular(locale, up) {
        td!(locale, summary_moved_up_1, count = up).into_any()
    } else {
        td!(locale, summary_moved_up_n, count = up).into_any()
    };
    let back = if singular(locale, again) {
        td!(locale, summary_again_1, count = again).into_any()
    } else {
        td!(locale, summary_again_n, count = again).into_any()
    };
    let next = summary
        .next_due
        .map(|date| td!(locale, summary_next_due, date = date_words(language, date)));
    view! {
        <div class="flex flex-col gap-3">
            <h2 class="font-serif text-xl text-heading">{td_string!(locale, summary_title)}</h2>
            <ul class="flex flex-col gap-1 text-ink">
                <li>{moved}</li>
                <li>{back}</li>
                {next.map(|next| view! { <li>{next}</li> })}
            </ul>
        </div>
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
        ReviewRun::new(session, before)
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
        ReviewRun::new(session, State::empty())
    }

    fn render_with(language: Language, run: ReviewRun, paused: bool) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
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
        let mut run = ReviewRun::new(session, State::empty());
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
            let none = Callback::new(|()| ());
            let paused = Signal::stored(false);
            view! { <SessionView phase=Phase::Broken(kind) paused=paused on_reload=none on_home=none /> }
            .to_html()
        })
    }

    #[test]
    fn a_progress_that_cannot_be_used_says_why_with_a_way_home() {
        for (kind, said) in [
            (BrokenKind::Invalid, "could not be read: it is damaged"),
            (BrokenKind::NewerVersion, "saved by a newer version"),
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
            let mut run =
                practice_run(state.clone(), today(), 5, Some(Stage::Week), 0).expect("a run");
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
        let mut run = practice_run(ahead_state(), today(), 2, None, 0).expect("a run");
        let (asked, _) = drain(&mut run, Answer::Pass);
        assert_eq!(terms_of(&asked), ["hoti 1", "kho"]);
    }

    /// The seed sets the order, and only the order: every seed asks the
    /// same cards, two seeds do not ask them in the same sequence.
    #[test]
    fn the_seed_orders_a_practice_ahead_and_nothing_else() {
        let ask = |seed| {
            let mut run = practice_run(ahead_state(), today(), 5, None, seed).expect("a run");
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
        assert!(practice_run(ahead_state(), today(), 5, Some(Stage::Year), 0).is_none());
        assert!(practice_run(State::empty(), today(), 5, None, 0).is_none());
        assert!(practice_run(ahead_state(), today(), 0, None, 0).is_none());
    }

    /// The header counts every question of the practice (`kho`: Meaning,
    /// Recall, Context).
    #[test]
    fn the_practice_header_counts_every_question() {
        let run = practice_run(ahead_state(), today(), 1, None, 0).expect("kho");
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
            let mut run = practice_run(ahead_state(), today(), 1, None, 0).expect("kho");
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
}
