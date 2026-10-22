//! The sync's browser triggers in a real browser (task 059, lane E): the
//! real listeners and timer of `sync_triggers`, driven through its
//! `#[doc(hidden)]` door, with events dispatched on the page.
//!
//! Two tests. The first runs in order, since the document's visibility and
//! the window's listeners are the page's: the open asks `Visible`; the
//! `online` event asks `Online`; a hidden page (its `visibilityState`
//! redefined) asks nothing on `visibilitychange` or `focus` and gets no
//! `Tick`; back visible, the `visibilitychange` and the window's `focus`
//! ask `Visible` and a `Tick` comes; a second install is a no-op (one
//! `online` event, one `Online`). The second observes the tick's beat and
//! that a re-install replaces the interval instead of stacking one.
//!
//! Run with `wasm-bindgen-test-runner` and a headless Chrome:
//! `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
//! CHROMEDRIVER=<chromedriver> cargo test --locked -p learn_pali --features
//! csr --target wasm32-unknown-unknown --test sync_triggers`.

#![cfg(all(target_arch = "wasm32", feature = "browser"))]

use std::cell::RefCell;
use std::rc::Rc;

use futures::channel::oneshot;
use learn_pali::__sync_triggers_door;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// The tick the test installs with, in milliseconds.
const TICK_MS: u32 = 40;

/// The reasons asked so far, by name, in order.
type Asked = Rc<RefCell<Vec<&'static str>>>;

/// Waits `ms` milliseconds (the page's `setTimeout`).
async fn wait(ms: i32) {
    let (sender, receiver) = oneshot::channel::<()>();
    let sender = RefCell::new(Some(sender));
    let done = Closure::<dyn FnMut()>::new(move || {
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(());
        }
    });
    web_sys::window()
        .expect("a window")
        .set_timeout_with_callback_and_timeout_and_arguments_0(done.as_ref().unchecked_ref(), ms)
        .expect("a timer");
    receiver.await.expect("the timer fired");
    drop(done);
}

/// Dispatches a plain event named `name` on `target`.
fn dispatch(target: &web_sys::EventTarget, name: &str) {
    let event = web_sys::Event::new(name).expect("an event");
    target.dispatch_event(&event).expect("dispatched");
}

/// Makes `document.visibilityState` read `state`, or, with `None`, removes
/// the own property again so the real value (visible, in a headless Chrome
/// page) shows.
fn set_visibility(state: Option<&str>) {
    let script = match state {
        Some(_) => {
            "Object.defineProperty(document, 'visibilityState', { value: state, configurable: true })"
        }
        None => "delete document.visibilityState",
    };
    js_sys::Function::new_with_args("state", script)
        .call1(
            &JsValue::NULL,
            &state.map_or(JsValue::NULL, JsValue::from_str),
        )
        .expect("the visibility is set");
}

/// How many times `name` was asked so far.
fn count(asked: &Asked, name: &str) -> usize {
    asked
        .borrow()
        .iter()
        .filter(|asked| **asked == name)
        .count()
}

/// Installs the real glue with a fresh guard; every reason lands in the
/// returned list.
fn install_fresh() -> Asked {
    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&asked);
    __sync_triggers_door(move |reason| sink.borrow_mut().push(reason), TICK_MS, true);
    asked
}

/// The window and the document, as event targets.
fn targets() -> (web_sys::EventTarget, web_sys::EventTarget) {
    let window = web_sys::window().expect("a window");
    let document = window.document().expect("a document");
    (window.into(), document.into())
}

/// A hidden page: `visibilitychange` and `focus` ask nothing (F8), and no
/// tick arrives.
async fn a_hidden_page_asks_nothing(asked: &Asked) {
    let (window, document) = targets();
    set_visibility(Some("hidden"));
    let visibles = count(asked, "Visible");
    let ticks = count(asked, "Tick");
    dispatch(&document, "visibilitychange");
    dispatch(&window, "focus");
    wait(150).await;
    assert_eq!(
        count(asked, "Visible"),
        visibles,
        "a hidden page asks no Visible, on visibilitychange or on focus"
    );
    assert_eq!(count(asked, "Tick"), ticks, "a hidden page gets no Tick");
}

