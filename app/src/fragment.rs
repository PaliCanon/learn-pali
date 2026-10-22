//! The Learn fragment: what a host mounts to run Learn inside its own shell.
//! The site mounts it at the root, the PaliCanon app inside its own
//! `ParentRoute` at `/learn`. There is no shell in it: no `AppShell`, no
//! rail, no footer, no `Title`.
//!
//! Three pieces, all the host's to place:
//!
//! - [`LearnRoutes`], the routes (today one: the home, with an empty
//!   relative path) that the PARENT mounts. Its base is never a prop: the
//!   brand link is built from `use_matched()` of the home route, so the
//!   route and the link cannot drift (`/` at the root, `/learn` in the
//!   app).
//! - [`LearnProvider`], the progress context: it is given the host's
//!   [`ProgressStore`] and [`FileTransfer`] (required, no default), provides
//!   them to the screens and renders the import's confirmation after its
//!   children. One per page.
//! - [`LearnSettings`], the 'Your progress' section for the host's
//!   Settings.
//!
//! # What the fragment needs from its host
//!
//! Each of these is tested:
//!
//! - `palicanon_ui_lib`'s `AppShell` above it: [`LearnSettings`] reads its
//!   `ShellState` (`settings_open`) to read the store each time Settings
//!   opens; without one it still renders and reads once.
//! - The page's one language signal (`provide_language`), which every
//!   string reads.
//! - Root-served assets: the brand's `/lotus.png`.
//! - One [`LearnProvider`] per page: the import's dialog has a fixed id
//!   (`pc-import`).
//! - A `palicanon-ui-lib` tag equal to the host's, so the styles and the
//!   components are the same.
//!
//! The deck is embedded in the build (`include_str!`): Learn works offline
//! from the first start.

use std::cell::Cell;

use leptos::prelude::*;
use leptos_router::MatchNestedRoutes;
use leptos_router::components::Route;
use leptos_router::hooks::use_matched;
use leptos_router::path;

use crate::backend::{Backend, Hint};
pub use crate::backend::{
    FileTransfer, HintMemory, PickFailed, ProgressStore, SaveError, SaveStatus, SharedFiles,
    SharedHint, SharedStore, SharedSync, SyncAccount, SyncError, SyncTransport,
};
use crate::components::data_block::{ImportConfirm, ProgressData, ProgressSection};
use crate::pages::home::Home;
pub use crate::store::{Load, classify};
use crate::sync::{SyncCycle, browser_timer, clock_label};
use crate::sync_pacing::Reason;
use crate::sync_triggers::install;

/// The Learn routes, to mount under a parent: one route with an empty
/// relative path, the home. The site puts it in its `Routes` at the root,
/// the app in its own `ParentRoute` at `/learn`; `/learn` and `/learn/` both
/// show the home there.
#[component(transparent)]
pub fn LearnRoutes() -> impl MatchNestedRoutes + Send + Clone + 'static {
    view! { <Route path=path!("") view=LearnHome /> }.into_inner()
}

/// The home route's view. The brand links to where the route lives: the
/// route's matched path (`use_matched`), which is `/learn` under a parent at
/// `/learn` and empty at the root, where the link is `/`. Built from the
/// route itself, so the route and the link cannot drift.
#[component]
fn LearnHome() -> impl IntoView {
    let matched = use_matched();
    let base = Signal::derive(move || {
        let matched = matched.get();
        if matched.is_empty() {
            "/".to_owned()
        } else {
            matched
        }
    });
    view! { <Home base=base /> }
}

