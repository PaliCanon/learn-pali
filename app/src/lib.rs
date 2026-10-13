//! learn-pali: learning Pāli, a PaliCanon service — Leptos SSR + hydrate on
//! `palicanon-ui-lib`. The frame (shell, rail, footer) around two pages:
//! the home (`/`, which holds the progress) and a review session
//! (`/review`), on the learning loop of `learn_core` and the deck
//! embedded in `deck`. The learner's progress stays in the browser
//! (`store`); the server only renders the first paint.

// `AppShell`'s nested `Option<ViewFn>` / `hidden=move || …` markup builds a
// type deep enough that rustc's default query-depth limit can overflow
// computing its layout for an unrelated reason elsewhere in the crate
// (pre-existing in ui-lib; ui-lab's gallery hits the same ceiling, tasks
// 010/027). Raised defensively since this service uses `AppShell` too.
#![recursion_limit = "512"]

pub mod app;
mod catalogue;
mod components;
mod deck;
mod learn;
#[cfg(feature = "ssr")]
pub mod origin;
mod pages;
mod seed;
mod store;
#[cfg(test)]
mod test_support;
mod today;

// `build.rs` includes this same file (`#[path]`) for the env badge's pure
// decisions; declared here too so `cargo test -p learn_pali` covers it.
#[allow(dead_code)]
mod git_info;

// The placeholder-parity check `build.rs` runs on the locale files; built
// here for the tests only, so its refusal is exercised
// (`catalogue::tests`), not just its absence of a false alarm.
#[cfg(test)]
#[path = "../../../ui-lib/build/placeholders.rs"]
mod placeholders;

/// Browser entry point: hydrate the server-rendered page.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
