//! The browser's events that ask the sync for a cycle (task 059): the page
//! opening or becoming visible again (`visibilitychange`, `focus`), the
//! browser coming back online, and the 5-minute tick while the page is
//! visible. Both hosts are webviews, so the shared code owns them; each
//! only calls the `ask` it is given with a [`Reason`], and the sync's gate
//! decides what to do with it.
//!
//! The decisions are pure ([`on_visibility`], [`on_tick`], [`on_online`])
//! and tested natively; the glue that listens and ticks is browser code
//! (`browser` feature) tested in a real browser (`tests/sync_triggers.rs`)
//! through [`__sync_triggers_door`]. On a native build [`install`] does
//! nothing: the server never syncs.
//!
//! The tick is skipped while the page is hidden and is not rescheduled
//! from a hidden tab: the interval keeps its beat, a hidden beat asks
//! nothing and queues nothing, and the visible return asks `Visible`.

use std::fmt::Debug;
use std::time::Duration;

use crate::sync_pacing::Reason;

#[cfg(feature = "browser")]
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[cfg(feature = "browser")]
use leptos::leptos_dom::helpers::IntervalHandle;
#[cfg(feature = "browser")]
use wasm_bindgen::{JsCast, closure::Closure};

/// The tick's period: a visible page asks every five minutes.
#[cfg_attr(
    not(feature = "browser"),
    allow(dead_code, reason = "read by the browser glue and the tests")
)]
const TICK: Duration = Duration::from_secs(5 * 60);

/// The reason to ask when `document.visibilityState` is read after a
/// `visibilitychange`: `Visible` when the page is visible, nothing when it
/// became hidden.
#[cfg_attr(
    not(feature = "browser"),
    allow(dead_code, reason = "read by the browser glue and the tests")
)]
fn on_visibility(visible: bool) -> Option<Reason> {
    visible.then_some(Reason::Visible)
}

/// The reason to ask when the window gains `focus`: the same decision as a
/// `visibilitychange`, since a focus event can reach a hidden page (a
/// window focused while its tab is behind another): `Visible` only when
/// `document.visibilityState` reads `visible`.
#[cfg_attr(
    not(feature = "browser"),
    allow(dead_code, reason = "read by the browser glue and the tests")
)]
fn on_focus(visible: bool) -> Option<Reason> {
    on_visibility(visible)
}

/// The reason to ask at a tick: `Tick` while the page is visible, nothing
/// while it is hidden.
#[cfg_attr(
    not(feature = "browser"),
    allow(dead_code, reason = "read by the browser glue and the tests")
)]
fn on_tick(visible: bool) -> Option<Reason> {
    visible.then_some(Reason::Tick)
}

/// The reason to ask when the browser comes back online.
#[cfg_attr(
    not(feature = "browser"),
    allow(dead_code, reason = "read by the browser glue and the tests")
)]
fn on_online() -> Reason {
    Reason::Online
}

/// What the interval's creation gives: its handle to keep, or the line to
/// log when the browser refused it (G6: never dropped silently; the other
/// listeners stay).
#[cfg_attr(
    not(feature = "browser"),
    allow(dead_code, reason = "read by the browser glue and the tests")
)]
fn tick_started<H, E: Debug>(started: Result<H, E>) -> Result<H, String> {
    started.map_err(|error| format!("learn sync: the 5-minute tick could not start: {error:?}"))
}

#[cfg(feature = "browser")]
thread_local! {
    /// Set by the first [`install_with`]: a second call is a no-op, so the
    /// mount Effect running again (a remount at home) never doubles the
    /// listeners or the interval, which live as long as the page. The page
    /// is one thread (wasm); the browser test resets it through the door.
    static INSTALLED: Cell<bool> = const { Cell::new(false) };

    /// The 5-minute interval's handle, kept for the page's life (F1, deep
    /// 13630): the tick is owned by the page, not by a value that could be
    /// dropped. In leptos 0.8 an `IntervalHandle` is a plain `Copy` id
    /// with no `Drop`, so dropping it would not stop the interval today;
    /// holding it here makes the lifetime explicit, and lets the test door
    /// clear the old interval before a re-install instead of stacking a
    /// second one.
    static TICK_HANDLE: RefCell<Option<IntervalHandle>> = const { RefCell::new(None) };
}

/// Whether `document.visibilityState` reads `visible` now.
#[cfg(feature = "browser")]
fn page_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}

/// Calls `handler` at each `name` event of `target`, for the page's life.
#[cfg(feature = "browser")]
fn listen(target: &web_sys::EventTarget, name: &str, handler: impl Fn() + 'static) {
    let closure = Closure::<dyn Fn()>::new(handler);
    // The listener is never removed: it lives as long as the page.
    let _ = target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Installs the listeners and the tick; every event calls `ask` with its
/// reason. Called once, on the client, when the sync is mounted. The
/// listeners live as long as the page.
#[cfg(feature = "browser")]
pub(crate) fn install(ask: impl Fn(Reason) + 'static) {
    install_with(Rc::new(ask), TICK);
}

/// On a native build nothing listens: the server never syncs.
#[cfg(not(feature = "browser"))]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the signature the browser build shares"
)]
pub(crate) fn install(ask: impl Fn(Reason) + 'static) {
    let _ = ask;
}

