//! `/`: the home. The brand (lotus and name, hidden on a phone where the
//! shell's top bar already shows them), the title for a screen reader only
//! (the brand names the page), then [`HomeView`]:
//! the first paint is a skeleton (`aria-busy`, a CTA row of the loaded
//! row's height);
//! after hydration the page reads the store ([`Home`]) and shows one of the
//! home's states (empty, due, nothing due, broken) in the same box.
//!
//! The CTA row holds the state's CTA at the left and, at the right, the
//! '+' of 'New terms', which opens a dialog with the picker. The CTA is one
//! button, Practice: '▶ Practice · N' while cards are due (the review:
//! stages move), else '▶ Practice' with the batch slider and the next
//! review's date beside (a practice ahead of the nearest terms, of the
//! strip's selected stage when one is: nothing in the progress changes).
//! With no term started there is no Practice: the '+' is the way in.
//! On `sm` and up the column sits in the middle of the page's height when
//! it is shorter; a phone keeps it at the top.
//!
//! The home also holds the progress (there is no other page for it): under
//! the CTA row, the strip of the six stages, three by three, whose cells
//! filter the term list in place (the page's state, no URL: lost on
//! reload), the list capped at about five rows, the terms the deck does
//! not know, then the data block (export, import).
//! With nothing saved, or a damaged progress, the data block alone offers
//! the import; a progress saved by a newer version offers none.
//!
//! The review runs in place: a CTA swaps the home's content for a session
//! ([`crate::components::session`]) under the brand, and Stop or 'Back
//! home' swap it back (no route, no history entry: the browser's Back
//! leaves the site).

use learn_core::date::Date;
use learn_core::deck::Deck;
use learn_core::progress::State;
use learn_core::session::Session;
use learn_core::stage::Stage;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_i18n::{td, td_string};
use palicanon_ui_lib::{
    Button, Dialog, Disclosed, DisclosureButton, Language, Size, Slider, Variant, language_signal,
};

use crate::catalogue::{Locale, strings};
use crate::components::data_block::DataBlock;
use crate::components::new_terms::{AddButton, NewTermsPicker, Pick};
use crate::components::session::{Phase, SessionView, practice_phase, singular, start_phase};
use crate::components::term_list::{Orphans, StageStrip, StageTerms, next_review, orphan_rows};
use crate::deck::deck;
use crate::learn::home_state::{BrokenKind, HomeState, home_state, new_left};
use crate::learn::stages::stage_counts;
use crate::learn::words::date_words;
use crate::store::{self, Load, on_storage_change};
use crate::today::{browser_today, on_visible, session_today};

/// The CTA box of every state: its content at the top, no empty height.
const CTA_BOX: &str = "flex flex-col gap-4";

/// A state's wrapper: the box, then what sits under it.
const FRAME: &str = "flex flex-col gap-6";

/// The row of the state's CTA and the '+' (HC), one thin row: at least a
/// compact button's height (`h-8`), the skeleton's too, so the layout does
/// not move when the store has been read.
const CTA_ROW: &str = "flex flex-wrap items-center gap-3 min-h-8";

/// How many terms a practice ahead asks at first (user, 2026-10-06:
/// « pourquoi pas 5 de base »).
const BATCH: u32 = 5;

/// The stored progress the home lists, with the session's today (the
/// export's file name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// The stored progress.
    pub state: State,
    /// The session's today (`session_today`).
    pub today: Date,
}

/// What the home shows, computed from the store.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    state: HomeState,
    new_left: usize,
    counts: [usize; 6],
    saved: Option<Saved>,
}

impl Default for Model {
    /// The first paint: the skeleton.
    fn default() -> Self {
        Model {
            state: HomeState::Loading,
            new_left: 0,
            counts: [0; 6],
            saved: None,
        }
    }
}

/// What the learner can ask of the home; [`Home`] performs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HomeAction {
    /// Review what is due ('▶ Practice · N'): a session on the stored
    /// progress, in place; stages move.
    Review,
    /// Practise ahead (nothing due): the `n` started terms with the nearest
    /// next review, of stage `only` when given; nothing in the progress
    /// changes.
    PracticeAhead {
        /// How many terms.
        n: usize,
        /// The strip's selected stage, if any.
        only: Option<Stage>,
    },
    /// Add this many new terms.
    AddNew(usize),
    /// Replace the unreadable progress by an empty one.
    StartAgain,
    /// Download the stored text as is.
    DownloadRaw,
    /// Replace the whole progress by an imported one (confirmed).
    Replace(State),
}

/// What the page shows: the home's own content, or a session in its
/// place (one page, no route, no history entry).
#[derive(Clone, Copy)]
pub(crate) enum Mode {
    /// The title, the CTA row and the progress.
    Home,
    /// A review session ([`SessionView`]) under the brand.
    Session(Phase),
}

/// The page: the brand, then the home's content (a skeleton until the
/// store is read, an Effect after hydration; read again when the page
/// becomes visible or another tab changes it), its list filtered by the
/// strip (the page's state); or, once a CTA started one, a session in its
/// place, left by Stop or 'Back home', which read the store again.
#[component]
pub fn Home() -> impl IntoView {
    let page = Page::new();
    view! {
        <div
            data-page="home"
            class="flex flex-col gap-8 px-6 pt-6 pb-12 sm:mx-auto sm:w-full sm:max-w-[40rem] sm:flex-1 sm:justify-center"
        >
            {move || {
                let mode = page.mode.get();
                let model = match mode {
                    Mode::Home => page.model.get(),
                    Mode::Session(_) => Model::default(),
                };
                view! {
                    <HomeScreen
                        mode=mode
                        model=model
                        stage=page.stage
                        batch=page.batch
                        on_action=page.on_action
                        paused=page.paused
                        on_reload=page.start
                        on_home=page.home
                    />
                }
            }}
        </div>
    }
}

/// The page's state and what changes it.
#[derive(Clone, Copy)]
struct Page {
    /// What the home shows, from the store.
    model: RwSignal<Model>,
    /// The home's content or a session.
    mode: RwSignal<Mode>,
    /// The session is paused: another tab changed the progress.
    paused: RwSignal<bool>,
    /// Starts a session on the stored state, in place.
    start: Callback<()>,
    /// Back to the home's content, the store read again.
    home: Callback<()>,
    /// Performs a CTA.
    on_action: Callback<HomeAction>,
    /// The strip's filter: the page's state, not the URL (lost on reload).
    stage: RwSignal<Option<Stage>>,
    /// The practice-ahead batch (the slider): the page's state.
    batch: RwSignal<u32>,
}

impl Page {
    /// The signals, the store read after hydration and on the page's
    /// return, the storage event. In a session the model is not read: a
    /// refresh never rebuilds the card under the learner, and another
    /// tab's change pauses the session instead.
    fn new() -> Self {
        let owner = Owner::current().expect("a component runs under an owner");
        let (model, mode, paused) = (
            RwSignal::new(Model::default()),
            RwSignal::new(Mode::Home),
            RwSignal::new(false),
        );
        let refresh = move || {
            let next = read_model();
            if model.get_untracked() != next {
                model.set(next);
            }
        };
        let at_home = move || matches!(mode.get_untracked(), Mode::Home);
        Effect::new(move |_| refresh());
        on_visible(move || {
            if at_home() {
                refresh();
            }
        });
        on_storage_change(move || {
            if at_home() {
                refresh();
            } else {
                paused.set(true);
            }
        });
        let (start, ahead) = starters(owner, mode, paused, refresh);
        let home = Callback::new(move |()| {
            refresh();
            mode.set(Mode::Home);
        });
        let on_action = Callback::new(move |action| {
            perform(
                action,
                refresh,
                || start.run(()),
                |n, only| ahead.run((n, only)),
            );
        });
        Page {
            model,
            mode,
            paused,
            start,
            home,
            on_action,
            stage: RwSignal::new(None),
            batch: RwSignal::new(BATCH),
        }
    }
}

/// The two ways into a session in place: `start`, the review on the stored
/// state; `ahead`, a practice ahead of `(n, only)`, or a fresh read of the
/// store when there is nothing to practise.
fn starters(
    owner: Owner,
    mode: RwSignal<Mode>,
    paused: RwSignal<bool>,
    refresh: impl Fn() + Send + Sync + 'static,
) -> (Callback<()>, Callback<(usize, Option<Stage>)>) {
    let open = move |phase| {
        paused.set(false);
        mode.set(Mode::Session(phase));
    };
    let review_owner = owner.clone();
    let start = Callback::new(move |()| open(start_phase(&review_owner)));
    let ahead = Callback::new(move |(n, only)| match practice_phase(&owner, n, only) {
        Some(phase) => open(phase),
        None => refresh(),
    });
    (start, ahead)
}

/// The page's column for `mode`: the brand, then the title (screen readers
/// only) and [`HomeView`] at home, or [`SessionView`] alone in a session.
#[component]
fn HomeScreen(
    /// The home's content or a session.
    mode: Mode,
    /// What the home shows (read at home only).
    model: Model,
    /// The strip's filter.
    stage: RwSignal<Option<Stage>>,
    /// The practice-ahead batch.
    batch: RwSignal<u32>,
    /// The CTAs; [`Home`] performs them.
    on_action: Callback<HomeAction>,
    /// The session is paused (another tab changed the progress).
    #[prop(into)]
    paused: Signal<bool>,
    /// 'Reload the session'.
    on_reload: Callback<()>,
    /// Stop, 'Back home' and the brand in a session: back to the home.
    on_home: Callback<()>,
) -> impl IntoView {
    let t = strings();
    let in_session = matches!(mode, Mode::Session(_));
    let body = match mode {
        Mode::Home => {
            let Model {
                state,
                new_left,
                counts,
                saved,
            } = model;
            view! {
                <h1 class="sr-only">{move || td_string!(t.locale(), home_title)}</h1>
                <HomeView
                    state=state
                    new_left=new_left
                    counts=counts
                    saved=saved
                    stage=stage
                    batch=batch
                    on_action=on_action
                />
            }
            .into_any()
        }
        Mode::Session(phase) => {
            view! { <SessionView phase=phase paused=paused on_reload=on_reload on_home=on_home /> }
                .into_any()
        }
    };
    view! {
        <BrandLink in_session=in_session on_home=on_home />
        {body}
    }
}