/// Back visible: `visibilitychange` and `focus` ask `Visible`, and a tick
/// arrives.
async fn a_visible_page_asks(asked: &Asked) {
    let (window, document) = targets();
    let visibles = count(asked, "Visible");
    let ticks = count(asked, "Tick");
    set_visibility(None);
    dispatch(&document, "visibilitychange");
    assert_eq!(
        count(asked, "Visible"),
        visibles + 1,
        "visible again asks Visible"
    );
    dispatch(&window, "focus");
    assert_eq!(count(asked, "Visible"), visibles + 2, "focus asks Visible");
    wait(150).await;
    assert!(count(asked, "Tick") > ticks, "a visible page gets a Tick");
}

/// G6: one `focus` on a visible page asks exactly one `Visible` (not two:
/// a second listener would double it). Counted before and after one
/// dispatched event; the door's open-ask is not part of it.
fn a_focus_asks_exactly_one_visible(asked: &Asked) {
    let (window, _) = targets();
    let before = count(asked, "Visible");
    dispatch(&window, "focus");
    assert_eq!(
        count(asked, "Visible"),
        before + 1,
        "one focus on a visible page asks one Visible, not two"
    );
}

/// A second install (no guard reset) adds no listener and asks nothing.
fn a_second_install_is_a_no_op(asked: &Asked) {
    let (window, _) = targets();
    let onlines = count(asked, "Online");
    let visibles = count(asked, "Visible");
    let sink = Rc::clone(asked);
    __sync_triggers_door(move |reason| sink.borrow_mut().push(reason), TICK_MS, false);
    assert_eq!(
        count(asked, "Visible"),
        visibles,
        "a second install does not ask again"
    );
    dispatch(&window, "online");
    assert_eq!(
        count(asked, "Online"),
        onlines + 1,
        "one online event, one Online"
    );
}

#[wasm_bindgen_test]
async fn the_browser_events_ask_the_sync_and_a_hidden_page_asks_nothing() {
    let (window, _) = targets();
    let asked = install_fresh();

    // The open: one Visible, at install.
    assert_eq!(
        asked.borrow().first().copied(),
        Some("Visible"),
        "the open asks Visible"
    );

    // The online event asks Online.
    let before = count(&asked, "Online");
    dispatch(&window, "online");
    assert_eq!(
        count(&asked, "Online"),
        before + 1,
        "online asks Online once"
    );

    a_hidden_page_asks_nothing(&asked).await;
    a_visible_page_asks(&asked).await;
    a_focus_asks_exactly_one_visible(&asked);
    a_second_install_is_a_no_op(&asked);
}

/// How long the tick tests observe the beat, in milliseconds.
const WINDOW_MS: i32 = 400;

/// The tick keeps beating (F1): at least three ticks arrive within
/// [`WINDOW_MS`] of a visible page with a 40 ms period, and a second
/// install through the door with the guard reset replaces the interval
/// instead of adding one: the old sink hears no tick any more and the
/// total stays what one interval can give.
///
/// Not caught: dropping the interval handle (`set_interval`): in
/// leptos 0.8.21 `IntervalHandle` is a `Copy` wrapper with no `Drop`, so
/// dropping it never clears the interval; the stored handle is the
/// documented contract, and what this test pins is that a re-install
/// clears the stored one.
#[wasm_bindgen_test]
async fn the_tick_keeps_beating_and_a_reinstall_replaces_it() {
    set_visibility(None);
    let first = install_fresh();
    wait(WINDOW_MS).await;
    let beat = count(&first, "Tick");
    assert!(
        beat >= 3,
        "at least three ticks within {WINDOW_MS} ms, got {beat}"
    );

    let first_before = count(&first, "Tick");
    let second = install_fresh();
    wait(WINDOW_MS).await;
    assert_eq!(
        count(&first, "Tick"),
        first_before,
        "the replaced interval is gone: the old sink hears no tick"
    );
    let total = count(&second, "Tick") + (count(&first, "Tick") - first_before);
    assert!(
        total >= 3,
        "the new interval beats, got {total} ticks in {WINDOW_MS} ms"
    );
    let ceiling =
        usize::try_from(WINDOW_MS / i32::try_from(TICK_MS).expect("small")).expect("small") + 1;
    assert!(
        total <= ceiling,
        "one interval at most: {total} ticks in {WINDOW_MS} ms (ceiling {ceiling}), not doubled"
    );
}