/// [`install`] with a chosen tick period. Asks `Visible` once (the page
/// opening), listens to `visibilitychange` (asking only when the page is
/// visible) and to the window's `focus` (`Visible`, only when visible) and
/// `online` (`Online`), and ticks every `tick` (`Tick` only while visible). A second
/// call does nothing.
#[cfg(feature = "browser")]
fn install_with(ask: Rc<dyn Fn(Reason)>, tick: Duration) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    if INSTALLED.replace(true) {
        return;
    }
    if let Some(reason) = on_visibility(page_visible()) {
        ask(reason);
    }
    let on_change = Rc::clone(&ask);
    listen(document.as_ref(), "visibilitychange", move || {
        if let Some(reason) = on_visibility(page_visible()) {
            on_change(reason);
        }
    });
    let on_window_focus = Rc::clone(&ask);
    listen(window.as_ref(), "focus", move || {
        if let Some(reason) = on_focus(page_visible()) {
            on_window_focus(reason);
        }
    });
    let on_back_online = Rc::clone(&ask);
    listen(window.as_ref(), "online", move || {
        on_back_online(on_online());
    });
    keep_tick(leptos::prelude::set_interval_with_handle(
        move || {
            if let Some(reason) = on_tick(page_visible()) {
                ask(reason);
            }
        },
        tick,
    ));
}

/// Keeps the interval's handle for the page's life, or, when the browser
/// refused the interval, logs it to the console (G6) and goes on: the
/// listeners stay, only the beat is missing.
#[cfg(feature = "browser")]
fn keep_tick(started: Result<IntervalHandle, wasm_bindgen::JsValue>) {
    match tick_started(started) {
        Ok(handle) => TICK_HANDLE.set(Some(handle)),
        Err(message) => {
            leptos::logging::error!("{message}");
            TICK_HANDLE.set(None);
        }
    }
}

/// The reason's name, as the test door reports it.
#[cfg(feature = "browser")]
fn reason_name(reason: Reason) -> &'static str {
    match reason {
        Reason::Saved => "Saved",
        Reason::Visible => "Visible",
        Reason::Tick => "Tick",
        Reason::Online => "Online",
        Reason::Home => "Home",
    }
}

/// A test door: installs the real glue with a closure naming each reason,
/// so `tests/sync_triggers.rs` (which sees the public API only) can drive
/// the real listeners and timer in a browser. `tick_ms` is the tick's
/// period; `reset_guard` re-arms the install-once guard first. It exists
/// only for that test and is not part of the API.
#[cfg(feature = "browser")]
#[doc(hidden)]
pub fn __sync_triggers_door(ask: impl Fn(&'static str) + 'static, tick_ms: u32, reset_guard: bool) {
    if reset_guard {
        INSTALLED.set(false);
        if let Some(old) = TICK_HANDLE.take() {
            old.clear();
        }
    }
    install_with(
        Rc::new(move |reason| ask(reason_name(reason))),
        Duration::from_millis(u64::from(tick_ms)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// G6: a refused interval is reported with the error and names the
    /// tick, so the console tells why no five-minute beat comes; a started
    /// one reports nothing and its handle is kept. NOT caught: the glue
    /// forgetting to log the `Err` it gets (the log call is browser code).
    #[test]
    fn a_refused_interval_is_reported_and_a_started_one_kept() {
        let refused = tick_started::<u32, _>(Err("blocked by the page"));
        let message = refused.expect_err("an Err is reported");
        assert!(message.contains("tick"), "{message}");
        assert!(message.contains("blocked by the page"), "{message}");
        assert_eq!(tick_started::<u32, &str>(Ok(7)), Ok(7));
    }

    /// Design: "(2) at open and when document.visibilityState turns
    /// visible". A visible page asks `Visible`; a hidden one asks nothing.
    #[test]
    fn a_page_turning_visible_asks_visible_and_hidden_asks_nothing() {
        assert_eq!(on_visibility(true), Some(Reason::Visible));
        assert_eq!(on_visibility(false), None);
    }

    /// F8 (deep 13630, gemini 13442): `focus` asks only when the page is
    /// visible, like `visibilitychange`. Mutation not caught: the glue
    /// calling `on_focus` with a constant instead of the real state (the
    /// wasm test dispatches `focus` on a hidden page).
    #[test]
    fn a_focus_asks_visible_only_when_the_page_is_visible() {
        assert_eq!(on_focus(true), Some(Reason::Visible));
        assert_eq!(on_focus(false), None);
    }

    /// Design: "a 5-min tick, skipped while hidden". Mutation not caught:
    /// a tick that ignores visibility *and* a visible one (the table holds
    /// both rows, so that one is caught); the wasm test proves the glue
    /// passes the real state.
    #[test]
    fn a_tick_asks_only_while_visible() {
        assert_eq!(on_tick(true), Some(Reason::Tick));
        assert_eq!(on_tick(false), None);
    }

    /// Design: "the browser's 'online' event".
    #[test]
    fn the_online_event_asks_online() {
        assert_eq!(on_online(), Reason::Online);
    }

    /// Design: "a 5-min tick". Not caught: the glue using another period
    /// than `TICK` (the wasm test passes its own).
    #[test]
    fn the_tick_is_five_minutes() {
        assert_eq!(TICK, Duration::from_secs(300));
    }
}
