//! The Learn fragment in the app's mode, in a real browser (task 048): built
//! with `csr` alone (no `web-store`), mounted by ui-lib's `csr::mount` under
//! a `ParentRoute` at `/learn` with a fake store on a seeded state. The home
//! appears at `/learn` and at `/learn/`, review answers are given and the
//! store saw one save each, and `/learn/nope` is Not found.
//!
//! `csr::mount` leaks its tree and mounts into `<body>`: each test gives its
//! app a distinct root id and looks only inside it.
//!
//! Run with `wasm-bindgen-test-runner` and a headless Chrome:
//! `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
//! CHROMEDRIVER=<chromedriver> cargo test --locked -p learn_pali --features
//! csr --target wasm32-unknown-unknown --test csr`.

#![cfg(all(target_arch = "wasm32", feature = "csr"))]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use learn_core::date::Date;
use learn_core::deck::{Deck, TermId};
use learn_core::progress::State;
use learn_core::session::Session;
use learn_pali::fragment::{
    FileTransfer, LearnProvider, LearnRoutes, LearnSettings, Load, PickFailed, ProgressStore,
    SaveError, SaveStatus, SharedFiles, SharedStore, SharedSync, SyncAccount, SyncError,
    SyncTransport,
};
use leptos::prelude::*;
use leptos_router::components::{Outlet, ParentRoute, Router, Routes};
use leptos_router::path;
use palicanon_ui_lib::{Language, csr};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// A store's `on_change` handler.
type Handler = Box<dyn Fn()>;

/// The progress the fake store holds and every state it was given to save.
#[derive(Clone)]
struct BrowserStore {
    load: Rc<RefCell<Load>>,
    saved: Rc<RefCell<Vec<State>>>,
    /// The status the store reports; the tests move it.
    status: RwSignal<SaveStatus>,
    /// The fragment's `on_change` handlers.
    callbacks: Rc<RefCell<Vec<Handler>>>,
}

impl BrowserStore {
    /// The progress changed outside a save (another tab): `load` answers
    /// `state`, then the fragment's handlers run.
    fn changed_to(&self, state: State) {
        *self.load.borrow_mut() = Load::Ok(state);
        for callback in self.callbacks.borrow().iter() {
            callback();
        }
    }
}

impl ProgressStore for BrowserStore {
    fn load(&self) -> Load {
        self.load.borrow().clone()
    }

    fn save(&self, state: &State) {
        self.saved.borrow_mut().push(state.clone());
        *self.load.borrow_mut() = Load::Ok(state.clone());
    }

    fn status(&self) -> Signal<SaveStatus> {
        self.status.into()
    }

    fn on_change(&self, callback: Box<dyn Fn()>) {
        self.callbacks.borrow_mut().push(callback);
    }
}

/// No file ever moves in this test.
struct NoFiles;

impl FileTransfer for NoFiles {
    fn export(&self, _name: &str, _text: &str) {}

    fn pick_import(&self, _on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {}
}

/// Two terms started on a day long past, so they are due whatever today is.
fn seeded() -> State {
    seeded_with(2)
}

/// `n` terms started on a day long past, so they are due whatever today is.
fn seeded_with(n: usize) -> State {
    seeded_on(n, "2000-01-01")
}

/// `n` terms started on `day`.
fn seeded_on(n: usize, day: &str) -> State {
    let deck = Deck::from_json(include_str!("../../core/fixtures/deck.json")).expect("the deck");
    let day = Date::parse(day).expect("a date");
    let mut session = Session::start(&deck, State::empty(), day, 0).expect("a session");
    assert_eq!(session.request_new(n), n);
    session.into_state()
}

/// The first element matching `selector` inside the app of `root`.
fn first(root: &str, selector: &str) -> Option<web_sys::Element> {
    let root = document().get_element_by_id(root).expect("the app's root");
    root.query_selector(selector).expect("a selector")
}

/// The first `<button>` of `root` whose text contains `label`.
fn button(root: &str, label: &str) -> Option<web_sys::HtmlElement> {
    let root = document().get_element_by_id(root).expect("the app's root");
    let buttons = root.query_selector_all("button").expect("buttons");
    (0..buttons.length())
        .filter_map(|at| buttons.item(at))
        .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        .find(|button| {
            button
                .text_content()
                .is_some_and(|text| text.contains(label))
        })
}

/// Lets the effects and the events of the page run.
async fn settle() {
    for _ in 0..4 {
        leptos::task::tick().await;
    }
}

/// The page is moved to `url`, as the app's router would have it.
fn go_to(url: &str) {
    window()
        .history()
        .expect("history")
        .push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(url))
        .expect("pushState");
}