/// The progress context of the Learn screens, over the host's `store` and
/// `files`: it provides them, the progress data Settings and the home share
/// and the writes counter, registers the store's `on_change` (the progress
/// changed outside a save: another tab, a write conflict), and renders its
/// `children` then the import's confirmation (outside Settings: opening it
/// closes Settings). One per page.
///
/// Given a `sync` transport (task 050), it provides the sync cycle
/// (`crate::sync`): Settings shows the sync, the home its chip, and a cycle
/// runs at load when a token is at hand. A merge the cycle saves runs the
/// same handler as the store's `on_change`. Without one, nothing changes.
///
/// Put it above the `Router` (or build the store inside the router's
/// children): a router's children closure must be `Send`, and a store, an
/// `Rc`, is not.
#[component]
pub fn LearnProvider(
    /// Where the progress lives.
    store: SharedStore,
    /// How a file moves in and out.
    files: SharedFiles,
    /// Where the progress is copied between devices; none: no sync, no
    /// sync UI.
    #[prop(optional_no_strip)]
    sync: Option<SharedSync>,
    /// Where the 'sync on several devices' hint is remembered as answered
    /// (task 059); none: the hint keeps its flag in memory for the tab.
    #[prop(optional_no_strip)]
    hint: Option<SharedHint>,
    /// The host's screens, with [`LearnRoutes`] and [`LearnSettings`]
    /// somewhere inside them.
    children: Children,
) -> impl IntoView {
    let backend = Backend::new(store, files);
    provide_context(backend);
    let data = ProgressData::new();
    // The kept failure follows the store's status while no page is mounted:
    // a memo is read when asked, and a `Failed` that came and went with
    // nobody looking would be missed.
    Effect::new(move |_| {
        data.failure.track();
    });
    provide_context(data);
    provide_context(data.writes());
    // The store says the memory changed outside a save: read it again for
    // Settings, and move the writes counter so the home reads it too (it is
    // paused in a session).
    let changed = move || {
        data.read();
        data.moved();
    };
    backend.on_change(changed);
    if let Some(memory) = hint {
        provide_context(Hint::new(memory));
    }
    if let Some(transport) = sync {
        let cycle = SyncCycle::new(transport, backend, changed, clock_label, browser_timer);
        provide_context(cycle);
        // Mounted, on the client only: the sync UI shows (the server's
        // paint, with no transport, shows none), and a cycle runs when a
        // token is at hand. A change the learner saved asks the sync (the
        // cycle's own merge save does not) once the cycle is mounted, so
        // nothing asks a cycle before it (the server's paint, hydration,
        // an import before this Effect); the browser's events ask it too.
        let done = Cell::new(false);
        Effect::new(move |_| mount_sync(&done, cycle, backend));
    }
    view! {
        {children()}
        <ImportConfirm data=data />
    }
}

/// The mount Effect's body, run once per `done` (G7, the once guard, like
/// `install`'s own): the Effect may run again (a tracked signal read inside
/// the cycle's ask, a remount at home) and the cycle must not be mounted,
/// asked and given a saved-listener again. The guard was chosen over an
/// Effect that tracks nothing, because `cycle.mount()` asks, and an ask
/// reads the account: it is not guaranteed to stay untracked.
fn mount_sync(done: &Cell<bool>, cycle: SyncCycle, backend: Backend) {
    if done.replace(true) {
        return;
    }
    cycle.mount();
    backend.on_saved(move || cycle.ask(Reason::Saved));
    install(move |reason| cycle.ask(reason));
}

/// 'Your progress', the section for the host's Settings (`AppShell`'s
/// `settings_extra`): Export and Import, with the reason when they are off,
/// then the sync (its `ProgressActions`) when the host gave a transport.
///
/// # Panics
/// When there is no [`LearnProvider`] above it.
#[component]
pub fn LearnSettings() -> impl IntoView {
    let data = use_context::<ProgressData>().expect("LearnSettings needs a LearnProvider above it");
    view! { <ProgressSection data=data /> }
}