/// Reads the store and the clock into the model.
fn read_model() -> Model {
    compute(&store::load(), deck(), browser_today())
}

/// The model for `load`, the deck and the browser's date: `today` is the
/// later of it and the state's last day (Q2). A load the store could not
/// use has nothing to add and nothing to count.
fn compute(load: &Load, deck: &Deck, browser: Date) -> Model {
    let (today, counts, saved) = match load {
        Load::Ok(state) => {
            let today = session_today(browser, state);
            let saved = Saved {
                state: state.clone(),
                today,
            };
            (today, stage_counts(state, deck), Some(saved))
        }
        _ => (browser, [0; 6], None),
    };
    Model {
        state: home_state(load, deck, today),
        new_left: new_left(load, deck),
        counts,
        saved,
    }
}

/// The state after `action` (new terms), to be saved; `None` when it
/// moved nothing (or the day is before the state's last).
fn grown(action: HomeAction, state: State, deck: &Deck, today: Date) -> Option<State> {
    let mut session = Session::start(deck, state, today).ok()?;
    let moved = match action {
        HomeAction::AddNew(n) => session.request_new(n),
        HomeAction::Review
        | HomeAction::PracticeAhead { .. }
        | HomeAction::StartAgain
        | HomeAction::DownloadRaw
        | HomeAction::Replace(_) => 0,
    };
    (moved > 0).then(|| session.snapshot())
}

/// Does what the learner asked: start the review in place (`go`), a
/// practice ahead (`ahead`), or grow the progress, save it and start a
/// session in place; or, when nothing moved, read the store again.
fn perform(
    action: HomeAction,
    refresh: impl Fn(),
    go: impl Fn(),
    ahead: impl Fn(usize, Option<Stage>),
) {
    match action {
        HomeAction::Review => go(),
        HomeAction::PracticeAhead { n, only } => ahead(n, only),
        HomeAction::StartAgain => {
            store::save(&State::empty());
            refresh();
        }
        HomeAction::DownloadRaw => download_raw(),
        HomeAction::Replace(state) => {
            store::save(&state);
            refresh();
        }
        HomeAction::AddNew(_) => {
            let state = match store::load() {
                Load::Ok(state) => state,
                Load::Empty => State::empty(),
                Load::Invalid { .. } | Load::NewerVersion { .. } => return refresh(),
            };
            let today = session_today(browser_today(), &state);
            match grown(action, state, deck(), today) {
                Some(next) => {
                    store::save(&next);
                    go();
                }
                None => refresh(),
            }
        }
    }
}

/// Offers the stored text, as is, as a file.
fn download_raw() {
    if let Load::Invalid { raw } | Load::NewerVersion { raw } = store::load() {
        store::download(&store::export_name(browser_today()), &raw);
    }
}

/// What the home shows for `state`: the CTA row (the state's CTA or the
/// next review's date, and the '+' that opens the 'New terms' dialog), the
/// progress (the strip, the list, the data block), or the Broken banner.
#[allow(
    clippy::similar_names,
    reason = "`state` and `stage` are the design's names for the two props"
)]
#[component]
pub fn HomeView(
    /// The home's state ([`crate::learn::home_state::home_state`]).
    state: HomeState,
    /// The deck's terms not started yet
    /// ([`crate::learn::home_state::new_left`]): 0 hides the '+' (Q4).
    new_left: usize,
    /// Started terms per stage, Day first (the strip); their sum is the
    /// practice slider's maximum, 0 offers no Practice. Zeros by default.
    #[prop(optional)]
    counts: [usize; 6],
    /// The stored progress (the term list, the orphans, the export); none:
    /// the strip without a list.
    #[prop(default = None)]
    saved: Option<Saved>,
    /// The strip's filter: the list shows that stage only; `None`, every
    /// term. A filter of its own when not given. A practice ahead asks
    /// that stage only.
    #[prop(optional)]
    stage: Option<RwSignal<Option<Stage>>>,
    /// The practice-ahead batch (the slider's value). One of its own, at
    /// 5, when not given.
    #[prop(optional)]
    batch: Option<RwSignal<u32>>,
    /// Called with what the learner chose; [`Home`] performs it. Nothing
    /// happens without it.
    #[prop(optional, into)]
    on_action: Option<Callback<HomeAction>>,
) -> impl IntoView {
    let stage = stage.unwrap_or_else(|| RwSignal::new(None));
    let batch = batch.unwrap_or_else(|| RwSignal::new(BATCH));
    let act = Callback::new(move |action| {
        if let Some(on_action) = on_action {
            on_action.run(action);
        }
    });
    match state {
        HomeState::Loading => view! { <Skeleton /> }.into_any(),
        HomeState::Empty => view! { <EmptyBlock new_left=new_left act=act /> }.into_any(),
        HomeState::Due(due) => view! {
            <DueBlock due=due new_left=new_left act=act>
                <HomeProgress counts=counts saved=saved stage=stage act=act />
            </DueBlock>
        }
        .into_any(),
        HomeState::NothingDue => {
            let cta = nothing_due_cta(counts, saved.as_ref(), stage, batch);
            view! {
                <NothingDueBlock cta=cta new_left=new_left act=act>
                    <HomeProgress counts=counts saved=saved stage=stage act=act />
                </NothingDueBlock>
            }
            .into_any()
        }
        HomeState::Broken(kind) => view! { <BrokenBlock kind=kind act=act /> }.into_any(),
    }
}

/// Nothing due: '▶ Practice' ahead when a term the deck knows is started,
/// the next review's date beside; with none started, the date alone (or
/// nothing).
fn nothing_due_cta(
    counts: [usize; 6],
    saved: Option<&Saved>,
    stage: RwSignal<Option<Stage>>,
    batch: RwSignal<u32>,
) -> Option<CtaSlot> {
    let next = saved.and_then(|saved| next_review(&saved.state));
    let started = counts.iter().sum();
    if started > 0 {
        Some(CtaSlot::Ahead(Ahead {
            started,
            batch,
            stage,
            next,
        }))
    } else {
        next.map(CtaSlot::NextReview)
    }
}

/// The first paint: busy, one disabled neutral button in a CTA row of the
/// loaded row's height, no state label, no strip.
#[component]
fn Skeleton() -> impl IntoView {
    let t = strings();
    view! {
        <div aria-busy="true" data-home-state="loading" class=FRAME>
            <div data-cta-box="" class=CTA_BOX>
                <div data-cta-row="" class=CTA_ROW>
                    <Button size=Size::Sm disabled=Signal::derive(|| true) on_click=|_| {}>
                        {move || td_string!(t.locale(), home_loading)}
                    </Button>
                </div>
            </div>
        </div>
    }
}

/// No progress yet: the teaching sentence, the CTA row under it with the
/// '+' of 'New terms' alone at the right (the way in), then the data block,
/// whose import is the way back to a saved progress.
#[component]
fn EmptyBlock(new_left: usize, act: Callback<HomeAction>) -> impl IntoView {
    let t = strings();
    view! {
        <div data-home-state="empty" class=FRAME>
            <CtaBox cta=None new_left=new_left act=act>
                <p class="max-w-prose text-body">{move || td_string!(t.locale(), home_teach)}</p>
            </CtaBox>
            <ImportOnly act=act />
        </div>
    }
}

/// Cards due: the CTA row ('▶ Practice · N' and the '+'), then `children`
/// (the progress).
#[component]
fn DueBlock(
    due: usize,
    new_left: usize,
    act: Callback<HomeAction>,
    children: Children,
) -> impl IntoView {
    view! {
        <div data-home-state="due" class=FRAME>
            <CtaBox cta=Some(CtaSlot::Practice(due)) new_left=new_left act=act />
            {children()}
        </div>
    }
}

/// Nothing due: the CTA row ('▶ Practice' ahead with its slider and the
/// next review's date when a term is started, else the date, else nothing,
/// and the '+'); no row when it would be empty; then `children` (the
/// progress).
#[component]
fn NothingDueBlock(
    cta: Option<CtaSlot>,
    new_left: usize,
    act: Callback<HomeAction>,
    children: Children,
) -> impl IntoView {
    view! {
        <div data-home-state="nothing-due" class=FRAME>
            <CtaBox cta=cta new_left=new_left act=act />
            {children()}
        </div>
    }
}

/// What the CTA row's left slot shows (HC: one compact button, no
/// headline; or a date, muted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CtaSlot {
    /// '▶ Practice · N', N the cards due: the review.
    Practice(usize),
    /// '▶ Practice', its batch slider and the next review's date (nothing
    /// due, terms started): a practice ahead.
    Ahead(Ahead),
    /// 'Next review: <date>' (nothing due, nothing started), muted.
    NextReview(Date),
}

/// What a practice ahead is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ahead {
    /// The started terms the deck knows: the slider's maximum.
    started: usize,
    /// How many terms to practise: the slider's value, the page's state.
    batch: RwSignal<u32>,
    /// The strip's selected stage: the practice asks that stage only.
    stage: RwSignal<Option<Stage>>,
    /// The next review's date, shown beside.
    next: Option<Date>,
}

/// The terms a practice ahead asks for a slider at `batch`: at least 1, at
/// most the `started` terms.
fn batch_of(started: usize, batch: u32) -> usize {
    usize::try_from(batch)
        .unwrap_or(usize::MAX)
        .clamp(1, started.max(1))
}

