//! `/`: the home. The brand (lotus and name, hidden on a phone where the
//! shell's top bar already shows them), the title for a screen reader only
//! (the brand names the page), then [`HomeView`]:
//! the first paint is a skeleton (`aria-busy`, a CTA row of the loaded
//! row's height);
//! after hydration the page reads the store ([`Home`]) and shows one of the
//! home's states (empty, due, nothing due, broken) in the same box.
//!
//! The home also holds the progress (there is no other page for it): under
//! the CTA row, the strip of the six stages, compact, whose cells filter the
//! term list in place (the page's state, no URL: lost on reload), the list
//! capped at about five rows, the
//! terms the deck does not know, then the data block (export, import).
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
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Disclosed, DisclosureButton, Size, Variant};

use crate::catalogue::{Locale, strings};
use crate::components::data_block::DataBlock;
use crate::components::new_terms::NewTerms;
use crate::components::session::{Phase, SessionView, start_phase};
use crate::components::term_list::{Orphans, StageStrip, StageTerms, orphan_rows};
use crate::deck::deck;
use crate::learn::home_state::{BrokenKind, HomeState, home_state, new_left};
use crate::learn::stages::stage_counts;
use crate::store::{self, Load, on_storage_change};
use crate::today::{browser_today, on_visible, session_today};

/// The CTA box of every state: its content at the top, no empty height.
const CTA_BOX: &str = "flex flex-col gap-4";

/// A state's wrapper: the box, then what sits under it.
const FRAME: &str = "flex flex-col gap-6";

/// The row of the CTA and 'Add new terms' (HC): side by side from `sm` up,
/// 'Add new terms' at the right; on a phone the full-width CTA pushes it
/// under, still at the right. At least an Lg button's height (`h-13`), with
/// no CTA in it or in the skeleton: the layout does not move when the store
/// has been read.
const CTA_ROW: &str = "flex flex-wrap items-center gap-3 min-h-13";

/// The CTA's layout: the full width on a phone, in the thumb's reach.
const CTA_LAYOUT: &str = "w-full justify-center sm:w-auto sm:self-start";

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
    /// Review what is due: a session on the stored progress, in place.
    Resume,
    /// Bring the next stage forward (nothing due).
    AdvanceBatch,
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
            class="flex flex-col gap-8 px-6 pt-6 pb-12 sm:mx-auto sm:w-full sm:max-w-[40rem]"
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
        let start = Callback::new(move |()| {
            paused.set(false);
            mode.set(Mode::Session(start_phase(&owner)));
        });
        let home = Callback::new(move |()| {
            refresh();
            mode.set(Mode::Home);
        });
        let on_action = Callback::new(move |action| perform(action, refresh, || start.run(())));
        Page {
            model,
            mode,
            paused,
            start,
            home,
            on_action,
            stage: RwSignal::new(None),
        }
    }
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

/// The state after `action` (new terms, or the next stage), to be saved;
/// `None` when it moved nothing (or the day is before the state's last).
fn grown(action: HomeAction, state: State, deck: &Deck, today: Date) -> Option<State> {
    let mut session = Session::start(deck, state, today).ok()?;
    let moved = match action {
        HomeAction::AddNew(n) => session.request_new(n),
        HomeAction::AdvanceBatch => session.advance(5),
        HomeAction::Resume
        | HomeAction::StartAgain
        | HomeAction::DownloadRaw
        | HomeAction::Replace(_) => 0,
    };
    (moved > 0).then(|| session.snapshot())
}