/// A fake store on the seeded state, status `Saved`.
fn seeded_store() -> BrowserStore {
    store_on(seeded())
}

/// A fake store holding `state`, status `Saved`.
fn store_on(state: State) -> BrowserStore {
    BrowserStore {
        load: Rc::new(RefCell::new(Load::Ok(state))),
        saved: Rc::default(),
        status: RwSignal::new(SaveStatus::Saved),
        callbacks: Rc::default(),
    }
}

/// The fragment mounted at `url` in an app of id `root`, over a fake store
/// on the seeded state, under a `ParentRoute` at `/learn`; returns the store
/// and a signal that takes the router (the page) away and brings it back,
/// the provider staying where it is.
fn mount_at(root: &'static str, url: &str) -> (BrowserStore, RwSignal<bool>) {
    mount_on(root, url, seeded_store())
}

/// [`mount_at`] over `store`.
fn mount_on(root: &'static str, url: &str, store: BrowserStore) -> (BrowserStore, RwSignal<bool>) {
    let (store, page, _) = mount_in(root, url, store, None);
    (store, page)
}

/// [`mount_on`] in `language` when given; the language signal comes back
/// (the choice is kept in a cookie: a test that sets one puts English back).
fn mount_in(
    root: &'static str,
    url: &str,
    store: BrowserStore,
    language: Option<Language>,
) -> (BrowserStore, RwSignal<bool>, RwSignal<Language>) {
    go_to(url);
    let page = RwSignal::new(true);
    let chosen = Rc::new(Cell::new(None));
    let seen = Rc::clone(&chosen);
    let (shared, files): (SharedStore, SharedFiles) = (Rc::new(store.clone()), Rc::new(NoFiles));
    csr::mount(move || {
        let signal = palicanon_ui_lib::provide_language();
        if let Some(language) = language {
            signal.set(language);
        }
        seen.set(Some(signal));
        // The provider sits above the router: the router's children closure
        // is `Send`, and a store (an `Rc`) is not.
        view! {
            <div id=root>
                <LearnProvider store=shared files=files>
                    {move || {
                        page.get()
                            .then(|| {
                                view! {
                                    <Router>
                                        <Routes fallback=|| "Not found.">
                                            <ParentRoute
                                                path=path!("/learn")
                                                view=|| view! { <Outlet /> }
                                            >
                                                <LearnRoutes />
                                            </ParentRoute>
                                        </Routes>
                                    </Router>
                                }
                            })
                    }}
                </LearnProvider>
            </div>
        }
    });
    (store, page, chosen.get().expect("the app was mounted"))
}

/// The fragment at the root of the app (no parent route), at `url`.
fn mount_at_root(root: &'static str, url: &str) {
    go_to(url);
    let store = seeded_store();
    let (shared, files): (SharedStore, SharedFiles) = (Rc::new(store), Rc::new(NoFiles));
    csr::mount(move || {
        palicanon_ui_lib::provide_language();
        view! {
            <div id=root>
                <LearnProvider store=shared files=files>
                    <Router>
                        <Routes fallback=|| "Not found.">
                            <LearnRoutes />
                        </Routes>
                    </Router>
                </LearnProvider>
            </div>
        }
    });
}