/// What a click on the CTA asks: the review while cards are due, else a
/// practice ahead of the slider's batch in the selected stage; the date
/// asks nothing.
fn cta_action(cta: CtaSlot) -> Option<HomeAction> {
    match cta {
        CtaSlot::Practice(_) => Some(HomeAction::Review),
        CtaSlot::Ahead(ahead) => Some(HomeAction::PracticeAhead {
            n: batch_of(ahead.started, ahead.batch.get_untracked()),
            only: ahead.stage.get_untracked(),
        }),
        CtaSlot::NextReview(_) => None,
    }
}

/// The CTA box: `children` (the Empty state's sentence) above the CTA row,
/// then the dialog the row's '+' opens (outside the box: its title is no
/// headline of the box). Nothing when it would be empty.
#[component]
fn CtaBox(
    /// The row's left slot; none: the '+' alone.
    cta: Option<CtaSlot>,
    /// The deck's terms not started yet; 0: no '+' and no dialog.
    new_left: usize,
    /// Performs the learner's choice.
    act: Callback<HomeAction>,
    /// What sits above the row.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let row = (cta.is_some() || new_left > 0)
        .then(|| view! { <CtaRow cta=cta new_left=new_left open=open act=act /> });
    (children.is_some() || row.is_some()).then(|| {
        view! {
            <div data-cta-box="" class=CTA_BOX>
                {children.map(|children| children())}
                {row}
            </div>
            <NewTermsDialog new_left=new_left act=act open=open />
        }
    })
}

/// The CTA row (HC), one thin row: the state's [`Cta`] at the left, the
/// '+' of 'New terms' at the right.
#[component]
fn CtaRow(
    /// The left slot; none: the '+' alone.
    cta: Option<CtaSlot>,
    /// The deck's terms not started yet; 0: no '+'.
    new_left: usize,
    /// The dialog's state, shared with [`NewTermsDialog`].
    open: RwSignal<bool>,
    /// Performs the CTA.
    act: Callback<HomeAction>,
) -> impl IntoView {
    view! {
        <div data-cta-row="" class=CTA_ROW>
            {cta.map(|cta| view! { <Cta cta=cta act=act /> })}
            <AddTermsButton new_left=new_left open=open />
        </div>
    }
}

/// The row's left slot, one component a later change can restyle alone:
/// a compact CTA, or the next review's date, muted. Follows the language.
#[component]
fn Cta(
    /// What it shows.
    cta: CtaSlot,
    /// Performs the CTA.
    act: Callback<HomeAction>,
) -> impl IntoView {
    let language = language_signal();
    move || {
        let language = language.get();
        let locale = Locale::from(language);
        match cta {
            CtaSlot::Practice(due) => {
                let name = if singular(locale, due) {
                    td_string!(locale, cta_practice_label_1, count = due).to_string()
                } else {
                    td_string!(locale, cta_practice_label_n, count = due).to_string()
                };
                let label = td_string!(locale, cta_practice);
                view! { <CompactCta cta=cta label=label count=due name=name act=act /> }.into_any()
            }
            CtaSlot::Ahead(ahead) => view! { <PracticeAhead ahead=ahead act=act /> }.into_any(),
            CtaSlot::NextReview(date) => next_review_line(language, date),
        }
    }
}

/// 'Next review: <date>', muted.
fn next_review_line(language: Language, date: Date) -> AnyView {
    let locale = Locale::from(language);
    let date = date_words(language, date);
    view! { <p class="text-muted">{td!(locale, next_review, date = date)}</p> }.into_any()
}

/// Nothing due: '▶ Practice', named 'Practice ahead, N terms' (N follows
/// the slider), then ui-lib's `Slider` (1 to the started terms), then the
/// next review's date.
#[component]
fn PracticeAhead(
    /// The batch, the stage, the started terms, the date.
    ahead: Ahead,
    /// Performs the practice.
    act: Callback<HomeAction>,
) -> impl IntoView {
    let language = language_signal();
    let button = move || {
        let locale = Locale::from(language.get());
        let count = batch_of(ahead.started, ahead.batch.get());
        let name = td_string!(locale, cta_practice_ahead_label, count = count).to_string();
        let label = td_string!(locale, cta_practice);
        view! { <CompactCta cta=CtaSlot::Ahead(ahead) label=label name=name act=act /> }
    };
    view! {
        {button}
        <Slider
            value=ahead.batch
            min=1
            max=u32::try_from(ahead.started).unwrap_or(u32::MAX)
            label=Signal::derive(move || {
                td_string!(Locale::from(language.get()), practice_batch_label).to_owned()
            })
        />
        {move || ahead.next.map(|date| next_review_line(language.get(), date))}
    }
}

/// A compact CTA: Primary, small (the '+' height), '▶ label' and, given,
/// '· count'; named `name` when given (the ▶ and the dot are not read).
#[component]
fn CompactCta(
    /// The slot it stands for: what a click asks ([`cta_action`]).
    cta: CtaSlot,
    /// Its label.
    label: &'static str,
    /// A count shown after the label.
    #[prop(optional)]
    count: Option<usize>,
    /// Its accessible name, when the label and count are not enough.
    #[prop(optional)]
    name: Option<String>,
    /// Performs it.
    act: Callback<HomeAction>,
) -> impl IntoView {
    let on_click = move |_| {
        if let Some(action) = cta_action(cta) {
            act.run(action);
        }
    };
    let body = move || {
        view! {
            <span aria-hidden="true">"▶"</span>
            <span>{label}</span>
            {count
                .map(|count| {
                    view! {
                        <span aria-hidden="true">"·"</span>
                        <span>{count}</span>
                    }
                })}
        }
    };
    match name {
        Some(name) => view! {
            <Button variant=Variant::Primary size=Size::Sm label=name on_click=on_click>
                {body()}
            </Button>
        }
        .into_any(),
        None => view! {
            <Button variant=Variant::Primary size=Size::Sm on_click=on_click>
                {body()}
            </Button>
        }
        .into_any(),
    }
}

/// The unreadable progress: the banner, 'Download the raw copy', then, for
/// an Invalid one only, 'Start again' and the data block's import (Q3: a
/// newer one is never overwritten). The CTAs that need the state are not
/// offered.
#[component]
fn BrokenBlock(kind: BrokenKind, act: Callback<HomeAction>) -> impl IntoView {
    let t = strings();
    view! {
        <div data-home-state="broken" class=FRAME>
            <div data-cta-box="" class=CTA_BOX>
                <p role="alert" class="max-w-prose text-body">
                    {move || match kind {
                        BrokenKind::Invalid => td_string!(t.locale(), broken_invalid),
                        BrokenKind::NewerVersion => td_string!(t.locale(), broken_newer),
                    }}
                </p>
                <div class="flex flex-wrap gap-3">
                    <Button
                        variant=Variant::Primary
                        size=Size::Lg
                        on_click=move |_| act.run(HomeAction::DownloadRaw)
                    >
                        {move || td_string!(t.locale(), broken_download)}
                    </Button>
                    {(kind == BrokenKind::Invalid)
                        .then(|| {
                            view! {
                                <Button
                                    size=Size::Lg
                                    on_click=move |_| act.run(HomeAction::StartAgain)
                                >
                                    {move || td_string!(t.locale(), broken_start_again)}
                                </Button>
                            }
                        })}
                </div>
            </div>
            {(kind == BrokenKind::Invalid).then(|| view! { <ImportOnly act=act /> })}
        </div>
    }
}

/// The disclosure that opens [`NewTermsDialog`]: a '+', small, Accent, at
/// the right of the CTA row; its name is 'New terms' in the page's
/// language. Nothing when the deck has no term left to start.
#[component]
fn AddTermsButton(
    /// The deck's terms not started yet; 0 renders nothing.
    new_left: usize,
    /// The dialog's state, shared with [`NewTermsDialog`].
    open: RwSignal<bool>,
) -> impl IntoView {
    let t = strings();
    (new_left > 0).then_some(move || {
        let label = td_string!(t.locale(), add_terms);
        view! {
            <DisclosureButton
                controls=Disclosed::new(NEW_TERMS_ID, open)
                variant=Variant::Accent
                size=Size::Sm
                label_closed=label
                label_open=label
                layout="ml-auto"
            >
                <span class="text-lg leading-none">"+"</span>
            </DisclosureButton>
        }
    })
}

/// The id of the 'New terms' dialog, which the '+' controls.
const NEW_TERMS_ID: &str = "pc-new-terms";

/// The dialog [`AddTermsButton`] opens, titled 'New terms': the picker in
/// its body, 'Cancel' (Ghost) and 'Add' (Primary) in its footer. 'Add'
/// closes it, then adds the terms and starts the session in place.
/// Nothing when the deck has no term left to start.
#[component]
fn NewTermsDialog(
    /// The deck's terms not started yet; 0 renders nothing.
    new_left: usize,
    /// Performs the learner's choice.
    act: Callback<HomeAction>,
    /// The dialog's state, shared with [`AddTermsButton`].
    open: RwSignal<bool>,
) -> impl IntoView {
    let t = strings();
    let pick = Pick::new();
    let on_add = Callback::new(move |n| {
        open.set(false);
        act.run(HomeAction::AddNew(n));
    });
    let footer = move || {
        view! {
            <Button variant=Variant::Ghost on_click=move |_| open.set(false)>
                {move || td_string!(t.locale(), cancel)}
            </Button>
            <AddButton pick=pick new_left=new_left on_add=on_add />
        }
    };
    (new_left > 0).then(|| {
        view! {
            <Dialog
                controls=Disclosed::new(NEW_TERMS_ID, open)
                title=Signal::derive(move || td_string!(t.locale(), add_terms).to_owned())
                footer=footer
            >
                <NewTermsPicker pick=pick new_left=new_left />
            </Dialog>
        }
    })
}