/// Does what the learner asked: grow the progress, save it and start a
/// session in place (`go`); or, when nothing moved, read the store again.
fn perform(action: HomeAction, refresh: impl Fn(), go: impl Fn()) {
    match action {
        HomeAction::Resume => go(),
        HomeAction::StartAgain => {
            store::save(&State::empty());
            refresh();
        }
        HomeAction::DownloadRaw => download_raw(),
        HomeAction::Replace(state) => {
            store::save(&state);
            refresh();
        }
        HomeAction::AddNew(_) | HomeAction::AdvanceBatch => {
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

/// What the home shows for `state`: the CTA row in a box of fixed height,
/// the new-terms picker under it, the progress (the strip, the list, the
/// data block), or the Broken banner.
#[allow(
    clippy::similar_names,
    reason = "`state` and `stage` are the design's names for the two props"
)]
#[component]
pub fn HomeView(
    /// The home's state ([`crate::learn::home_state::home_state`]).
    state: HomeState,
    /// The deck's terms not started yet
    /// ([`crate::learn::home_state::new_left`]): 0 disables the CTA when
    /// nothing is due nor to advance (Q4).
    new_left: usize,
    /// Started terms per stage, Day first (the strip); zeros by default.
    #[prop(optional)]
    counts: [usize; 6],
    /// The stored progress (the term list, the orphans, the export); none:
    /// the strip without a list.
    #[prop(default = None)]
    saved: Option<Saved>,
    /// The strip's filter: the list shows that stage only; `None`, every
    /// term. A filter of its own when not given.
    #[prop(optional)]
    stage: Option<RwSignal<Option<Stage>>>,
    /// Called with what the learner chose; [`Home`] performs it. Nothing
    /// happens without it.
    #[prop(optional, into)]
    on_action: Option<Callback<HomeAction>>,
) -> impl IntoView {
    let stage = stage.unwrap_or_else(|| RwSignal::new(None));
    let act = Callback::new(move |action| {
        if let Some(on_action) = on_action {
            on_action.run(action);
        }
    });
    match state {
        HomeState::Loading => view! { <Skeleton /> }.into_any(),
        HomeState::Empty => view! { <EmptyBlock new_left=new_left act=act /> }.into_any(),
        HomeState::Due(_) => view! {
            <DueBlock new_left=new_left act=act>
                <HomeProgress counts=counts saved=saved stage=stage act=act />
            </DueBlock>
        }
        .into_any(),
        HomeState::NothingDue { advance_possible } => view! {
            <NothingDueBlock advance_possible=advance_possible new_left=new_left act=act>
                <HomeProgress counts=counts saved=saved stage=stage act=act />
            </NothingDueBlock>
        }
        .into_any(),
        HomeState::Broken(kind) => view! { <BrokenBlock kind=kind act=act /> }.into_any(),
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
                    <Button
                        size=Size::Lg
                        layout=CTA_LAYOUT
                        disabled=Signal::derive(|| true)
                        on_click=|_| {}
                    >
                        {move || td_string!(t.locale(), home_loading)}
                    </Button>
                </div>
            </div>
        </div>
    }
}

/// No progress yet: the teaching sentence and a row whose only action is
/// 'Add new terms', the way in (at the left, the default size; user,
/// 2026-10-06), the picker under it, then the data block, whose import is
/// the way back to a saved progress.
#[component]
fn EmptyBlock(new_left: usize, act: Callback<HomeAction>) -> impl IntoView {
    let t = strings();
    let open = RwSignal::new(false);
    view! {
        <div data-home-state="empty" class=FRAME>
            <div data-cta-box="" class=CTA_BOX>
                <p class="max-w-prose text-body">{move || td_string!(t.locale(), home_teach)}</p>
                <CtaRow cta=None new_left=new_left act=act open=open />
            </div>
            <AddTermsPanel new_left=new_left act=act open=open />
            <ImportOnly act=act />
        </div>
    }
}

/// Cards due: the CTA row ('Resume', no count: the session's header gives
/// it), the picker under it, then `children` (the progress).
#[component]
fn DueBlock(new_left: usize, act: Callback<HomeAction>, children: Children) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <div data-home-state="due" class=FRAME>
            <div data-cta-box="" class=CTA_BOX>
                <CtaRow cta=Some(Cta::Resume) new_left=new_left act=act open=open />
            </div>
            <AddTermsPanel new_left=new_left act=act open=open />
            {children()}
        </div>
    }
}

/// Nothing due: the CTA row ('Review the next stage' when advancing would
/// move something, else 'Add new terms' alone at the left), the picker
/// under it, then `children` (the progress).
#[component]
fn NothingDueBlock(
    advance_possible: bool,
    new_left: usize,
    act: Callback<HomeAction>,
    children: Children,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let cta = advance_possible.then_some(Cta::NextStage);
    view! {
        <div data-home-state="nothing-due" class=FRAME>
            <div data-cta-box="" class=CTA_BOX>
                <CtaRow cta=cta new_left=new_left act=act open=open />
            </div>
            <AddTermsPanel new_left=new_left act=act open=open />
            {children()}
        </div>
    }
}

/// A state's call to action (HC: one button, no headline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cta {
    /// 'Resume' (Due).
    Resume,
    /// 'Review the next stage' (nothing due, a stage to advance).
    NextStage,
}

