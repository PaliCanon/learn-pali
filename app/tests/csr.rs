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
use learn_core::deck::Deck;
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
use palicanon_ui_lib::csr;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// The progress the fake store holds and every state it was given to save.
#[derive(Clone)]
struct BrowserStore {
    load: Rc<RefCell<Load>>,
    saved: Rc<RefCell<Vec<State>>>,
    /// The status the store reports; the tests move it.
    status: RwSignal<SaveStatus>,
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

    fn on_change(&self, _callback: Box<dyn Fn()>) {}
}

/// No file ever moves in this test.
struct NoFiles;

impl FileTransfer for NoFiles {
    fn export(&self, _name: &str, _text: &str) {}

    fn pick_import(&self, _on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {}
}

/// Two terms started on a day long past, so they are due whatever today is.
fn seeded() -> State {
    let deck = Deck::from_json(include_str!("../../core/fixtures/deck.json")).expect("the deck");
    let day = Date::parse("2000-01-01").expect("a date");
    let mut session = Session::start(&deck, State::empty(), day, 0).expect("a session");
    assert_eq!(session.request_new(2), 2);
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
    BrowserStore {
        load: Rc::new(RefCell::new(Load::Ok(seeded()))),
        saved: Rc::default(),
        status: RwSignal::new(SaveStatus::Saved),
    }
}

/// The fragment mounted at `url` in an app of id `root`, over a fake store
/// on the seeded state, under a `ParentRoute` at `/learn`; returns the store
/// and a signal that takes the router (the page) away and brings it back,
/// the provider staying where it is.
fn mount_at(root: &'static str, url: &str) -> (BrowserStore, RwSignal<bool>) {
    go_to(url);
    let store = seeded_store();
    let page = RwSignal::new(true);
    let (shared, files): (SharedStore, SharedFiles) = (Rc::new(store.clone()), Rc::new(NoFiles));
    csr::mount(move || {
        palicanon_ui_lib::provide_language();
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
    (store, page)
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
    go_to("/learn");
    let transport = CountingSync {
        prepared: Rc::default(),
        account: RwSignal::new(account),
    };
    let (shared, files, sync): (SharedStore, SharedFiles, SharedSync) = (
        Rc::new(seeded_store()),
        Rc::new(NoFiles),
        Rc::new(transport.clone()),
    );
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
    transport
}

/// Signed out with Settings closed: no sync UI asks anything of Google.
/// Settings shown: its Sign in appears and Google's script is readied, so
/// the tap opens the window in its gesture. A remembered account: the
/// home's chip appears and readies it too.
#[wasm_bindgen_test]
async fn the_sync_ui_readies_google_only_where_a_tap_may_follow() {
    let home = mount_with_sync("csr-sync-home", SyncAccount::SignedOut, false);
    settle().await;
    assert_eq!(home.prepared.get(), 0, "nothing before a tap may follow");
    assert!(button("csr-sync-home", "Sync").is_none(), "no chip");

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
    assert!(button("csr-sync-chip", "Sync").is_some(), "the chip");
    assert!(chip.prepared.get() >= 1, "the chip shown: readied");
}