/// The progress under the CTA row: the strip (its cells filter the list
/// in place), the term list and the orphans when a progress is saved, then
/// the data block.
#[component]
fn HomeProgress(
    counts: [usize; 6],
    saved: Option<Saved>,
    stage: RwSignal<Option<Stage>>,
    act: Callback<HomeAction>,
) -> impl IntoView {
    let on_replace = Callback::new(move |state| act.run(HomeAction::Replace(state)));
    let (today, list) = match &saved {
        Some(Saved { state, today }) => {
            let orphans = orphan_rows(state);
            let list = view! {
                <StageTerms progress=state.clone() stage=stage />
                <Orphans orphans=orphans />
            };
            (*today, Some(list))
        }
        None => (Date::from_days(0), None),
    };
    let export = saved.map(|saved| saved.state);
    view! {
        <StageStrip counts=counts stage=stage />
        {list}
        <DataBlock export_state=export today=today can_import=true on_replace=on_replace />
    }
}

/// The data block with nothing to export: its import replaces a missing or
/// damaged progress.
#[component]
fn ImportOnly(act: Callback<HomeAction>) -> impl IntoView {
    let on_replace = Callback::new(move |state| act.run(HomeAction::Replace(state)));
    view! {
        <DataBlock
            export_state=None
            today=Date::from_days(0)
            can_import=true
            on_replace=on_replace
        />
    }
}

/// The lotus and "PaliLearn", the service's name, a link home (HL). Hidden
/// on a phone, where the shell's top bar already shows the lotus. In a
/// session a plain click goes back to the home's content (`on_home`, no
/// navigation); `href="/"` stays for a middle or modified click.
#[component]
fn BrandLink(
    /// A session is shown.
    in_session: bool,
    /// Back to the home's content.
    on_home: Callback<()>,
) -> impl IntoView {
    let on_click = move |ev: MouseEvent| {
        if in_session && plain_click(&ev) {
            ev.prevent_default();
            on_home.run(());
        }
    };
    view! {
        <a
            href="/"
            on:click=on_click
            class="hidden shrink-0 items-center gap-2 font-serif text-lg font-semibold text-heading select-none sm:flex"
        >
            <img src="/lotus.png" alt="" width="22" height="22" />
            "PaliLearn"
        </a>
    }
}