impl Cta {
    /// What it asks of the home.
    fn action(self) -> HomeAction {
        match self {
            Cta::Resume => HomeAction::Resume,
            Cta::NextStage => HomeAction::AdvanceBatch,
        }
    }

    /// Its label in `locale`.
    fn label(self, locale: Locale) -> &'static str {
        match self {
            Cta::Resume => td_string!(locale, cta_resume),
            Cta::NextStage => td_string!(locale, cta_next_stage),
        }
    }
}

/// The CTA row (HC): the state's CTA at the left when there is one, 'Add
/// new terms' small at the right; with no CTA, 'Add new terms' is the row's
/// only action and sits at the left at the default size (Empty, and nothing
/// due with nothing to advance).
#[component]
fn CtaRow(
    /// The state's CTA; none leaves 'Add new terms' alone.
    cta: Option<Cta>,
    /// The deck's terms not started yet; 0 hides 'Add new terms'.
    new_left: usize,
    /// Performs the CTA.
    act: Callback<HomeAction>,
    /// The picker's disclosure, shared with [`AddTermsPanel`].
    open: RwSignal<bool>,
) -> impl IntoView {
    let t = strings();
    let way_in = cta.is_none();
    view! {
        <div data-cta-row="" class=CTA_ROW>
            {cta
                .map(|cta| {
                    view! {
                        <Button
                            variant=Variant::Primary
                            size=Size::Lg
                            layout=CTA_LAYOUT
                            on_click=move |_| act.run(cta.action())
                        >
                            {move || cta.label(t.locale())}
                        </Button>
                    }
                })}
            <AddTermsButton new_left=new_left open=open way_in=way_in />
        </div>
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

/// 'Add new terms': the disclosure that opens [`AddTermsPanel`], small, at
/// the row's right (HC); as the way in, at the default size and at the
/// left. Nothing when the deck has no term left to start.
#[component]
fn AddTermsButton(
    /// The deck's terms not started yet; 0 renders nothing.
    new_left: usize,
    /// The disclosure's state, shared with [`AddTermsPanel`].
    open: RwSignal<bool>,
    /// The row's only action: the default size, at the left.
    #[prop(optional)]
    way_in: bool,
) -> impl IntoView {
    let t = strings();
    let (size, layout) = if way_in {
        (Size::Md, "")
    } else {
        (Size::Sm, "ml-auto")
    };
    (new_left > 0).then(|| {
        view! {
            <DisclosureButton controls=Disclosed::new("pc-new-terms", open) size=size layout=layout>
                {move || td_string!(t.locale(), add_terms)}
            </DisclosureButton>
        }
    })
}

/// The region [`AddTermsButton`] controls: [`NewTerms`] at the column's
/// full width, under the CTA row, while open. Empty (and taking no gap)
/// when closed; nothing when the deck has no term left to start.
#[component]
fn AddTermsPanel(
    /// The deck's terms not started yet; 0 renders nothing.
    new_left: usize,
    /// Performs the learner's choice.
    act: Callback<HomeAction>,
    /// The disclosure's state, shared with [`AddTermsButton`].
    open: RwSignal<bool>,
) -> impl IntoView {
    (new_left > 0).then(|| {
        view! {
            <div id="pc-new-terms" class="empty:hidden">
                {move || {
                    open.get()
                        .then(|| {
                            view! {
                                <NewTerms
                                    on_add=move |n| act.run(HomeAction::AddNew(n))
                                    new_left=new_left
                                />
                            }
                        })
                }}
            </div>
        }
    })
}

/// The progress under the CTA row: the strip (its cells filter the list
/// in place), the term list and the orphans when a progress is saved,
/// then the data block.
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

#[cfg(test)]
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

    /// HL: the page sits in one centred column from `sm` up; a phone keeps
    /// the full width and its gutter.
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
        ] {
            assert!(tag.split(['"', ' ']).any(|c| c == class), "{class}: {tag}");
        }
    }

    /// HC: no state has a headline; one button is enough.
    #[test]
    fn no_state_has_a_headline() {
        for state in [
            HomeState::Due(1),
            HomeState::Due(12),
            HomeState::NothingDue {
                advance_possible: true,
            },
            HomeState::NothingDue {
                advance_possible: false,
            },
        ] {
            let html = en(state, 10);
            for absent in ["to review", "Nothing due today"] {
                assert!(!html.contains(absent), "{state:?} {absent}: {html}");
            }
            let cta_box = element_at(&html, at(&html, "data-cta-box"));
            assert!(!cta_box.contains("<h2"), "{state:?}: {cta_box}");
        }
    }

    /// HC: one row, the state's CTA at the left when there is one, 'Add
    /// new terms' small (`Size::Sm`) at the right (`ml-auto`); the picker
    /// opens under the row, never inside it. Not the Empty state's row,
    /// where 'Add new terms' is the way in.
    #[test]
    fn one_row_holds_the_cta_and_add_new_terms_small_at_the_right() {
        for (state, cta) in [
            (HomeState::Due(12), Some(">Resume<")),
            (
                HomeState::NothingDue {
                    advance_possible: true,
                },
                Some("Review the next stage"),
            ),
        ] {
            let html = en(state, 10);
            let row = element_at(&html, at(&html, "data-cta-row"));
            assert!(row.contains("flex flex-wrap items-center gap-3"), "{row}");
            assert_eq!(
                row.matches("<button").count(),
                1 + usize::from(cta.is_some()),
                "{row}"
            );
            if let Some(cta) = cta {
                assert!(at(row, cta) < at(row, ">Add new terms<"), "{cta}: {row}");
            }
            let add = tag_before(row, "button", ">Add new terms<");
            for class in ["h-8", "ml-auto"] {
                assert!(add.split(['"', ' ']).any(|c| c == class), "{class}: {add}");
            }
            assert!(!row.contains(r#"id="pc-new-terms""#), "{row}");
            assert!(
                at(&html, r#"id="pc-new-terms""#) > at(&html, ">Add new terms<"),
                "{html}"
            );
        }
    }

    /// The closed picker's empty region takes no gap in the column.
    #[test]
    fn the_closed_picker_region_takes_no_room() {
        let html = en(HomeState::Due(2), 10);
        let tag = tag_before(&html, "div", r#"id="pc-new-terms""#);
        assert!(tag.contains("empty:hidden"), "{tag}");
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

    /// The row's only action sits at the left at the default size: 'Add new
    /// terms' alone, with nothing due and nothing to advance.
    #[test]
    fn nothing_to_review_leaves_add_new_terms_alone_at_the_left() {
        let html = en(
            HomeState::NothingDue {
                advance_possible: false,
            },
            10,
        );
        let row = element_at(&html, at(&html, "data-cta-row"));
        assert_eq!(row.matches("<button").count(), 1, "{row}");
        let add = tag_before(row, "button", ">Add new terms<");
        let classes: Vec<&str> = add.split(['"', ' ']).collect();
        for absent in ["ml-auto", "h-8"] {
            assert!(!classes.contains(&absent), "{absent}: {add}");
        }
        assert!(classes.contains(&"h-11"), "the default size: {add}");
    }

    /// The Empty state: the teaching sentence, then a row whose only action
    /// is 'Add new terms', at the left and at the default size (user,
    /// 2026-10-06: no 'Start with 5 terms', the picker is the way in).
    #[test]
    fn the_empty_state_teaches_and_offers_the_picker_alone() {
        let html = en(HomeState::Empty, 50);
        assert!(!html.contains(r#"aria-busy="true""#), "{html}");
        assert!(
            html.contains("A term is one word, asked four ways"),
            "{html}"
        );
        assert!(!html.contains("Start with 5"), "{html}");
        let row = element_at(&html, at(&html, "data-cta-row"));
        assert_eq!(row.matches("<button").count(), 1, "{row}");
        let add = tag_before(row, "button", ">Add new terms<");
        assert!(!has_attr(&add, "disabled"), "{add}");
        assert!(add.contains(r#"aria-expanded="false""#), "{add}");
        assert!(add.contains(r#"aria-controls="pc-new-terms""#), "{add}");
        let classes: Vec<&str> = add.split(['"', ' ']).collect();
        for absent in ["ml-auto", "h-8"] {
            assert!(!classes.contains(&absent), "{absent}: {add}");
        }
        assert!(classes.contains(&"h-11"), "the default size: {add}");
        assert!(
            at(&html, r#"id="pc-new-terms""#) > at(&html, ">Add new terms<"),
            "the picker under the row: {html}"
        );
        assert!(!html.contains("<ul"), "no strip: {html}");
        assert!(!html.contains(r#"href="/progress"#), "{html}");
    }

    /// HD: due cards offer 'Resume', no count (the session's header gives
    /// it), and the session starts in place.
    #[test]
    fn cards_due_offer_resume_without_a_count_and_new_terms() {
        for n in [1, 12] {
            let html = en(HomeState::Due(n), 40);
            let resume = tag_before(&html, "button", ">Resume<");
            assert!(!has_attr(&resume, "disabled"), "{resume}");
            assert!(!html.contains(&format!("Review {n}")), "{html}");
            assert!(!html.contains("cards"), "{html}");
            assert!(!html.contains("/review"), "{html}");
        }
        let html = en(HomeState::Due(12), 40);
        let add = tag_before(&html, "button", "Add new terms");
        assert!(add.contains(r#"aria-expanded="false""#), "{add}");
        assert!(add.contains("aria-controls"), "{add}");
        assert!(!html.contains("radiogroup"), "closed: {html}");
    }

    #[test]
    fn resume_reads_in_french_and_vietnamese() {
        for (language, resume) in [(Language::Fr, "Reprendre"), (Language::Vi, "Tiếp tục")] {
            let html = view_of(language, HomeState::Due(3), 10, [0; 6]);
            tag_before(&html, "button", &format!(">{resume}<"));
            let row = element_at(&html, at(&html, "data-cta-row"));
            assert!(
                !row.contains(">3") && !row.contains("3<"),
                "no count: {row}"
            );
        }
    }

    #[test]
    fn the_new_terms_disclosure_opens_the_picker_inline() {
        let html = Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            let act = Callback::new(|_: HomeAction| {});
            let open = RwSignal::new(true);
            view! {
                <AddTermsButton new_left=40 open=open />
                <AddTermsPanel new_left=40 act=act open=open />
            }
            .to_html()
        });
        let add = tag_before(&html, "button", "Add new terms");
        assert!(add.contains(r#"aria-expanded="true""#), "{add}");
        assert!(html.contains(r#"role="radiogroup""#), "{html}");
        for label in ["+5", "+10", "Custom"] {
            assert!(html.contains(label), "{label}: {html}");
        }
        assert!(!html.contains("New terms"), "no label, HC: {html}");
    }

    #[test]
    fn nothing_to_add_hides_the_new_terms_disclosure() {
        let html = en(HomeState::Due(3), 0);
        assert!(html.contains(">Resume<"), "{html}");
        assert!(!html.contains("Add new terms"), "{html}");
    }

    #[test]
    fn nothing_due_with_a_stage_to_advance_offers_the_next_stage() {
        let html = en(
            HomeState::NothingDue {
                advance_possible: true,
            },
            0,
        );
        assert!(!html.contains("Nothing due today"), "{html}");
        let button = tag_before(&html, "button", "Review the next stage");
        assert!(!has_attr(&button, "disabled"), "{button}");
        assert!(!html.contains(r#"href="/review""#), "{html}");
    }

    /// HC: nothing due and nothing to advance: no CTA, 'Add new terms'
    /// alone; with the deck exhausted too, nothing to press.
    #[test]
    fn nothing_due_and_nothing_to_advance_offers_no_cta() {
        let none = HomeState::NothingDue {
            advance_possible: false,
        };
        let html = en(none, 5);
        for absent in [
            "Add 5 new terms",
            "Review the next stage",
            "All terms started",
        ] {
            assert!(!html.contains(absent), "{absent}: {html}");
        }
        tag_before(&html, "button", ">Add new terms<");
        let html = en(none, 0);
        let row = element_at(&html, at(&html, "data-cta-row"));
        assert!(!row.contains("<button"), "{row}");
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
            "Add new terms",
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
        assert!(!html.contains("Add new terms"), "{html}");
    }

    #[test]
    fn the_strip_has_six_compact_cells_filtering_the_home_in_place() {
        let counts = [6, 5, 4, 3, 2, 1];
        for state in [
            HomeState::Due(2),
            HomeState::NothingDue {
                advance_possible: false,
            },
        ] {
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
                    r#">{label}</span><span class="order-first text-base font-semibold text-heading">{count}</span>"#
                );
                assert!(html.contains(&counted), "compact: {counted}: {html}");
            }
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
        assert!(!html.contains("cartes à revoir"), "{html}");
        assert!(html.contains(">Reprendre<"), "{html}");
    }

    #[test]
    fn every_state_reads_in_french_and_vietnamese() {
        for (language, want) in [
            (
                Language::Fr,
                [
                    ">Ajouter des termes<",
                    "Revoir l’étape suivante",
                    "Recommencer",
                ],
            ),
            (
                Language::Vi,
                [
                    ">Thêm thuật ngữ mới<",
                    "Ôn giai đoạn tiếp theo",
                    "Bắt đầu lại",
                ],
            ),
        ] {
            let empty = view_of(language, HomeState::Empty, 50, [0; 6]);
            assert!(empty.contains(want[0]), "{empty}");
            for gone in ["Commencer avec 5", "Bắt đầu với 5"] {
                assert!(!empty.contains(gone), "{gone}: {empty}");
            }
            let none = view_of(
                language,
                HomeState::NothingDue {
                    advance_possible: true,
                },
                0,
                [0; 6],
            );
            assert!(none.contains(want[1]), "{none}");
            let broken = view_of(language, HomeState::Broken(BrokenKind::Invalid), 0, [0; 6]);
            assert!(broken.contains(want[2]), "{broken}");
        }
    }

    /// Every state, with its terms left to start.
    fn every_state() -> [(HomeState, usize); 9] {
        [
            (HomeState::Loading, 0),
            (HomeState::Empty, 50),
            (HomeState::Due(1), 10),
            (HomeState::Due(12), 10),
            (
                HomeState::NothingDue {
                    advance_possible: true,
                },
                0,
            ),
            (
                HomeState::NothingDue {
                    advance_possible: false,
                },
                5,
            ),
            (
                HomeState::NothingDue {
                    advance_possible: false,
                },
                0,
            ),
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
            let tag = tag_around(&html, "data-cta-box");
            for class in ["min-h-40", "justify-center"] {
                assert!(
                    !tag.split(['"', ' ']).any(|c| c == class),
                    "{state:?}: {class} in {tag}"
                );
            }
        }
    }

    /// The first paint's row has the loaded row's height (an Lg button's,
    /// `h-13`), even with no CTA in it: the layout does not move when the
    /// store has been read.
    #[test]
    fn the_cta_row_has_the_same_height_from_the_first_paint() {
        let rows: Vec<(HomeState, String)> = every_state()
            .into_iter()
            .filter(|(state, _)| !matches!(state, HomeState::Broken(_)))
            .map(|(state, left)| {
                (
                    state,
                    tag_around(&en(state, left), "data-cta-row").to_owned(),
                )
            })
            .collect();
        assert_eq!(rows.len(), 7, "{rows:?}");
        for (state, row) in &rows {
            assert!(
                row.split(['"', ' ']).any(|c| c == "min-h-13"),
                "{state:?}: {row}"
            );
        }
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
        [
            HomeState::Due(5),
            HomeState::NothingDue {
                advance_possible: true,
            },
        ]
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
                at(&html, ">Daily<"),
                at(&html, "data-term-list"),
                at(&html, "Not in this deck"),
                at(&html, "Your progress is on this device"),
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
            quarter.starts_with(">Quarterly</span><span class=\"order-first text-base font-semibold text-heading\">0<"),
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
        assert_eq!(html.matches(">Resume<").count(), 1, "{html}");
        assert!(!html.contains("Your progress<"), "no progress link: {html}");
    }

    #[test]
    fn the_empty_home_offers_the_import_and_no_list() {
        let html = en(HomeState::Empty, 50);
        assert!(html.contains("Your progress is on this device"), "{html}");
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
                td_string!(locale, data_notice),
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
    fn advancing_brings_the_long_terms_forward() {
        let progress = deck()
            .terms
            .iter()
            .map(|term| entry(&term.id.0, Stage::Month, today() + 20))
            .collect();
        let state = grown(
            HomeAction::AdvanceBatch,
            state_of(progress),
            deck(),
            today(),
        )
        .expect("five brought forward");
        let due = state.progress.iter().filter(|p| p.due <= today()).count();
        assert_eq!(due, 5);
    }

    #[test]
    fn advancing_with_terms_left_to_start_moves_nothing() {
        let state = state_of(vec![entry("kho", Stage::Month, today() + 20)]);
        assert_eq!(
            grown(HomeAction::AdvanceBatch, state, deck(), today()),
            None
        );
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