/// What the fragment relies on from `leptos_router`, pinned: the home is a
/// route with an empty relative path, so a parent decides where it lives,
/// and `use_matched()` of that route is the parent's path (the base the
/// links are built from).
#[cfg(all(test, feature = "ssr"))]
mod router_tests {
    use leptos::prelude::*;
    use leptos_router::components::{Outlet, ParentRoute, Route, Router, Routes};
    use leptos_router::hooks::use_matched;
    use leptos_router::location::RequestUrl;
    use leptos_router::path;

    /// A route view that prints what `use_matched()` answers.
    #[component]
    fn Probe() -> impl IntoView {
        let matched = use_matched();
        view! { <p data-probe=move || matched.get()>"probe"</p> }
    }

    /// The probe under a `ParentRoute` at `/learn`, `url` the request.
    fn under_learn(url: &str) -> String {
        Owner::new().with(|| {
            provide_context(RequestUrl::new(url));
            view! {
                <Router>
                    <Routes fallback=|| "Not found.">
                        <ParentRoute path=path!("/learn") view=|| view! { <Outlet /> }>
                            <Route path=path!("") view=Probe />
                        </ParentRoute>
                    </Routes>
                </Router>
            }
            .to_html()
        })
    }

    /// The probe at the root, `url` the request.
    fn at_root(url: &str) -> String {
        Owner::new().with(|| {
            provide_context(RequestUrl::new(url));
            view! {
                <Router>
                    <Routes fallback=|| "Not found.">
                        <Route path=path!("") view=Probe />
                    </Routes>
                </Router>
            }
            .to_html()
        })
    }

    #[test]
    fn learn_and_learn_slash_match_the_empty_route_and_matched_is_learn() {
        for url in ["/learn", "/learn/"] {
            let html = under_learn(url);
            assert!(html.contains(r#"data-probe="/learn""#), "{url}: {html}");
            assert!(!html.contains("Not found."), "{url}: {html}");
        }
    }

    #[test]
    fn the_root_and_other_paths_do_not_match_under_learn() {
        for url in ["/", "/nope", "/learnx", "/learn/nope"] {
            let html = under_learn(url);
            assert!(html.contains("Not found."), "{url}: {html}");
            assert!(!html.contains("probe"), "{url}: {html}");
        }
    }

    #[test]
    fn at_the_root_the_empty_route_matches_slash_only() {
        let html = at_root("/");
        assert!(!html.contains("Not found."), "{html}");
        assert!(html.contains(r#"data-probe=""#), "matched is empty: {html}");
        let html = at_root("/nope");
        assert!(html.contains("Not found."), "{html}");
    }
}

/// The fragment mounted the way its hosts mount it: under a `ParentRoute`
/// at `/learn` (the app) and at the root (the site), in a bare host with no
/// shell.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::rc::Rc;

    use leptos_router::components::{Outlet, ParentRoute, Router, Routes};
    use leptos_router::location::RequestUrl;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::test_support::{FakeFiles, FakeStore};

    /// A provider over fresh fakes around `children`.
    fn with_provider(children: impl FnOnce() -> AnyView + Send + 'static) -> impl IntoView {
        let store: SharedStore = Rc::new(FakeStore::new(Load::Empty));
        let files: SharedFiles = Rc::new(FakeFiles::default());
        view! {
            <LearnProvider store=store files=files>
                {children()}
            </LearnProvider>
        }
    }

    /// The fragment under a `ParentRoute` at `/learn`, `url` the request.
    fn under_learn(url: &str) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            provide_context(RequestUrl::new(url));
            view! {
                <Router>
                    {with_provider(|| {
                        view! {
                            <Routes fallback=|| "Not found.">
                                <ParentRoute path=path!("/learn") view=|| view! { <Outlet /> }>
                                    <LearnRoutes />
                                </ParentRoute>
                            </Routes>
                        }
                            .into_any()
                    })}
                </Router>
            }
            .to_html()
        })
    }

