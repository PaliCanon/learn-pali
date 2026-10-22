//! The one-time sync hint in a real browser (task 059): signed out, it
//! shows after the first finished session and not before; a sign-in answers
//! it for good, so signing out again does not bring it back (the hint's
//! sign-in Effect, which the server-rendered tests cannot see).
//!
//! Its own test file, hence its own page: the hint's answered flag is the
//! tab's (a thread-local), so a test that signs in anywhere else in the
//! same run would answer it first.
//!
//! Run with `wasm-bindgen-test-runner` and a headless Chrome:
//! `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
//! CHROMEDRIVER=<chromedriver> cargo test --locked -p learn_pali --features
//! csr --target wasm32-unknown-unknown --test sync_hint`.

#![cfg(all(target_arch = "wasm32", feature = "csr"))]

use std::cell::RefCell;
use std::rc::Rc;

use learn_core::book::Book;
use learn_core::date::Date;
use learn_core::deck::Deck;
use learn_core::progress::State;
use learn_core::session::Session;
use learn_pali::fragment::{
    FileTransfer, LearnProvider, LearnRoutes, Load, PickFailed, ProgressStore, SaveStatus,
    SharedFiles, SharedStore, SharedSync, SyncAccount, SyncError, SyncTransport,
};
use leptos::prelude::*;
use leptos_router::components::{Outlet, ParentRoute, Router, Routes};
use leptos_router::path;
use palicanon_ui_lib::csr;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// A store holding a Book whose words deck is two terms deep, due whatever
/// today is.
struct Store(RefCell<Load<Book>>);

impl ProgressStore for Store {
    fn load(&self) -> Load<Book> {
        self.0.borrow().clone()
    }

    fn save(&self, book: &Book) {
        *self.0.borrow_mut() = Load::Ok(book.clone());
    }

    fn status(&self) -> Signal<SaveStatus> {
        RwSignal::new(SaveStatus::Saved).into()
    }

    fn on_change(&self, _callback: Box<dyn Fn()>) {}
}

/// No file ever moves in this test.
struct NoFiles;

impl FileTransfer for NoFiles {
    fn export(&self, _name: &str, _text: &str) {}

    fn pick_import(&self, _on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {}
}

/// A transport whose account the test moves; it never answers.
#[derive(Clone)]
struct Account(RwSignal<SyncAccount>);

impl SyncTransport for Account {
    fn sign_in(&self, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}

    fn sign_out(&self) {}

    fn account(&self) -> Signal<SyncAccount> {
        self.0.into()
    }

    fn pull(&self, _on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {}

    fn push(&self, _text: &str, _on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {}
}

/// Two terms started on a day long past.
fn seeded() -> State {
    let deck = Deck::from_json(include_str!("../../core/fixtures/deck.json")).expect("the deck");
    let day = Date::parse("2000-01-01").expect("a date");
    let mut session = Session::start(&deck, State::empty(), day, 0).expect("a session");
    assert_eq!(session.request_new(2), 2);
    session.into_state()
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

/// The text of the app of `root`.
fn text_of(root: &str) -> String {
    document()
        .get_element_by_id(root)
        .and_then(|root| root.text_content())
        .unwrap_or_default()
}

/// Mounts the fragment at `/learn` over `account`.
fn mount(root: &'static str, account: Account) {
    window()
        .history()
        .expect("history")
        .push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/learn"))
        .expect("pushState");
    let store: SharedStore = Rc::new(Store(RefCell::new(Load::Ok(Book::from_state(seeded())))));
    let (files, sync): (SharedFiles, SharedSync) = (Rc::new(NoFiles), Rc::new(account));
    csr::mount(move || {
        palicanon_ui_lib::provide_language();
        view! {
            <div id=root>
                <LearnProvider store=store files=files sync=Some(sync)>
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

/// Answers every card of the session, then goes back home.
async fn finish_the_session(root: &str) {
    button(root, "Practice").expect("Practice").click();
    settle().await;
    for _ in 0..30 {
        let Some(card) = button(root, "Show the answer") else {
            break;
        };
        card.click();
        settle().await;
        button(root, "Knew it").expect("grade").click();
        settle().await;
    }
    button(root, "Back home").expect("Back home").click();
    settle().await;
}

#[wasm_bindgen_test]
async fn the_hint_shows_after_a_finished_session_and_a_sign_in_answers_it_for_good() {
    let account = Account(RwSignal::new(SyncAccount::SignedOut));
    mount("csr-hint", account.clone());
    settle().await;
    let hint = || text_of("csr-hint").contains("Use Learn on several devices");
    assert!(!hint(), "not before a session is finished");
    finish_the_session("csr-hint").await;
    assert!(hint(), "{}", text_of("csr-hint"));
    account.0.set(SyncAccount::Remembered { email: None });
    settle().await;
    assert!(!hint(), "signed in: no hint");
    account.0.set(SyncAccount::SignedOut);
    settle().await;
    assert!(!hint(), "the sign-in answered it: it stays away");
}