/// A primary click with no modifier: the click that follows a link in
/// this tab.
fn plain_click(ev: &MouseEvent) -> bool {
    ev.button() == 0 && !(ev.ctrl_key() || ev.meta_key() || ev.shift_key() || ev.alt_key())
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use learn_core::deck::TermId;
    use learn_core::facet::FacetSet;
    use learn_core::progress::Progress;
    use learn_core::stage::Stage;
    use leptos_router::components::Router;
    use leptos_router::location::RequestUrl;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::components::session::tests::run_on;
    use crate::deck::deck;
    use crate::learn::home_state::BrokenKind;
    use crate::test_support::{button_labelled, progress_fixture, tag_around};

    /// [`Home`] as the server renders it at `/`, inside the real Router.
    fn render(language: Language) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            provide_context(RequestUrl::new("/"));
            view! {
                <Router>
                    <Home />
                </Router>
            }
            .to_html()
        })
    }

    /// [`HomeView`] fed `state`, `new_left` and `counts`.
    fn view_of(
        language: Language,
        state: HomeState,
        new_left: usize,
        counts: [usize; 6],
    ) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                view! { <HomeView state=state new_left=new_left counts=counts /> }.to_html()
            })
            // The hydration markers between a count and its words are no text.
            .replace("<!>", "")
    }

    fn en(state: HomeState, new_left: usize) -> String {
        view_of(Language::En, state, new_left, [0; 6])
    }

    /// Started terms per stage: 21 in all.
    const STARTED: [usize; 6] = [6, 5, 4, 3, 2, 1];

    /// Nothing due today.
    const NOTHING_DUE: HomeState = HomeState::NothingDue;

    /// The `<tag …>` open tag just before the first `text`.
    fn tag_before(html: &str, tag: &str, text: &str) -> String {
        let at = html
            .find(text)
            .unwrap_or_else(|| panic!("{text:?} in {html}"));
        let start = html[..at]
            .rfind(&format!("<{tag}"))
            .unwrap_or_else(|| panic!("<{tag} before {text:?} in {html}"));
        let end = start + html[start..].find('>').expect(html);
        html[start..=end].to_owned()
    }

    /// `name` is an attribute of the open tag (not a class token).
    fn has_attr(tag: &str, name: &str) -> bool {
        tag.trim_end_matches('>')
            .split_whitespace()
            .any(|token| token == name || token.starts_with(&format!("{name}=")))
    }

    /// The whole `<div>` element whose open tag holds byte `inside`.
    fn element_at(html: &str, inside: usize) -> &str {
        let start = html[..inside].rfind("<div").expect(html);
        let mut depth = 0;
        let mut i = start;
        loop {
            let open = html[i..].find("<div").map(|o| i + o);
            let close = html[i..].find("</div>").map(|c| i + c).expect(html);
            match open {
                Some(open) if open < close => {
                    depth += 1;
                    i = open + 4;
                }
                _ => {
                    depth -= 1;
                    i = close + 6;
                    if depth == 0 {
                        return &html[start..i];
                    }
                }
            }
        }
    }

    fn at(html: &str, text: &str) -> usize {
        html.find(text)
            .unwrap_or_else(|| panic!("{text:?} in {html}"))
    }

    // ---- T1: the first paint ----

    #[test]
    fn the_home_has_its_title_in_every_language() {
        for (language, title) in [
            (Language::En, "Learn Pāli"),
            (Language::Fr, "Apprendre le pāli"),
            (Language::Vi, "Học tiếng Pāli"),
        ] {
            let html = render(language);
            assert_eq!(html.matches("<h1").count(), 1, "{html}");
            assert!(html.contains(&format!(">{title}</h1>")), "{html}");
        }
    }

    /// The brand (the lotus and the name) already names the page (user,
    /// 2026-10-06): the title is no longer shown, it stays the page's
    /// heading for a screen reader.
    #[test]
    fn the_title_is_read_not_shown() {
        for language in [Language::En, Language::Fr, Language::Vi] {
            let html = render(language);
            let h1 = tag_before(&html, "h1", "</h1>");
            let classes: Vec<&str> = h1.split(['"', ' ']).collect();
            assert!(classes.contains(&"sr-only"), "{h1}");
            for shown in ["font-serif", "text-2xl", "text-heading"] {
                assert!(!classes.contains(&shown), "{shown}: {h1}");
            }
        }
    }

    #[test]
    fn the_brand_is_palilearn_hidden_on_a_phone_with_the_lotus() {
        let html = render(Language::En);
        let tag = tag_before(&html, "a", "PaliLearn");
        assert!(tag.contains(r#"href="/""#), "{tag}");
        assert!(tag.contains("hidden"), "{tag}");
        assert!(tag.contains("sm:flex"), "{tag}");
        assert!(html.contains(r#"<img src="/lotus.png""#), "{html}");
        assert!(!html.contains("PaliCanon"), "{html}");
    }

    /// HL: the page sits in one centred column from `sm` up, in the middle
    /// of the page's height when it is shorter (008); a phone keeps the
    /// full width, its gutter, and the top.
    #[test]
    fn the_home_sits_in_one_centred_column() {
        let html = render(Language::En);
        let tag = tag_before(&html, "div", r#"data-page="home""#);
        for class in [
            "px-6",
            "gap-8",
            "sm:mx-auto",
            "sm:w-full",
            "sm:max-w-[40rem]",
            "sm:flex-1",
            "sm:justify-center",
        ] {
            assert!(tag.split(['"', ' ']).any(|c| c == class), "{class}: {tag}");
        }
    }

    /// HC: no state has a headline; one button is enough.
    #[test]
    fn no_state_has_a_headline() {
        for state in [HomeState::Due(1), HomeState::Due(12), HomeState::NothingDue] {
            let html = en(state, 10);
            for absent in ["to review", "Nothing due today"] {
                assert!(!html.contains(absent), "{state:?} {absent}: {html}");
            }
            if html.contains("data-cta-box") {
                let cta_box = element_at(&html, at(&html, "data-cta-box"));
                assert!(!cta_box.contains("<h2"), "{state:?}: {cta_box}");
            }
        }
    }

    /// The CTA row holds the state's CTA at the left and the '+' at the
    /// right (user, 2026-10-06: « passe le bouton + au niveau du resume »).
    #[test]
    fn the_cta_row_holds_the_cta_at_the_left_and_the_plus_at_the_right() {
        for state in [HomeState::Due(12), NOTHING_DUE] {
            let cta = ">Practice<";
            let html = view_of(Language::En, state, 10, STARTED);
            let row = element_at(&html, at(&html, "data-cta-row"));
            assert!(row.contains("flex flex-wrap items-center gap-3"), "{row}");
            assert_eq!(row.matches("<button").count(), 2, "{row}");
            assert!(
                at(row, cta) < at(row, r#"aria-label="New terms""#),
                "the CTA, then the '+': {row}"
            );
            let add = tag_before(row, "button", r#"aria-label="New terms""#);
            assert!(add.split(['"', ' ']).any(|c| c == "ml-auto"), "{add}");
            assert!(
                !row.contains("role=\"dialog\""),
                "the dialog is not in the row: {row}"
            );
        }
    }

    /// The '+' sits on the CTA row, before the strip and the list (no
    /// longer between them), small, at the right, in Accent.
    #[test]
    fn the_plus_sits_on_the_cta_row_not_between_the_strip_and_the_list() {
        for state in started() {
            let html = with_progress(Language::En, state, None);
            let row = element_at(&html, at(&html, "data-cta-row"));
            assert!(row.contains(r#"aria-label="New terms""#), "{row}");
            let order = [
                at(&html, "data-cta-row"),
                at(&html, r#"aria-label="New terms""#),
                at(&html, ">Daily<"),
                at(&html, "data-term-list"),
            ];
            assert!(order.is_sorted(), "{order:?}: {html}");
            assert_eq!(
                html.matches(r#"aria-label="New terms""#).count(),
                1,
                "{html}"
            );
            let add = tag_before(&html, "button", r#"aria-label="New terms""#);
            let classes: Vec<&str> = add.split(['"', ' ']).collect();
            for class in ["h-8", "ml-auto", "border-gold/40"] {
                assert!(classes.contains(&class), "{class}: {add}");
            }
            assert!(!classes.contains(&"border-line"), "Accent, not Chip: {add}");
        }
    }

    /// Just a '+', named 'New terms' in the language of the page; a click
    /// flips `aria-expanded` and opens the picker under it.
    #[test]
    fn the_button_is_a_plus_named_in_the_language() {
        for (language, name) in [
            (Language::En, "New terms"),
            (Language::Fr, "Nouveaux termes"),
            (Language::Vi, "Thuật ngữ mới"),
        ] {
            let html = view_of(language, HomeState::Due(2), 10, [0; 6]);
            let add = tag_before(&html, "button", &format!(r#"aria-label="{name}""#));
            assert!(add.contains(&format!(r#"title="{name}""#)), "{add}");
            let from = html.find(&add).expect(&html) + add.len();
            let inner = &html[from..from + html[from..].find("</button>").expect(&html)];
            assert!(inner.contains(r#"leading-none">+</span>"#), "{inner}");
            assert!(inner.contains("text-lg"), "{inner}");
            assert!(!inner.contains(name), "the name is the label only: {inner}");
        }
    }

    /// The button's label is 'New terms', not 'Add new terms' (user,
    /// 2026-10-06).
    #[test]
    fn the_add_terms_button_reads_new_terms() {
        let html = en(HomeState::Due(2), 10);
        assert!(html.contains(r#"aria-label="New terms""#), "{html}");
        assert!(!html.contains("Add new terms"), "{html}");
    }

    /// The '+' opens a dialog titled 'New terms': the picker in its body,
    /// 'Add' (Primary) and 'Cancel' (Ghost) in its footer; no inline panel.
    #[test]
    fn the_plus_opens_a_dialog_with_the_picker_and_add_and_cancel() {
        let html = en(HomeState::Due(2), 10);
        let add = tag_before(&html, "button", r#"aria-label="New terms""#);
        assert!(add.contains(r#"aria-controls="pc-new-terms""#), "{add}");
        let panel = tag_around(&html, r#"id="pc-new-terms""#);
        assert!(panel.contains(r#"role="dialog""#), "{panel}");
        let dialog = element_at(&html, at(&html, r#"id="pc-new-terms""#));
        assert!(dialog.contains(">New terms</h2>"), "{dialog}");
        assert!(dialog.contains(r#"role="radiogroup""#), "{dialog}");
        let footer = &dialog[at(dialog, "border-t border-line px-5 py-4")..];
        for label in ["+5", "+10", "Custom"] {
            assert!(
                at(dialog, label) < at(dialog, "border-t border-line px-5 py-4"),
                "{label}: {dialog}"
            );
        }
        let add = tag_before(footer, "button", ">Add<");
        assert!(add.contains("bg-primary"), "Primary: {add}");
        let cancel = tag_before(footer, "button", ">Cancel<");
        assert!(cancel.contains("underline"), "Ghost: {cancel}");
        assert_eq!(
            html.matches(">Add<").count(),
            1,
            "one Add, in the footer: {html}"
        );
        assert!(!html.contains("empty:hidden"), "no inline panel: {html}");
        let overlay = tag_before(&html, "div", r#"id="pc-new-terms""#);
        let overlay = &html[..html.find(&overlay).expect(&html)];
        let root = tag_before(overlay, "div", "fixed inset-0 z-50");
        assert!(has_attr(&root, "hidden"), "closed at first: {root}");
    }

    #[test]
    fn the_home_renders_no_footer_of_its_own() {
        assert!(!render(Language::En).contains("<footer"));
    }

    /// T1: the first paint is the skeleton in every language: busy, one
    /// disabled neutral button in the CTA box, no state label, no strip.
    #[test]
    fn the_first_paint_is_the_skeleton_in_every_language() {
        for (language, loading, labels) in [
            (
                Language::En,
                "Loading your progress…",
                [
                    "to review",
                    "Nothing due",
                    "Start with 5",
                    "Review",
                    "Your progress",
                ],
            ),
            (
                Language::Fr,
                "Chargement de votre progression…",
                [
                    "à revoir",
                    "Rien à revoir",
                    "Commencer",
                    "Revoir",
                    "Votre progression",
                ],
            ),
            (
                Language::Vi,
                "Đang tải tiến độ của bạn…",
                [
                    "cần ôn",
                    "Hôm nay không",
                    "Bắt đầu với",
                    "Ôn ",
                    "Tiến độ của bạn",
                ],
            ),
        ] {
            let html = render(language);
            assert!(html.contains(r#"aria-busy="true""#), "{html}");
            assert!(html.contains("data-cta-box"), "{html}");
            let button = tag_before(&html, "button", loading);
            assert!(has_attr(&button, "disabled"), "{button}");
            for label in labels {
                assert!(!html.contains(label), "{language:?} {label}: {html}");
            }
            assert!(!html.contains("<ul"), "no strip: {html}");
            assert!(!html.contains(r#"href="/review""#), "{html}");
        }
    }

    // ---- T3: the states ----

    /// Nothing due, nothing started: no CTA and no
    /// date, the row holds the '+' alone, small and at the right.
    #[test]
    fn nothing_to_review_and_nothing_started_has_the_plus_alone_on_the_row() {
        let html = en(HomeState::NothingDue, 10);
        let row = element_at(&html, at(&html, "data-cta-row"));
        assert_eq!(row.matches("<button").count(), 1, "{row}");
        assert!(!row.contains("Next review"), "{row}");
        assert!(!row.contains("<p"), "{row}");
        let add = tag_before(row, "button", r#"aria-label="New terms""#);
        let classes: Vec<&str> = add.split(['"', ' ']).collect();
        for class in ["h-8", "ml-auto"] {
            assert!(classes.contains(&class), "{class}: {add}");
        }
        assert!(!classes.contains(&"h-11"), "{add}");
    }

    /// The Empty state: the teaching sentence, then 'New terms' at the same
    /// right-aligned place, small, then its picker (user, 2026-10-06: no
    /// 'Start with 5 terms', the picker is the way in); no CTA row.
    #[test]
    fn the_empty_state_teaches_and_offers_new_terms_at_the_right() {
        let html = en(HomeState::Empty, 50);
        assert!(!html.contains(r#"aria-busy="true""#), "{html}");
        let teach = at(&html, "A term is one word, asked four ways");
        assert!(!html.contains("Start with 5"), "{html}");
        let row = element_at(&html, at(&html, "data-cta-row"));
        assert_eq!(row.matches("<button").count(), 1, "the '+' alone: {row}");
        assert!(teach < at(&html, "data-cta-row"), "{html}");
        let add = tag_before(&html, "button", r#"aria-label="New terms""#);
        assert!(!has_attr(&add, "disabled"), "{add}");
        assert!(add.contains(r#"aria-expanded="false""#), "{add}");
        assert!(add.contains(r#"aria-controls="pc-new-terms""#), "{add}");
        let classes: Vec<&str> = add.split(['"', ' ']).collect();
        for class in ["h-8", "ml-auto"] {
            assert!(classes.contains(&class), "{class}: {add}");
        }
        assert!(
            teach < at(&html, r#"aria-label="New terms""#)
                && at(&html, r#"aria-label="New terms""#) < at(&html, r#"id="pc-new-terms""#),
            "the sentence, the button, the picker: {html}"
        );
        assert!(!html.contains("<ul"), "no strip: {html}");
        assert!(!html.contains(r#"href="/progress"#), "{html}");
    }

    /// The `<button>` element whose content holds `text`, whole.
    fn button_with<'a>(html: &'a str, text: &str) -> &'a str {
        let at = at(html, text);
        let start = html[..at].rfind("<button").expect(html);
        &html[start..at + html[at..].find("</button>").expect(html)]
    }

    /// Due cards offer one Practice (user, 2026-10-06: « on resume s'il
    /// reste des cards, sinon practice »): Primary, Sm (the '+' height),
    /// '▶ Practice · N', named 'Practice, N cards due'; no slider (the due
    /// cards are the batch).
    #[test]
    fn cards_due_offer_practice_with_the_count_and_no_slider() {
        for n in [1, 12] {
            let html = view_of(Language::En, HomeState::Due(n), 40, STARTED);
            assert!(!html.contains(r#"type="range""#), "{html}");
            let resume = button_with(&html, ">Practice<");
            let tag = &resume[..=resume.find('>').expect(resume)];
            assert!(!has_attr(tag, "disabled"), "{tag}");
            let classes: Vec<&str> = tag.split(['"', ' ']).collect();
            for class in ["h-8", "bg-primary"] {
                assert!(classes.contains(&class), "{class}: {tag}");
            }
            assert!(!classes.contains(&"h-13"), "compact: {tag}");
            assert!(
                tag.contains(&format!(
                    r#"aria-label="Practice, {n} {}""#,
                    if n == 1 { "card due" } else { "cards due" }
                )),
                "{tag}"
            );
            let order = [
                at(resume, r#"aria-hidden="true">▶<"#),
                at(resume, ">Practice<"),
                at(resume, ">·<"),
                at(resume, &format!(">{n}<")),
            ];
            assert!(order.is_sorted(), "{order:?}: {resume}");
            assert!(!html.contains("/review"), "{html}");
        }
        let html = en(HomeState::Due(12), 40);
        let add = tag_before(&html, "button", "New terms");
        assert!(add.contains(r#"aria-expanded="false""#), "{add}");
        assert!(add.contains("aria-controls"), "{add}");
    }

    /// One card due reads in the singular in English and French; Vietnamese
    /// has one form.
    #[test]
    fn one_card_due_reads_in_the_singular() {
        for (language, resume, name) in [
            (Language::En, "Practice", "Practice, 1 card due"),
            (Language::Fr, "S’entraîner", "S’entraîner, 1 carte à revoir"),
            (Language::Vi, "Luyện tập", "Luyện tập, 1 thẻ cần ôn"),
        ] {
            let html = view_of(language, HomeState::Due(1), 10, [0; 6]);
            let button = button_with(&html, &format!(">{resume}<"));
            assert!(
                button.contains(&format!(r#"aria-label="{name}""#)),
                "{button}"
            );
        }
    }

    #[test]
    fn practice_reads_in_french_and_vietnamese() {
        for (language, resume, name) in [
            (
                Language::Fr,
                "S’entraîner",
                "S’entraîner, 3 cartes à revoir",
            ),
            (Language::Vi, "Luyện tập", "Luyện tập, 3 thẻ cần ôn"),
        ] {
            let html = view_of(language, HomeState::Due(3), 10, [0; 6]);
            let button = button_with(&html, &format!(">{resume}<"));
            assert!(
                button.contains(&format!(r#"aria-label="{name}""#)),
                "{button}"
            );
            assert!(button.contains(">3<"), "{button}");
        }
    }

    /// Nothing due, terms started: '▶ Practice' (compact, no count), named
    /// 'Practice ahead, 5 terms', then ui-lib's `Slider` at 5 (1 to the
    /// started terms, step 1, its value shown).
    #[test]
    fn nothing_due_offers_practice_ahead_with_the_slider_at_five() {
        let html = view_of(Language::En, NOTHING_DUE, 10, STARTED);
        let button = button_with(&html, ">Practice<");
        let tag = &button[..=button.find('>').expect(button)];
        assert!(tag.split(['"', ' ']).any(|c| c == "h-8"), "{tag}");
        assert!(tag.contains("bg-primary"), "{tag}");
        assert!(
            tag.contains(r#"aria-label="Practice ahead, 5 terms""#),
            "{tag}"
        );
        assert!(!button.contains(">·<"), "no count: {button}");
        let row = element_at(&html, at(&html, "data-cta-row"));
        let slider = tag_before(row, "input", r#"type="range""#);
        for attr in [
            r#"type="range""#,
            r#"min="1""#,
            r#"max="21""#,
            r#"step="1""#,
            r#"value="5""#,
            r#"aria-label="Terms to practise""#,
        ] {
            assert!(slider.contains(attr), "{attr}: {slider}");
        }
        assert!(
            row.contains(r#"<span class="tabular-nums text-sm text-heading">5</span>"#),
            "ui-lib's Slider shows its value: {row}"
        );
        assert!(
            at(row, ">Practice<") < at(row, r#"type="range""#)
                && at(row, r#"type="range""#) < at(row, r#"aria-label="New terms""#),
            "Practice, the slider, then the '+': {row}"
        );
    }

    /// Fewer started terms than the batch: the name counts what will be
    /// asked, and the slider stops at the started terms.
    #[test]
    fn the_batch_never_names_more_terms_than_are_started() {
        let html = view_of(Language::En, NOTHING_DUE, 10, [2, 1, 0, 0, 0, 0]);
        let button = button_with(&html, ">Practice<");
        assert!(
            button.contains(r#"aria-label="Practice ahead, 3 terms""#),
            "{button}"
        );
        let slider = tag_before(&html, "input", r#"type="range""#);
        assert!(slider.contains(r#"max="3""#), "{slider}");
    }

    #[test]
    fn practice_ahead_reads_in_french_and_vietnamese() {
        for (language, label, name) in [
            (
                Language::Fr,
                ">S’entraîner<",
                "S’entraîner en avance, 5 termes",
            ),
            (Language::Vi, ">Luyện tập<", "Luyện tập trước, 5 thuật ngữ"),
        ] {
            let html = view_of(language, NOTHING_DUE, 10, STARTED);
            let button = button_with(&html, label);
            assert!(
                button.contains(&format!(r#"aria-label="{name}""#)),
                "{button}"
            );
        }
    }

    /// Nothing due: 'Next review: <date>', muted, stays beside Practice,
    /// before the '+'.
    #[test]
    fn nothing_due_keeps_the_next_review_date_beside_practice() {
        for (language, text) in [
            (Language::En, "Next review: 5 October 2026"),
            (Language::Fr, "Prochaine révision : 5 octobre 2026"),
            (Language::Vi, "Lần ôn tới: ngày 5 tháng 10 năm 2026"),
        ] {
            let html = with_progress(language, NOTHING_DUE, None);
            let row = element_at(&html, at(&html, "data-cta-row"));
            let p = tag_before(row, "p", text);
            assert!(p.contains(r#"class="text-muted""#), "{p}");
            let order = [
                at(row, "bg-primary"),
                at(row, r#"type="range""#),
                at(row, text),
                at(row, r#"aria-controls="pc-new-terms""#),
            ];
            assert!(order.is_sorted(), "Practice, slider, date, '+': {row}");
        }
    }

    /// The CTA's click: due cards run the normal review; nothing due runs a
    /// practice ahead of the slider's batch (at most the started terms),
    /// limited to the stage selected in the strip.
    #[test]
    fn the_cta_asks_a_review_when_due_and_a_practice_ahead_otherwise() {
        assert_eq!(cta_action(CtaSlot::Practice(3)), Some(HomeAction::Review));
        Owner::new().with(|| {
            let (batch, stage) = (RwSignal::new(7), RwSignal::new(Some(Stage::Week)));
            let ahead = |started| {
                CtaSlot::Ahead(Ahead {
                    started,
                    batch,
                    stage,
                    next: None,
                })
            };
            assert_eq!(
                cta_action(ahead(21)),
                Some(HomeAction::PracticeAhead {
                    n: 7,
                    only: Some(Stage::Week)
                })
            );
            stage.set(None);
            assert_eq!(
                cta_action(ahead(4)),
                Some(HomeAction::PracticeAhead { n: 4, only: None }),
                "the batch stops at the started terms"
            );
        });
        assert_eq!(cta_action(CtaSlot::NextReview(today())), None);
    }

    /// `perform`: a review starts the normal session, a practice ahead
    /// starts the practice with its batch and stage, neither the other.
    #[test]
    fn perform_runs_the_review_or_the_practice_ahead() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let run = |action| {
            perform(
                action,
                || calls.borrow_mut().push("refresh".to_owned()),
                || calls.borrow_mut().push("review".to_owned()),
                |n, only| calls.borrow_mut().push(format!("ahead {n} {only:?}")),
            );
        };
        run(HomeAction::Review);
        run(HomeAction::PracticeAhead {
            n: 5,
            only: Some(Stage::Month),
        });
        assert_eq!(calls.into_inner(), ["review", "ahead 5 Some(Month)"]);
    }

    /// One Practice replaces Resume and 'Review the next stage' in every
    /// state and language.
    #[test]
    fn no_resume_and_no_next_stage_anywhere() {
        for language in [Language::En, Language::Fr, Language::Vi] {
            for (state, left) in every_state() {
                let html = view_of(language, state, left, STARTED);
                for gone in [
                    "Resume",
                    "Reprendre",
                    "Tiếp tục",
                    "Review the next stage",
                    "Revoir l’étape suivante",
                    "Ôn giai đoạn tiếp theo",
                ] {
                    assert!(!html.contains(gone), "{state:?} {gone}: {html}");
                }
            }
        }
    }

    #[test]
    fn the_new_terms_disclosure_opens_the_dialog() {
        let html = Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            let act = Callback::new(|_: HomeAction| {});
            let open = RwSignal::new(true);
            view! {
                <AddTermsButton new_left=40 open=open />
                <NewTermsDialog new_left=40 act=act open=open />
            }
            .to_html()
        });
        let add = tag_before(&html, "button", "New terms");
        assert!(add.contains(r#"aria-expanded="true""#), "{add}");
        let root = tag_before(&html, "div", "fixed inset-0 z-50");
        assert!(!has_attr(&root, "hidden"), "open: {root}");
        assert!(html.contains(r#"role="radiogroup""#), "{html}");
        for label in ["+5", "+10", "Custom"] {
            assert!(html.contains(label), "{label}: {html}");
        }
    }

    #[test]
    fn nothing_to_add_hides_the_new_terms_disclosure() {
        let html = en(HomeState::Due(3), 0);
        assert!(html.contains(">Practice<"), "{html}");
        assert!(!html.contains("New terms"), "{html}");
    }

    /// No term started (only terms the deck does not know): no Practice
    /// (the '+' is the way in), 'New terms' alone; with the deck exhausted
    /// too, nothing to press.
    #[test]
    fn nothing_started_offers_no_practice() {
        let none = NOTHING_DUE;
        let html = en(none, 5);
        for absent in [
            "Add 5 new terms",
            "Practice",
            r#"type="range""#,
            "All terms started",
        ] {
            assert!(!html.contains(absent), "{absent}: {html}");
        }
        tag_before(&html, "button", r#"aria-label="New terms""#);
        let html = en(none, 0);
        assert!(!html.contains("data-cta-row"), "{html}");
        assert!(!html.contains("New terms"), "{html}");
        assert!(!html.contains("All terms started"), "{html}");
    }

    #[test]
    fn an_unreadable_state_offers_the_raw_copy_before_starting_again() {
        let html = en(HomeState::Broken(BrokenKind::Invalid), 0);
        assert!(html.contains(r#"role="alert""#), "{html}");
        assert!(html.contains("could not be read: it is damaged"), "{html}");
        assert!(
            at(&html, "Download the raw copy") < at(&html, "Start again"),
            "{html}"
        );
        for label in ["Download the raw copy", "Start again"] {
            let button = tag_before(&html, "button", label);
            assert!(!has_attr(&button, "disabled"), "{button}");
        }
        for absent in [
            "Start with 5",
            "New terms",
            "Add 5 new terms",
            "Review",
            "<ul",
        ] {
            assert!(!html.contains(absent), "{absent}: {html}");
        }
    }

    #[test]
    fn a_newer_state_can_only_be_downloaded() {
        let html = en(HomeState::Broken(BrokenKind::NewerVersion), 0);
        assert!(html.contains("saved by a newer version"), "{html}");
        assert!(html.contains("Download the raw copy"), "{html}");
        assert!(!html.contains("Start again"), "never overwritten: {html}");
        assert!(!html.contains("New terms"), "{html}");
    }

    #[test]
    fn the_strip_has_six_cells_three_by_three_filtering_the_home_in_place() {
        let counts = [6, 5, 4, 3, 2, 1];
        for state in [HomeState::Due(2), HomeState::NothingDue] {
            let html = view_of(Language::En, state, 10, counts);
            assert_eq!(html.matches("<li").count(), 6, "{html}");
            let mut last = 0;
            for (label, count) in [
                ("Daily", 6),
                ("Weekly", 5),
                ("Monthly", 4),
                ("Quarterly", 3),
                ("Half-yearly", 2),
                ("Yearly", 1),
            ] {
                let tag = tag_before(&html, "button", &format!(">{label}<"));
                assert!(tag.contains(r#"aria-pressed="false""#), "{tag}");
                let cell = at(&html, &format!(">{label}<"));
                assert!(cell > last, "in order: {html}");
                last = cell;
                let counted = format!(
                    r#">{label}</span><span class="order-first text-2xl font-semibold text-heading">{count}</span>"#
                );
                assert!(
                    html.contains(&counted),
                    "the default size: {counted}: {html}"
                );
            }
            let strip = tag_before(&html, "ul", ">Daily<");
            assert!(strip.contains("grid-cols-3"), "{strip}");
            assert!(
                !strip.split(['"', ' ']).any(|c| c == "sm:flex"),
                "three by three: {strip}"
            );
            assert!(
                !html.contains("text-base font-semibold"),
                "not compact: {html}"
            );
            assert!(!html.contains("?stage="), "no URL: {html}");
            assert!(!html.contains("/progress"), "{html}");
        }
    }

    #[test]
    fn the_strip_follows_the_language() {
        let html = view_of(Language::Fr, HomeState::Due(2), 10, [0; 6]);
        for label in [
            "Quotidien",
            "Hebdomadaire",
            "Mensuel",
            "Trimestriel",
            "Semestriel",
            "Annuel",
        ] {
            assert!(html.contains(&format!(">{label}<")), "{label}: {html}");
        }
        assert!(!html.contains("cartes à revoir<"), "{html}");
        assert!(html.contains(">S’entraîner<"), "{html}");
    }

    #[test]
    fn every_state_reads_in_french_and_vietnamese() {
        for (language, want) in [
            (
                Language::Fr,
                [
                    r#"aria-label="Nouveaux termes""#,
                    ">S’entraîner<",
                    "Recommencer",
                ],
            ),
            (
                Language::Vi,
                [
                    r#"aria-label="Thuật ngữ mới""#,
                    ">Luyện tập<",
                    "Bắt đầu lại",
                ],
            ),
        ] {
            let empty = view_of(language, HomeState::Empty, 50, [0; 6]);
            assert!(empty.contains(want[0]), "{empty}");
            for gone in ["Commencer avec 5", "Bắt đầu với 5"] {
                assert!(!empty.contains(gone), "{gone}: {empty}");
            }
            let none = view_of(language, NOTHING_DUE, 0, STARTED);
            assert!(none.contains(want[1]), "{none}");
            let broken = view_of(language, HomeState::Broken(BrokenKind::Invalid), 0, [0; 6]);
            assert!(broken.contains(want[2]), "{broken}");
        }
    }

    /// Every state, with its terms left to start.
    fn every_state() -> [(HomeState, usize); 8] {
        [
            (HomeState::Loading, 0),
            (HomeState::Empty, 50),
            (HomeState::Due(1), 10),
            (HomeState::Due(12), 10),
            (HomeState::NothingDue, 0),
            (HomeState::NothingDue, 5),
            (HomeState::Broken(BrokenKind::Invalid), 0),
            (HomeState::Broken(BrokenKind::NewerVersion), 0),
        ]
    }

    /// The CTA box is no empty box: no minimum height, no centring; its
    /// content sits at the top.
    #[test]
    fn the_cta_box_has_no_empty_height() {
        for (state, left) in every_state() {
            let html = en(state, left);
            if !html.contains("data-cta-box") {
                continue;
            }
            let tag = tag_around(&html, "data-cta-box");
            for class in ["min-h-40", "justify-center"] {
                assert!(
                    !tag.split(['"', ' ']).any(|c| c == class),
                    "{state:?}: {class} in {tag}"
                );
            }
        }
    }

    /// The first paint's row has the loaded CTA row's height (a compact
    /// button's and the '+', `h-8`): the layout does not move when the
    /// store has been read. A state with nothing on the row has no row.
    #[test]
    fn the_cta_row_has_the_same_height_from_the_first_paint() {
        let rows: Vec<(HomeState, String)> = every_state()
            .into_iter()
            .filter_map(|(state, left)| {
                let html = en(state, left);
                html.contains("data-cta-row")
                    .then(|| (state, tag_around(&html, "data-cta-row").to_owned()))
            })
            .collect();
        // Every state but the broken ones and the two with nothing to do
        // (nothing started, nothing left to add).
        assert_eq!(rows.len(), 5, "{rows:?}");
        for (state, row) in &rows {
            let classes: Vec<&str> = row.split(['"', ' ']).collect();
            assert!(classes.contains(&"min-h-8"), "{state:?}: {row}");
            assert!(!classes.contains(&"min-h-13"), "{state:?}: {row}");
        }
        let skeleton = render(Language::En);
        let button = tag_before(&skeleton, "button", "Loading your progress");
        assert!(button.split(['"', ' ']).any(|c| c == "h-8"), "{button}");
    }

    // ---- HP: the progress on the home ----

    /// [`HomeView`] for `state`, with the progress fixture saved, filtered by
    /// `stage`.
    fn with_progress(language: Language, home: HomeState, stage: Option<Stage>) -> String {
        let saved = Saved {
            state: progress_fixture(),
            today: today(),
        };
        let counts = stage_counts(&saved.state, deck());
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                let stage = RwSignal::new(stage);
                view! { <HomeView state=home new_left=47 counts=counts saved=Some(saved) stage=stage /> }
                .to_html()
            })
            .replace("<!>", "")
    }

    /// The two states with started terms.
    fn started() -> [HomeState; 2] {
        [HomeState::Due(5), HomeState::NothingDue]
    }

    #[test]
    fn the_strip_cells_filter_the_list_on_the_home() {
        for state in started() {
            let html = with_progress(Language::En, state, None);
            for label in [
                "Daily",
                "Weekly",
                "Monthly",
                "Quarterly",
                "Half-yearly",
                "Yearly",
            ] {
                let cell = tag_before(&html, "button", &format!(">{label}<"));
                assert!(cell.contains(r#"aria-pressed="false""#), "{cell}");
            }
            assert!(!html.contains("?stage="), "no URL: {html}");
            assert!(!html.contains("/progress"), "{html}");
            assert!(!html.contains("All terms"), "no filter: {html}");
        }
    }

    #[test]
    fn the_home_lists_the_terms_in_a_region_of_about_five_rows() {
        for state in started() {
            let html = with_progress(Language::En, state, None);
            let region = tag_around(&html, "data-term-list");
            for class in [
                "max-h-[22rem]",
                "overflow-y-auto",
                "overscroll-contain",
                "pr-1",
            ] {
                assert!(
                    region.split(['"', ' ']).any(|c| c == class),
                    "{class}: {region}"
                );
            }
            assert!(!region.contains("max-h-[60vh]"), "{region}");
            assert!(!region.contains("sm:max-h-"), "every width: {region}");
            assert!(region.contains(r#"role="region""#), "{region}");
            assert!(region.contains(r#"tabindex="0""#), "{region}");
            assert!(region.contains(r#"aria-label="Your terms""#), "{region}");
            assert_eq!(
                html.matches(r#"aria-controls="pc-detail-"#).count(),
                3,
                "{html}"
            );
        }
    }

    #[test]
    fn under_the_cta_row_come_the_strip_the_list_the_orphans_then_the_data_block() {
        for state in started() {
            let html = with_progress(Language::En, state, None);
            let order = [
                at(&html, "data-cta-row"),
                at(&html, r#"aria-label="New terms""#),
                at(&html, ">Daily<"),
                at(&html, "data-term-list"),
                at(&html, "Not in this deck"),
                at(&html, ">Export<"),
            ];
            assert!(order.is_sorted(), "{order:?}: {html}");
            let export = button_labelled(&html, "Export");
            assert!(!export.contains("disabled"), "{export}");
            let import = button_labelled(&html, "Import");
            assert!(!import.contains("disabled"), "{import}");
        }
    }

    #[test]
    fn the_stage_in_force_filters_the_list_and_is_the_pressed_cell() {
        let html = with_progress(Language::En, HomeState::Due(5), Some(Stage::Week));
        assert_eq!(
            html.matches(r#"aria-controls="pc-detail-"#).count(),
            1,
            "{html}"
        );
        assert!(html.contains(">kho</span>"), "{html}");
        assert!(!html.contains(">ca</span>"), "{html}");
        let weekly = tag_before(&html, "button", ">Weekly<");
        assert!(weekly.contains(r#"aria-pressed="true""#), "{weekly}");
        assert_eq!(html.matches(r#"aria-pressed="true""#).count(), 1, "{html}");
        assert!(!html.contains("Stage: Weekly"), "the cell says it: {html}");
        assert!(!html.contains("All terms"), "the cell says it: {html}");
        assert!(
            html.contains("Not in this deck"),
            "a filter leaves the orphans listed: {html}"
        );
    }

    #[test]
    fn the_orphans_are_listed_apart_and_not_counted_on_the_home() {
        let html = with_progress(Language::En, HomeState::Due(5), None);
        assert_eq!(html.matches("ghost 1").count(), 1, "{html}");
        let quarter = &html[at(&html, ">Quarterly<")..];
        assert!(
            quarter.starts_with(">Quarterly</span><span class=\"order-first text-2xl font-semibold text-heading\">0<"),
            "{quarter}"
        );
    }

    #[test]
    fn the_home_has_no_due_now_line_and_one_way_to_the_review() {
        let html = with_progress(Language::En, HomeState::Due(5), None);
        assert!(!html.contains("due now"), "{html}");
        assert!(
            !html.contains("/review"),
            "the review runs in place: {html}"
        );
        assert_eq!(html.matches(">Practice<").count(), 1, "{html}");
        assert!(!html.contains("Your progress<"), "no progress link: {html}");
    }

    #[test]
    fn the_empty_home_offers_the_import_and_no_list() {
        let html = en(HomeState::Empty, 50);
        assert!(!html.contains("Your progress is on this device"), "{html}");
        let import = button_labelled(&html, "Import");
        assert!(
            !import.contains("disabled"),
            "import is the way back: {import}"
        );
        let export = button_labelled(&html, "Export");
        assert!(export.contains("disabled"), "nothing to export: {export}");
        assert!(!html.contains("data-term-list"), "{html}");
        assert!(!html.contains(">Daily<"), "no strip: {html}");
    }

    #[test]
    fn a_damaged_progress_can_be_replaced_by_an_import_from_the_home() {
        let html = en(HomeState::Broken(BrokenKind::Invalid), 0);
        assert!(
            at(&html, "Start again") < at(&html, ">Import<"),
            "the banner first: {html}"
        );
        let import = button_labelled(&html, "Import");
        assert!(!import.contains("disabled"), "{import}");
        assert!(html.contains(r#"type="file""#), "{html}");
    }

    #[test]
    fn a_newer_progress_offers_no_import() {
        let html = en(HomeState::Broken(BrokenKind::NewerVersion), 0);
        assert!(!html.contains(">Import<"), "{html}");
        assert!(!html.contains(">Export<"), "{html}");
        assert!(!html.contains(r#"type="file""#), "{html}");
    }

    #[test]
    fn the_progress_on_the_home_reads_in_french_and_vietnamese() {
        for language in [Language::Fr, Language::Vi] {
            let html = with_progress(language, HomeState::Due(5), None);
            let locale = Locale::from(language);
            for text in [
                td_string!(locale, progress_terms),
                td_string!(locale, orphans_heading),
                td_string!(locale, export),
                td_string!(locale, import),
            ] {
                assert!(html.contains(text), "{language:?} {text}: {html}");
            }
            for heading in ["Time until", "Temps avant", "Thời gian đến"] {
                assert!(!html.contains(heading), "no strip heading: {html}");
            }
        }
    }

    /// HC: the strip follows the row directly, with no heading.
    #[test]
    fn the_strip_has_no_heading() {
        for state in started() {
            let html = with_progress(Language::En, state, None);
            assert!(!html.contains("Time until the next review"), "{html}");
            let strip = &html[at(&html, "<section")..at(&html, ">Daily<")];
            assert!(!strip.contains("<h2"), "{strip}");
        }
    }

    // ---- HD: the review in place ----

    /// [`HomeScreen`] at home (a due model) or in a session on `kho`.
    fn screen(session: bool) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(Language::En);
                let mode = if session {
                    let run = RwSignal::new(run_on("kho", &[], Stage::Day));
                    Mode::Session(Phase::Running(run))
                } else {
                    Mode::Home
                };
                let model = Model {
                    state: HomeState::Due(3),
                    new_left: 47,
                    counts: [1, 0, 0, 0, 0, 0],
                    saved: None,
                };
                let none = Callback::new(|()| ());
                view! {
                    <HomeScreen
                        mode=mode
                        model=model
                        stage=RwSignal::new(None)
                        batch=RwSignal::new(5)
                        on_action=Callback::new(|_| ())
                        paused=Signal::stored(false)
                        on_reload=none
                        on_home=none
                    />
                }
                .to_html()
            })
            .replace("<!>", "")
    }

    #[test]
    fn in_a_session_the_page_shows_the_session_alone_under_the_brand() {
        let html = screen(true);
        assert!(html.contains("0 / 3 cards"), "the session header: {html}");
        assert!(html.contains("Show the answer"), "the card: {html}");
        assert!(html.contains("data-review"), "{html}");
        assert!(!html.contains("<h1"), "{html}");
        assert!(!html.contains("data-cta-row"), "{html}");
        assert!(!html.contains(">Daily<"), "no strip: {html}");
        assert!(!html.contains("Your progress is on this device"), "{html}");
        let brand = tag_before(&html, "a", "PaliLearn");
        assert!(
            brand.contains(r#"href="/""#),
            "a middle click opens /: {brand}"
        );
        assert!(at(&html, "PaliLearn") < at(&html, "data-review"), "{html}");
    }

    #[test]
    fn at_home_the_page_shows_the_title_the_row_and_the_strip_and_no_session() {
        let html = screen(false);
        assert!(html.contains(">Learn Pāli</h1>"), "{html}");
        assert!(html.contains("data-cta-row"), "{html}");
        assert!(html.contains(">Daily<"), "{html}");
        assert!(!html.contains("data-review"), "{html}");
        assert!(at(&html, "PaliLearn") < at(&html, "<h1"), "{html}");
    }

    // ---- the pure parts ----

    fn today() -> Date {
        Date::parse("2026-10-05").expect("a date")
    }

    fn entry(term: &str, stage: Stage, due: Date) -> Progress {
        Progress {
            term: TermId(term.to_owned()),
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

    #[test]
    fn nothing_stored_is_the_empty_home_with_the_whole_deck_left() {
        let model = compute(&Load::Empty, deck(), today());
        assert_eq!(model.state, HomeState::Empty);
        assert_eq!(model.new_left, 50);
        assert_eq!(model.counts, [0; 6]);
    }

    #[test]
    fn a_broken_load_adds_nothing_and_shows_no_counts() {
        for (load, kind) in [
            (Load::Invalid { raw: "x".into() }, BrokenKind::Invalid),
            (
                Load::NewerVersion { raw: "x".into() },
                BrokenKind::NewerVersion,
            ),
        ] {
            let model = compute(&load, deck(), today());
            assert_eq!(model.state, HomeState::Broken(kind));
            assert_eq!((model.new_left, model.counts), (0, [0; 6]));
        }
    }

    #[test]
    fn the_counts_are_per_stage_and_leave_the_orphans_out() {
        let tomorrow = today() + 1;
        let state = state_of(vec![
            entry("kho", Stage::Day, today()),
            entry("hoti 1", Stage::Week, tomorrow),
            entry("evaṃ 1", Stage::Week, tomorrow),
            entry("gone", Stage::Year, tomorrow),
        ]);
        let model = compute(&Load::Ok(state), deck(), today());
        assert_eq!(model.counts, [1, 2, 0, 0, 0, 0]);
        assert_eq!(model.new_left, 47);
        assert_eq!(model.state, HomeState::Due(3));
    }

    #[test]
    fn a_browser_date_before_the_last_session_still_counts_as_that_day() {
        let mut state = state_of(vec![entry("kho", Stage::Day, today())]);
        state.last_today = Some(today() + 2);
        let model = compute(&Load::Ok(state), deck(), today());
        assert_eq!(model.state, HomeState::Due(3));
    }

    #[test]
    fn starting_adds_five_terms_and_stamps_the_day() {
        let state = grown(HomeAction::AddNew(5), State::empty(), deck(), today()).expect("a state");
        assert_eq!(state.progress.len(), 5);
        assert_eq!(state.last_today, Some(today()));
    }

    #[test]
    fn adding_n_terms_keeps_the_ones_already_started() {
        let first = grown(HomeAction::AddNew(5), State::empty(), deck(), today()).expect("five");
        let more = grown(HomeAction::AddNew(10), first.clone(), deck(), today()).expect("ten more");
        assert_eq!(more.progress.len(), 15);
        assert_eq!(&more.progress[..5], &first.progress[..]);
    }

    #[test]
    fn adding_more_than_what_is_left_adds_what_is_left() {
        let all = grown(HomeAction::AddNew(60), State::empty(), deck(), today()).expect("all");
        assert_eq!(all.progress.len(), 50);
        assert_eq!(grown(HomeAction::AddNew(5), all, deck(), today()), None);
    }

    #[test]
    fn the_other_actions_grow_nothing() {
        for action in [HomeAction::StartAgain, HomeAction::DownloadRaw] {
            assert_eq!(grown(action, State::empty(), deck(), today()), None);
        }
    }

    #[test]
    fn a_state_dated_after_today_is_not_grown() {
        let mut state = State::empty();
        state.last_today = Some(today() + 1);
        assert_eq!(grown(HomeAction::AddNew(5), state, deck(), today()), None);
    }
}
