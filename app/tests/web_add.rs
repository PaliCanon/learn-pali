//! A word added in a run, through the site's real store (`localStorage`):
//! after the run is stopped the home counts it and lists it, and a fresh
//! mount (a reload) reads it again from `pc_learn` (task 054, the user's
//! check on dev).
//!
//! Run with `wasm-bindgen-test-runner` and a headless Chrome:
//! `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
//! CHROMEDRIVER=<chromedriver> cargo test --locked -p learn_pali --features
//! csr,web-store --target wasm32-unknown-unknown --test web_add`.

#![cfg(all(target_arch = "wasm32", feature = "csr", feature = "web-store"))]

use learn_core::date::Date;
use learn_core::deck::Deck;
use learn_core::progress::State;
use learn_core::session::Session;
use learn_pali::fragment::{LearnProvider, LearnRoutes};
use learn_pali::{web_files, web_store};
use leptos::prelude::*;
use leptos_router::components::{Outlet, ParentRoute, Router, Routes};
use leptos_router::path;
use palicanon_ui_lib::csr;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// `n` terms started on `day`.
fn seeded_on(n: usize, day: &str) -> State {
    let deck = Deck::from_json(include_str!("../../core/fixtures/deck.json")).expect("the deck");
    let day = Date::parse(day).expect("a date");
    let mut session = Session::start(&deck, State::empty(), day, 0).expect("a session");
    assert_eq!(session.request_new(n), n);
    session.into_state()
}

/// The text of `localStorage[pc_learn]`.
fn stored() -> String {
    window()
        .local_storage()
        .expect("storage")
        .expect("a storage")
        .get_item("pc_learn")
        .expect("a read")
        .unwrap_or_default()
}

/// The first button of `root` whose text contains `label`.
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

/// The first button of `root` named `label` by its `aria-label`.
fn labelled(root: &str, label: &str) -> Option<web_sys::HtmlElement> {
    let root = document().get_element_by_id(root).expect("the app's root");
    root.query_selector(&format!(r#"button[aria-label="{label}"]"#))
        .expect("a selector")?
        .dyn_into()
        .ok()
}

/// Lets the effects and the events of the page run.
async fn settle() {
    for _ in 0..4 {
        leptos::task::tick().await;
    }
}

/// The site's fragment, over the real store, in an app of id `root`.
fn mount(root: &'static str) {
    window()
        .history()
        .expect("history")
        .push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/learn"))
        .expect("pushState");
    let (store, files) = (web_store(), web_files());
    csr::mount(move || {
        palicanon_ui_lib::provide_language();
        view! {
            <div id=root>
                <LearnProvider store=store files=files>
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

/// Presses 'Add to practice' on up to `n` words of the revealed example.
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

/// The count on the home's tile `label`.
fn tile(root: &str, label: &str) -> String {
    button(root, label)
        .and_then(|tile| tile.text_content())
        .unwrap_or_default()
}

/// The terms the home lists.
fn rows(root: &str) -> usize {
    document()
        .get_element_by_id(root)
        .expect("root")
        .query_selector_all("[data-term-list] > ul > li")
        .expect("rows")
        .length() as usize
}

#[wasm_bindgen_test]
async fn words_added_in_a_run_reach_local_storage_and_the_home() {
    run_then("web-add", "web-add-again", true).await;
}

/// The same, but the run is finished by answering every card.
#[wasm_bindgen_test]
async fn words_added_in_a_run_survive_finishing_it() {
    run_then("web-fin", "web-fin-again", false).await;
}

/// Leaves the run: by the close cross and Stop, or by answering every card
/// and 'Back home'.
async fn leave(app: &str, stop: bool) {
    if stop {
        labelled(app, "Stop the session").expect("cross").click();
        settle().await;
        button(app, "Stop").expect("Stop").click();
    } else {
        for _ in 0..200 {
            if let Some(show) = button(app, "Show the answer") {
                show.click();
                settle().await;
            }
            let Some(grade) = button(app, "Knew it") else {
                break;
            };
            grade.click();
            settle().await;
        }
        let text = document()
            .get_element_by_id(app)
            .and_then(|root| root.text_content())
            .unwrap_or_default();
        button(app, "Back home")
            .unwrap_or_else(|| panic!("the end: {text}"))
            .click();
    }
}

/// Seeds ten terms, adds words in a run, leaves it by Stop (`stop`) or by
/// answering to the end, then checks the home and a reload.
async fn run_then(app: &'static str, again: &'static str, stop: bool) {
    window()
        .local_storage()
        .unwrap()
        .unwrap()
        .set_item("pc_learn", &seeded_on(10, "2000-01-01").to_json())
        .unwrap();
    mount(app);
    settle().await;
    assert_eq!(tile(app, "Daily"), "Daily10");
    button(app, "Practice").expect("Practice").click();
    settle().await;
    button(app, "Show the answer").expect("card").click();
    settle().await;
    let added = add_words(app, 3).await;
    assert!(added >= 2, "several words were added: {added}");

    let after_add = State::from_json(&stored()).expect("the stored state");
    assert_eq!(after_add.progress.len(), 10 + added, "pc_learn: started");

    leave(app, stop).await;
    settle().await;
    // Stopped, every term waits in Daily; answered to the end, every one
    // (the words added too) passed on to Weekly.
    let all = 10 + added;
    let tiles: Vec<String> = [
        "Daily",
        "Weekly",
        "Monthly",
        "Quarterly",
        "Half-yearly",
        "Yearly",
    ]
    .iter()
    .map(|label| tile(app, label))
    .collect();
    let (day, week) = if stop { (all, 0) } else { (0, all) };
    assert_eq!(
        tiles.join("|"),
        format!("Daily{day}|Weekly{week}|Monthly0|Quarterly0|Half-yearly0|Yearly0"),
        "the home's tiles, stop={stop}"
    );
    assert_eq!(rows(app), 10 + added, "the list shows them");

    // A reload: a second app reads the storage afresh.
    mount(again);
    settle().await;
    assert_eq!(rows(again), 10 + added, "after a reload");
}