    /// The fragment at the root, `url` the request, in `language`.
    fn at_root(url: &str, language: Language) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            provide_context(RequestUrl::new(url));
            view! {
                <Router>
                    {with_provider(|| {
                        view! {
                            <Routes fallback=|| "Not found.">
                                <LearnRoutes />
                            </Routes>
                        }
                            .into_any()
                    })}
                </Router>
            }
            .to_html()
        })
    }

    /// The `href` of the brand link (the one around `PaliLearn`).
    fn brand_href(html: &str) -> String {
        let at = html.find("PaliLearn").expect(html);
        let start = html[..at].rfind("<a ").expect(html);
        let tag = &html[start..at];
        let href = tag.find("href=\"").expect(tag) + 6;
        tag[href..href + tag[href..].find('"').expect(tag)].to_owned()
    }

    #[test]
    fn learn_and_learn_slash_render_the_home() {
        for url in ["/learn", "/learn/"] {
            let html = under_learn(url);
            assert!(html.contains(r#"data-page="home""#), "{url}: {html}");
            assert!(!html.contains("Not found."), "{url}: {html}");
        }
    }

    #[test]
    fn the_root_and_other_paths_are_not_found_under_learn() {
        for url in ["/", "/nope", "/learn/nope"] {
            let html = under_learn(url);
            assert!(html.contains("Not found."), "{url}: {html}");
            assert!(!html.contains("data-page"), "{url}: {html}");
        }
    }

    /// The link is built from the route's own matched path, so the route
    /// and the href cannot drift.
    #[test]
    fn the_brand_links_to_the_parents_path() {
        assert_eq!(brand_href(&under_learn("/learn")), "/learn");
        assert_eq!(brand_href(&under_learn("/learn/")), "/learn");
    }

    #[test]
    fn the_same_fragment_at_the_root_keeps_the_brand_at_slash() {
        let html = at_root("/", Language::En);
        assert_eq!(brand_href(&html), "/");
        assert!(at_root("/nope", Language::En).contains("Not found."));
    }

    /// No shell inside the fragment: the host's rail, footer and title are
    /// the host's.
    #[test]
    fn the_fragment_holds_no_shell() {
        let html = under_learn("/learn");
        for shell in ["<nav", "<footer", "<header", "<title", "pc-rail"] {
            assert!(!html.contains(shell), "{shell}: {html}");
        }
    }

    /// Settings' section in a host with no `AppShell` (so no `ShellState`):
    /// it still renders, and is the only 'Your progress' on the page.
    #[test]
    fn a_host_without_a_shell_still_renders_the_settings_section() {
        let html = Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            with_provider(|| view! { <LearnSettings /> }.into_any()).to_html()
        });
        assert_eq!(html.matches(">Your progress</h3>").count(), 1, "{html}");
        assert!(
            html.contains(">Export<") && html.contains(">Import<"),
            "{html}"
        );
    }

    #[test]
    #[should_panic(expected = "LearnSettings needs a LearnProvider above it")]
    fn settings_without_a_provider_say_what_is_missing() {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            view! { <LearnSettings /> }.to_html()
        });
    }

    /// The store says the memory changed outside a save (another tab, a
    /// write conflict): the provider's handler moves the writes counter,
    /// which the home follows (it refreshes at home, pauses in a session).
    #[test]
    fn the_stores_change_moves_the_writes_the_home_follows() {
        use crate::components::data_block::StoreWrites;
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(crate::test_support::progress_fixture()));
            let shared: SharedStore = Rc::new(store.clone());
            let files: SharedFiles = Rc::new(FakeFiles::default());
            let writes = RwSignal::new(None::<StoreWrites>);
            let _ = view! {
                <LearnProvider store=shared files=files>
                    {move || {
                        writes.set(use_context::<StoreWrites>());
                    }}
                </LearnProvider>
            }
            .to_html();
            let StoreWrites(count) = writes.get_untracked().expect("the provider provides them");
            assert_eq!(count.get_untracked(), 0);
            store.changed_to(Load::NewerVersion { raw: "x".into() });
            assert_eq!(count.get_untracked(), 1);
            store.changed_to(Load::Unreadable("denied".into()));
            assert_eq!(count.get_untracked(), 2);
        });
    }

    /// What the fragment assumes of the page, pinned: the brand's lotus at
    /// the root, the one import dialog id, the host's language signal.
    #[test]
    fn the_hidden_dependencies_are_what_the_docs_say() {
        let html = under_learn("/learn");
        assert!(html.contains(r#"src="/lotus.png""#), "{html}");
        assert_eq!(html.matches(r#"id="pc-import""#).count(), 1, "{html}");
        let french = at_root("/", Language::Fr);
        assert!(
            french.contains("Apprendre le pāli") || french.contains("pāli"),
            "{french}"
        );
        assert!(!french.contains(">Learn Pāli<"), "{french}");
    }

    /// Task 059 F6: the saved-listener is registered inside the mount
    /// Effect, so nothing asks a cycle that is not mounted (the server's
    /// paint, hydration, an import before the Effect). Effects do not run
    /// in ssr: a learner save after the render must ask nothing. Every ask
    /// refreshes the transport first, so a registered listener would show
    /// as one refresh. NOT caught: a listener registered in a different
    /// Effect that also runs only on the client.
    #[test]
    fn a_save_before_the_mount_effect_asks_no_cycle() {
        use crate::test_support::FakeTransport;
        Owner::new().with(|| {
            let transport = FakeTransport::new(SyncAccount::SignedIn {
                email: Some("a@b.c".into()),
            });
            let shared: SharedSync = Rc::new(transport.clone());
            let store: SharedStore = Rc::new(FakeStore::new(Load::Empty));
            let files: SharedFiles = Rc::new(FakeFiles::default());
            let backend = RwSignal::new(None::<Backend>);
            let _ = view! {
                <LearnProvider store=store files=files sync=Some(shared)>
                    {move || {
                        backend.set(Some(Backend::current()));
                    }}
                </LearnProvider>
            }
            .to_html();
            let backend = backend.get_untracked().expect("the provider provides it");
            backend.save(&learn_core::progress::State::empty());
            assert_eq!(
                transport.refreshes(),
                0,
                "a save asked a cycle that was never mounted"
            );
        });
    }

    /// Task 059 G7: the mount Effect's body runs once, whatever the number
    /// of times the Effect runs (a tracked signal read inside would run it
    /// again). Two runs of the body leave the cycle asked once (the mount's
    /// own ask refreshes the transport once) and a learner save asks
    /// exactly one more cycle. NOT caught: a second `install` (the native
    /// build installs nothing; the browser's own guard is tested in
    /// `tests/sync_triggers.rs`), nor `on_saved` registered twice (a single
    /// slot, the latest wins: harmless).
    #[test]
    fn two_runs_of_the_mount_body_ask_once_and_a_save_asks_once_more() {
        use crate::backend::Backend;
        use crate::test_support::FakeTransport;
        Owner::new().with(|| {
            let transport = FakeTransport::new(SyncAccount::SignedIn {
                email: Some("a@b.c".into()),
            });
            let store: SharedStore = Rc::new(FakeStore::new(Load::Empty));
            let files: SharedFiles = Rc::new(FakeFiles::default());
            let backend = Backend::new(store, files);
            let shared: SharedSync = Rc::new(transport.clone());
            let cycle = SyncCycle::new(shared, backend, || {}, String::new, |_, _| {});
            let done = Cell::new(false);
            mount_sync(&done, cycle, backend);
            mount_sync(&done, cycle, backend);
            assert_eq!(transport.refreshes(), 1, "the second run asked again");
            assert!(cycle.mounted(), "the first run mounted it");
            backend.save(&learn_core::progress::State::empty());
            assert_eq!(transport.refreshes(), 2, "a save asks exactly one cycle");
        });
    }
}
