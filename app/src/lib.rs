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

// The site's frame (the shell, the rail, the footer, the document): the
// app builds with `csr` alone and brings its own around `fragment`.
#[cfg(any(feature = "ssr", feature = "hydrate"))]
pub mod app;
pub mod backend;
mod catalogue;
mod components;
mod deck;
pub mod drive_location;
pub mod fragment;
mod learn;
#[cfg(feature = "ssr")]
pub mod origin;
mod page;
mod pages;
pub mod release;
mod seed;
mod store;
mod sync;
#[cfg(test)]
mod test_support;
mod today;
// The `localStorage` store and the file input of the site: nothing of them
// is in a `csr` build (the app has its own).
#[cfg(any(feature = "ssr", feature = "web-store"))]
mod web;
#[cfg(any(feature = "ssr", feature = "web-store"))]
pub use web::{web_files, web_store};
// The site's sync through Google Drive (task 050): nothing of it is in a
// `csr` build (the app syncs through its own transport).
#[cfg(any(feature = "ssr", feature = "web-sync"))]
pub mod web_sync;
#[cfg(any(feature = "ssr", feature = "web-sync"))]
pub use web_sync::web_sync;

// The placeholder-parity check `build.rs` runs on the locale files; built
// here for the tests only, so its refusal is exercised
// (`catalogue::tests`), not just its absence of a false alarm.
#[cfg(test)]
#[path = "../build/placeholders.rs"]
mod placeholders;

/// Browser entry point: hydrate the server-rendered page from ui-lib's
/// root (`<div id="pc-app">`, which `app::shell` wraps the app in), not
/// from `<body>`: Cloudflare puts a decoy link before the app and scripts
/// after it, and `hydrate_body` panics on the first (task 040, ui-lib
/// 063). Sets `<html data-hydrated>` once done, which the deploy's
/// rollout check reads. The release the server wrote in `<head>` is read
/// first and provided to the app (task 060).
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use leptos::prelude::*;

    console_error_panic_hook::set_once();
    // The release the server wrote in `<head>`, read before the app
    // mounts: the badge is the server's, never recomputed here (task 060).
    palicanon_ui_lib::hydrate(|| {
        provide_context(release::ReleaseInfo::from_document());
        view! { <app::App /> }
    });
}