/// The fragment at `/learn`: the home shows, the brand links to `/learn`,
/// review answers are given and the fake store saw one save each.
#[wasm_bindgen_test]
async fn the_home_runs_a_review_under_learn_and_saves_each_answer() {
    let (store, _page) = mount_at("csr-learn", "/learn");
    settle().await;

    assert!(
        first("csr-learn", r#"[data-page="home"]"#).is_some(),
        "the home appears"
    );
    let brand = first("csr-learn", r#"a[href="/learn"]"#).expect("the brand links to /learn");
    assert_eq!(
        brand.text_content().as_deref(),
        Some("PaliLearn"),
        "the link around the name"
    );
    assert_eq!(store.saved.borrow().len(), 0, "mounting saves nothing");

    button("csr-learn", "Practice")
        .expect("the Practice button")
        .click();
    settle().await;
    assert!(
        button("csr-learn", "Show the answer").is_some(),
        "a card is asked"
    );
    assert_eq!(
        store.saved.borrow().len(),
        0,
        "starting a session saves nothing"
    );
    for answers in 1..=2 {
        button("csr-learn", "Show the answer")
            .expect("the card")
            .click();
        settle().await;
        button("csr-learn", "Knew it").expect("the grade").click();
        settle().await;
        assert_eq!(store.saved.borrow().len(), answers, "one answer, one save");
    }
}

/// Opens, one after another, the words of the revealed card until a panel
/// offers 'Add to practice'; the button, or `None` when no word offers it.
async fn open_a_word_offering_the_add(root: &str) -> Option<web_sys::HtmlElement> {
    open_a_word_offering(root, "Add to practice").await
}

/// [`open_a_word_offering_the_add`] with the button named `label`.
async fn open_a_word_offering(root: &str, label: &str) -> Option<web_sys::HtmlElement> {
    let words = document()
        .get_element_by_id(root)?
        .query_selector_all(r#"button[aria-controls^="pc-"]"#)
        .ok()?;
    for at in 0..words.length() {
        let word = words.item(at)?.dyn_into::<web_sys::HtmlElement>().ok()?;
        word.click();
        settle().await;
        if let Some(found) = button(root, label) {
            return Some(found);
        }
    }
    None
}

/// What the save of one press holds, whichever word the card offered first:
/// the word started is the last started term, and it is kept among the added
/// words exactly when the deck does not teach it (a deck term not started is
/// only started). The term and how many words are added.
fn word_pressed(state: &State) -> (TermId, usize) {
    let deck = Deck::from_json(include_str!("../../core/fixtures/deck.json")).expect("the deck");
    let id = state.progress.last().expect("a started term").term.clone();
    let outside = deck.term(&id).is_none();
    assert_eq!(state.added.len(), usize::from(outside), "{id:?}");
    if outside {
        assert_eq!(state.added[0].id, id);
    }
    (id, state.added.len())
}

/// A word of the revealed example is added from its panel (task 054): the
/// button turns into the 'Added' line in the open panel, the store saw the
/// word at once, started, with the two started terms untouched, and the
/// next answer's save keeps it.
#[wasm_bindgen_test]
async fn a_word_of_the_example_is_added_to_the_practice_from_its_panel() {
    let (store, _page) = mount_at("csr-learn-add", "/learn");
    settle().await;
    button("csr-learn-add", "Practice")
        .expect("the Practice button")
        .click();
    settle().await;
    button("csr-learn-add", "Show the answer")
        .expect("the card")
        .click();
    settle().await;
    assert_eq!(store.saved.borrow().len(), 0, "nothing saved yet");

    let add = open_a_word_offering_the_add("csr-learn-add")
        .await
        .expect("a word of the example offers the button");
    add.click();
    settle().await;

    // Another entry of the same word may still offer its own button.
    assert!(!add.is_connected(), "the pressed button is gone");
    let root = document().get_element_by_id("csr-learn-add").expect("root");
    let lines = root
        .query_selector_all(r#"[role="status"]"#)
        .expect("a selector");
    let added = (0..lines.length())
        .filter_map(|at| lines.item(at))
        .filter(|line| line.text_content().as_deref() == Some("Added"))
        .count();
    assert_eq!(added, 1, "one 'Added' line, in the panel that stayed open");
    let kept = {
        let saved = store.saved.borrow();
        assert_eq!(saved.len(), 1, "the word is saved at once");
        assert_eq!(saved[0].progress.len(), 3, "the word is started");
        assert_eq!(saved[0].progress[..2], seeded().progress[..]);
        word_pressed(&saved[0]).1
    };

    button("csr-learn-add", "Knew it")
        .expect("the grade")
        .click();
    settle().await;
    let saved = store.saved.borrow();
    assert_eq!(saved.len(), 2, "the answer saves too");
    assert_eq!(saved[1].added.len(), kept, "and keeps the word");
    assert_eq!(saved[1].progress.len(), 3, "started still");
}

/// `/learn/` (a trailing slash) shows the home too, linking to `/learn`.
#[wasm_bindgen_test]
async fn a_trailing_slash_shows_the_home() {
    let (_store, _page) = mount_at("csr-learn-slash", "/learn/");
    settle().await;
    assert!(first("csr-learn-slash", r#"[data-page="home"]"#).is_some());
    assert!(first("csr-learn-slash", r#"a[href="/learn"]"#).is_some());
}

/// Under `/learn` the router's fallback answers an unknown path: Not found,
/// no home.
#[wasm_bindgen_test]
async fn an_unknown_path_under_learn_is_not_found() {
    let (_store, _page) = mount_at("csr-learn-nope", "/learn/nope");
    settle().await;
    let root = document()
        .get_element_by_id("csr-learn-nope")
        .expect("the root");
    assert!(
        root.text_content()
            .is_some_and(|text| text.contains("Not found."))
    );
    assert!(first("csr-learn-nope", "[data-page]").is_none(), "no home");
}

/// At the root (no parent route) the brand links to `/`.
#[wasm_bindgen_test]
async fn at_the_root_the_brand_links_to_slash() {
    mount_at_root("csr-learn-root", "/");
    settle().await;
    assert!(first("csr-learn-root", r#"[data-page="home"]"#).is_some());
    assert!(
        first("csr-learn-root", r#"a[href="/"]"#).is_some(),
        "the brand links to /"
    );
    assert!(first("csr-learn-root", r#"a[href="/learn"]"#).is_none());
}

/// The strip 'not saved' lives with the provider, not with the page: after a
/// failure, leaving the page, a write in flight (Pending) and coming back,
/// it is still there; a save that succeeds takes it away.
#[wasm_bindgen_test]
async fn the_strip_stays_until_a_save_succeeds_across_a_leave_and_return() {
    let (store, page) = mount_at("csr-learn-strip", "/learn");
    settle().await;
    let strip = || first("csr-learn-strip", "[data-save-strip]");
    assert!(strip().is_none(), "saved: no strip");

    store.status.set(SaveStatus::Failed(SaveError::Transient));
    settle().await;
    assert!(strip().is_some(), "failed: the strip");

    page.set(false);
    settle().await;
    assert!(strip().is_none(), "the page is gone");
    store.status.set(SaveStatus::Pending);
    settle().await;
    page.set(true);
    settle().await;
    assert!(strip().is_some(), "back after Pending: still there");

    store.status.set(SaveStatus::Saved);
    settle().await;
    assert!(strip().is_none(), "a save succeeded: gone");
    // A failure that comes and goes while no page is mounted is kept too.
    page.set(false);
    settle().await;
    store.status.set(SaveStatus::Failed(SaveError::Transient));
    settle().await;
    store.status.set(SaveStatus::Pending);
    settle().await;
    page.set(true);
    settle().await;
    assert!(strip().is_some(), "failed while away, Pending, back: kept");
}

/// A sync transport that counts `prepare` (task 050, round 1, S4): the sync
/// UI readies Google's script where a tap may follow, and nowhere else.
#[derive(Clone)]
struct CountingSync {
    prepared: Rc<Cell<usize>>,
    account: RwSignal<SyncAccount>,
}

impl SyncTransport for CountingSync {
    fn sign_in(&self, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}

    fn sign_out(&self) {}

    fn account(&self) -> Signal<SyncAccount> {
        self.account.into()
    }

    fn pull(&self, _on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {}

    fn push(&self, _text: &str, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}

    fn prepare(&self) {
        self.prepared.set(self.prepared.get() + 1);
    }
}

/// The fragment at `/learn` with a counting sync for `account`, Settings'
/// section shown when `settings` (no shell: shown at once).
fn mount_with_sync(root: &'static str, account: SyncAccount, settings: bool) -> CountingSync {
    let transport = CountingSync {
        prepared: Rc::default(),
        account: RwSignal::new(account),
    };
    mount_sync(root, Rc::new(transport.clone()), settings);
    transport
}

/// The fragment at `/learn` over `sync`, Settings' section shown when
/// `settings`.
fn mount_sync(root: &'static str, sync: SharedSync, settings: bool) {
    mount_sync_on(root, sync, settings, seeded_store());
}

/// [`mount_sync`] over `store`.
fn mount_sync_on(root: &'static str, sync: SharedSync, settings: bool, store: BrowserStore) {
    go_to("/learn");
    let (shared, files): (SharedStore, SharedFiles) = (Rc::new(store), Rc::new(NoFiles));
    csr::mount(move || {
        palicanon_ui_lib::provide_language();
        view! {
            <div id=root>
                <LearnProvider store=shared files=files sync=Some(sync)>
                    {settings.then(|| view! { <LearnSettings /> })}
                    <Router>
                        <Routes fallback=|| "Not found.">
                            <ParentRoute path=path!("/learn") view=|| view! { <Outlet /> }>
                                <LearnRoutes />
                            </ParentRoute>
                        </Routes>
                    </Router>
                </LearnProvider>
            </div>
        }
    });
}

/// Signed out with Settings closed: no sync UI asks anything of Google.
/// Settings shown: its Sign in appears and Google's script is readied, so
/// the tap opens the window in its gesture. A remembered account: the
/// home's 'Sync paused' line with Resume appears and readies it too.
#[wasm_bindgen_test]
async fn the_sync_ui_readies_google_only_where_a_tap_may_follow() {
    let home = mount_with_sync("csr-sync-home", SyncAccount::SignedOut, false);
    settle().await;
    assert_eq!(home.prepared.get(), 0, "nothing before a tap may follow");
    assert!(
        button("csr-sync-home", "Resume").is_none(),
        "no paused line"
    );

    let settings = mount_with_sync("csr-sync-settings", SyncAccount::SignedOut, true);
    settle().await;
    assert!(button("csr-sync-settings", "Sync with Google Drive").is_some());
    assert!(settings.prepared.get() >= 1, "Settings shown: readied");

    let chip = mount_with_sync(
        "csr-sync-chip",
        SyncAccount::Remembered { email: None },
        false,
    );
    settle().await;
    assert!(
        button("csr-sync-chip", "Resume").is_some(),
        "the paused line's Resume"
    );
    assert!(chip.prepared.get() >= 1, "the Resume shown: readied");
}

/// A profile with practice behind it (ten terms started) is offered the add
/// like a fresh one (task 054, the user's own profile showed no button): the
/// word, a deck term not started or a word outside the deck, is started
/// beside the ten.
#[wasm_bindgen_test]
async fn a_profile_with_progress_is_offered_the_add_and_starts_the_word() {
    let (store, _page) = mount_on("csr-learn-ten", "/learn", store_on(seeded_with(10)));
    settle().await;
    button("csr-learn-ten", "Practice")
        .expect("the Practice button")
        .click();
    settle().await;
    button("csr-learn-ten", "Show the answer")
        .expect("the card")
        .click();
    settle().await;

    let add = open_a_word_offering_the_add("csr-learn-ten")
        .await
        .expect("a word of the example offers the button");
    add.click();
    settle().await;

    let text = document()
        .get_element_by_id("csr-learn-ten")
        .and_then(|root| root.text_content())
        .unwrap_or_default();
    assert!(!add.is_connected(), "the pressed button is gone");
    let saved = store.saved.borrow();
    assert_eq!(saved.len(), 1, "the word is saved at once: {text}");
    assert_eq!(saved[0].progress.len(), 11, "ten, and the word started");
    word_pressed(&saved[0]);
}

/// The first button of `root` named `label` by its `aria-label`.
fn labelled(root: &str, label: &str) -> Option<web_sys::HtmlElement> {
    first(root, &format!(r#"button[aria-label="{label}"]"#))?
        .dyn_into()
        .ok()
}

/// Presses 'Add to practice' on up to `n` words of the revealed example,
/// one panel after the other; how many were added.
async fn add_words(root: &str, n: usize) -> usize {
    let words = document()
        .get_element_by_id(root)
        .expect("root")
        .query_selector_all(r#"button[aria-controls^="pc-"]"#)
        .expect("words");
    let mut added = 0;
    for at in 0..words.length() {
        if added == n {
            break;
        }
        let word = words
            .item(at)
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .expect("a word");
        word.click();
        settle().await;
        if let Some(add) = button(root, "Add to practice") {
            add.click();
            settle().await;
            added += 1;
        }
    }
    added
}

/// The count shown on the home's tile `label` ('Daily'...).
fn tile(root: &str, label: &str) -> String {
    button(root, label)
        .and_then(|tile| tile.text_content())
        .unwrap_or_default()
}

/// Words added in a run are on the home when the learner leaves the run
/// (task 054, the user's check on dev): the Daily tile counts them, the list
/// shows them, the store holds them started.
#[wasm_bindgen_test]
async fn words_added_in_a_run_are_on_the_home_after_stopping() {
    stop_after_adding("csr-learn-back", seeded_with(10)).await;
}

/// The same in a practice ahead (nothing is due: the ten terms were started
/// on a day far ahead).
#[wasm_bindgen_test]
async fn words_added_in_a_practice_ahead_are_on_the_home_after_stopping() {
    stop_after_adding("csr-learn-ahead", seeded_on(10, "2099-01-01")).await;
}

/// Runs, adds words, stops, and checks the home against the store.
async fn stop_after_adding(root: &'static str, state: State) {
    let (store, _page) = mount_on(root, "/learn", store_on(state));
    settle().await;
    assert_eq!(tile(root, "Daily"), "Daily10");
    button(root, "Practice")
        .expect("the Practice button")
        .click();
    settle().await;
    button(root, "Show the answer").expect("the card").click();
    settle().await;
    let added = add_words(root, 3).await;
    assert!(added >= 2, "several words were added: {added}");

    labelled(root, "Stop the session")
        .expect("the close cross")
        .click();
    settle().await;
    button(root, "Stop").expect("the Stop button").click();
    settle().await;

    let stored = match &*store.load.borrow() {
        Load::Ok(state) => state.progress.len(),
        _ => 0,
    };
    assert_eq!(stored, 10 + added, "the store holds them started");
    assert_eq!(
        tile(root, "Daily"),
        format!("Daily{}", 10 + added),
        "the home's Daily tile counts them"
    );
    let rows = document()
        .get_element_by_id(root)
        .expect("root")
        .query_selector_all("[data-term-list] > ul > li")
        .expect("rows")
        .length();
    assert_eq!(rows as usize, 10 + added, "the list shows them");
}

/// A double click on 'Add to practice' (task 054, G3) ends on one 'Added'
/// line and one save: the second press finds the word added.
#[wasm_bindgen_test]
async fn a_double_click_on_the_add_saves_the_word_once() {
    let (store, _page) = mount_on("csr-learn-twice", "/learn", store_on(seeded_with(10)));
    settle().await;
    button("csr-learn-twice", "Practice")
        .expect("the Practice button")
        .click();
    settle().await;
    button("csr-learn-twice", "Show the answer")
        .expect("the card")
        .click();
    settle().await;
    let add = open_a_word_offering_the_add("csr-learn-twice")
        .await
        .expect("a word of the example offers the button");
    add.click();
    add.click();
    settle().await;

    assert_eq!(store.saved.borrow().len(), 1, "one save for two clicks");
    assert_eq!(store.saved.borrow()[0].progress.len(), 11);
    assert_eq!(added_lines("csr-learn-twice"), 1);
}

/// The 'Added' lines (`role="status"`, text `added`) under `root`.
fn added_lines_saying(root: &str, added: &str) -> usize {
    let lines = document()
        .get_element_by_id(root)
        .expect("root")
        .query_selector_all(r#"[role="status"]"#)
        .expect("a selector");
    (0..lines.length())
        .filter_map(|at| lines.item(at))
        .filter(|line| line.text_content().as_deref() == Some(added))
        .count()
}

/// The English 'Added' lines under `root`.
fn added_lines(root: &str) -> usize {
    added_lines_saying(root, "Added")
}

/// The button, its line and the save work in French and in Vietnamese too
/// (task 054): the labels of the whole flow follow the interface language.
#[wasm_bindgen_test]
async fn the_add_is_offered_and_said_in_french_and_in_vietnamese() {
    let cases = [
        (
            Language::Fr,
            "csr-learn-fr",
            [
                "S’entraîner",
                "Voir la réponse",
                "Ajouter à la pratique",
                "Ajouté",
            ],
        ),
        (
            Language::Vi,
            "csr-learn-vi",
            ["Luyện tập", "Xem đáp án", "Thêm vào luyện tập", "Đã thêm"],
        ),
    ];
    for (language, root, [practice, show, add, added]) in cases {
        let (store, _page, chosen) =
            mount_in(root, "/learn", store_on(seeded_with(10)), Some(language));
        settle().await;
        button(root, practice).expect("the Practice button").click();
        settle().await;
        button(root, show).expect("the card").click();
        settle().await;
        let found = open_a_word_offering(root, add)
            .await
            .unwrap_or_else(|| panic!("{language:?}: a word offers {add}"));
        found.click();
        settle().await;
        let seen = added_lines_saying(root, added);
        chosen.set(Language::En);
        settle().await;
        assert_eq!(seen, 1, "{language:?}: one '{added}' line");
        assert_eq!(store.saved.borrow().len(), 1, "{language:?}: saved");
        assert_eq!(store.saved.borrow()[0].progress.len(), 11, "{language:?}");
    }
}

/// The progress changed in another tab while a panel is open, the learner
/// reloads the session (task 054, U2): the rebuilt review still offers the
/// add and takes the word, beside what the other tab saved.
#[wasm_bindgen_test]
async fn a_rebuilt_review_still_offers_the_add_and_takes_the_word() {
    let root = "csr-learn-rebuilt";
    let (store, _page) = mount_on(root, "/learn", store_on(seeded_with(10)));
    settle().await;
    button(root, "Practice").expect("Practice").click();
    settle().await;
    button(root, "Show the answer").expect("the card").click();
    settle().await;
    let before = open_a_word_offering_the_add(root)
        .await
        .expect("a panel is open with the button");
    assert!(before.is_connected());

    // Another tab started an eleventh term; the session is paused.
    let mut other = seeded_with(11);
    other.last_today = seeded_with(10).last_today;
    store.changed_to(other);
    settle().await;
    button(root, "Reload the session")
        .expect("the pause banner offers the reload")
        .click();
    settle().await;

    button(root, "Show the answer").expect("a new card").click();
    settle().await;
    let add = open_a_word_offering_the_add(root)
        .await
        .expect("the rebuilt review offers the add");
    add.click();
    settle().await;
    let saved = store.saved.borrow();
    assert_eq!(saved.len(), 1, "the word is saved at once");
    assert_eq!(saved[0].progress.len(), 12, "eleven, and the word started");
    word_pressed(&saved[0]);
}

/// A sync transport the tests script (task 059): counts the sign-ins and the
/// pulls, answers a pull a moment later (never inside the call) with a
/// scripted result.
#[derive(Clone)]
struct ScriptedSync {
    account: RwSignal<SyncAccount>,
    sign_ins: Rc<Cell<usize>>,
    pulls: Rc<Cell<usize>>,
    answer: Rc<RefCell<Result<Option<String>, SyncError>>>,
}

impl ScriptedSync {
    fn new(account: SyncAccount, answer: Result<Option<String>, SyncError>) -> Self {
        ScriptedSync {
            account: RwSignal::new(account),
            sign_ins: Rc::default(),
            pulls: Rc::default(),
            answer: Rc::new(RefCell::new(answer)),
        }
    }
}

impl SyncTransport for ScriptedSync {
    fn sign_in(&self, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        self.sign_ins.set(self.sign_ins.get() + 1);
    }

    fn sign_out(&self) {}

    fn account(&self) -> Signal<SyncAccount> {
        self.account.into()
    }

    fn pull(&self, on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {
        self.pulls.set(self.pulls.get() + 1);
        let answer = self.answer.borrow().clone();
        leptos::task::spawn_local(async move { on_done(answer) });
    }

    fn push(&self, _text: &str, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}
}

/// Waits `ms` of real time (the gate arms real timers).
async fn wait(ms: i32) {
    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    let done = wasm_bindgen::closure::Closure::once_into_js(move || {
        let _ = tx.send(());
    });
    window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(done.unchecked_ref(), ms)
        .expect("a timer");
    rx.await.expect("the timer ran");
    settle().await;
}

/// The text of the app of `root`.
fn text_of(root: &str) -> String {
    document()
        .get_element_by_id(root)
        .and_then(|root| root.text_content())
        .unwrap_or_default()
}

/// The paused line (task 059): a remembered account with no token reads
/// 'Sync paused', no automatic ask reaches the transport, and the Resume tap
/// asks for a token in its gesture (the click is wired to `resume`).
#[wasm_bindgen_test]
async fn a_paused_account_asks_nothing_and_resume_signs_in_in_the_tap() {
    let sync = ScriptedSync::new(SyncAccount::Remembered { email: None }, Ok(None));
    mount_sync("csr-paused", Rc::new(sync.clone()), false);
    wait(60).await;
    assert!(text_of("csr-paused").contains("Sync paused"), "the line");
    assert_eq!(sync.pulls.get(), 0, "no automatic cycle while paused");
    assert_eq!(sync.sign_ins.get(), 0, "no window unasked");
    button("csr-paused", "Resume").expect("Resume").click();
    settle().await;
    assert_eq!(sync.sign_ins.get(), 1, "the tap asks for a token");
}

/// A failed cycle (task 059): the line says it retries by itself and offers
/// Retry; Retry (the click is wired to `sync_now`) runs a cycle at once,
/// well inside the 15 s gap.
#[wasm_bindgen_test]
async fn a_failed_cycle_says_it_retries_and_retry_runs_a_cycle_at_once() {
    let sync = ScriptedSync::new(
        SyncAccount::SignedIn { email: None },
        Err(SyncError::Network),
    );
    mount_sync("csr-retry", Rc::new(sync.clone()), false);
    wait(80).await;
    assert_eq!(sync.pulls.get(), 1, "the open asks one cycle");
    assert!(
        text_of("csr-retry").contains("Retrying automatically"),
        "{}",
        text_of("csr-retry")
    );
    button("csr-retry", "Retry").expect("Retry").click();
    wait(20).await;
    assert_eq!(sync.pulls.get(), 2, "a gesture bypasses the gap");
}
