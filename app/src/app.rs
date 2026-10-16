//! The service's shell around the pages: the HTML document ([`shell`],
//! server-rendered) and the app root ([`App`], what hydration mounts),
//! `AppShell`'s rail with the services (Search, Learn), the theme and
//! languages.

use leptos::prelude::*;
use leptos_i18n::td_string;
use leptos_meta::{HashedStylesheet, MetaTags, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use palicanon_ui_lib::{
    AppShell, EnvBadgeInfo, HydrationRoot, IconName, NavEntry, NavList, NavSection,
    language_attribute, provide_language, theme_attribute,
};

use crate::catalogue::strings;
use crate::components::data_block::{ImportConfirm, ProgressData, ProgressSection};
use crate::components::site_footer::SiteFooter;
use crate::pages::home::Home;

/// The branch this build came from (`build.rs`, `LEARN_PALI_GIT_BRANCH`):
/// `main`, `feat/…` — this service's own `env` for [`EnvBadgeInfo`]
/// (replaced by `dev` / `prod` once this deploys, like every other service).
pub(crate) const BUILD_BRANCH: &str = env!("LEARN_PALI_GIT_BRANCH");
/// The short commit this build came from (`build.rs`, `LEARN_PALI_GIT_SHA`),
/// a trailing `*` when the worktree had uncommitted changes at build time.
pub(crate) const BUILD_SHA: &str = env!("LEARN_PALI_GIT_SHA");

/// This build, as the footer's badge shows it (at the footer's left;
/// nothing on the shell's panel).
pub(crate) const BADGE: EnvBadgeInfo = EnvBadgeInfo {
    env: BUILD_BRANCH,
    sha: BUILD_SHA,
};

/// The HTML document around the app, rendered on the server. `data-theme`
/// and `lang` follow the request's cookies ([`theme_attribute`],
/// [`language_attribute`]), so the first paint never flashes the wrong
/// one. The stylesheet and the scripts are linked by their hashed names
/// (`hash-files` in `Cargo.toml`), so a new build is never served from
/// the browser's cache. The app sits in ui-lib's [`HydrationRoot`]
/// (`<div id="pc-app">`), where the browser's entry (`hydrate` in
/// `lib.rs`) starts: what Cloudflare adds to `<body>` before or after it
/// (a decoy link first, the beacon after) is never visited (task 040).
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang=language_attribute() data-theme=theme_attribute()>
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <link rel="icon" href="/favicon.ico" />
                <AutoReload options=options.clone() />
                <HashedStylesheet id="leptos" options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <HydrationRoot>
                    <App />
                </HydrationRoot>
            </body>
        </html>
    }
}

/// The app root: what hydration mounts, so it provides the page's one
/// language signal ([`provide_language`]; the server's shell already
/// set `<html lang>` from the very same resolution).
#[component]
pub fn App() -> impl IntoView {
    provide_language();
    provide_meta_context();
    view! {
        <Title text="PaliCanon — Learn" />
        <Router>
            <Layout />
        </Router>
    }
}

/// The rail's entries: the services, Search (the main site) then Learn
/// (this one, the current entry on `/`).
fn nav(current: Signal<String>) -> NavList {
    let t = strings();
    let sections = Signal::derive(move || {
        vec![NavSection {
            title: None,
            entries: vec![
                NavEntry {
                    label: td_string!(t.locale(), nav_search),
                    icon: IconName::Search,
                    href: "https://search.palicanon.app",
                },
                // `IconName::Grid` stands in until ui-lib has a Learn icon.
                NavEntry {
                    label: td_string!(t.locale(), nav_learn),
                    icon: IconName::Grid,
                    href: "/",
                },
            ],
        }]
    });
    NavList::new(sections, current)
}

/// The shell around the page: `AppShell`'s rail, theme and languages, its
/// Settings with 'Your progress' after Appearance (export, import), and
/// the one route (the home at `/`, which holds the progress and runs the
/// review in place), the footer in the shell's slot; then the import's
/// confirmation, outside Settings (opening it closes Settings). The home
/// follows the imports through the provided `StoreWrites`.
#[component]
fn Layout() -> impl IntoView {
    let location = use_location();
    let progress = ProgressData::new();
    provide_context(progress.writes());
    view! {
        <AppShell
            nav=nav(location.pathname.into())
            settings_extra=move || view! { <ProgressSection data=progress /> }
            footer=|| view! { <SiteFooter /> }
        >
            <Routes fallback=|| "Not found.">
                <Route path=path!("/") view=Home />
            </Routes>
        </AppShell>
        <ImportConfirm data=progress />
    }
}

/// The document and layout tests render on the server: they need `ssr`
/// (axum, tokio), which a plain `--workspace` build does not enable.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use futures::StreamExt;
    use leptos_meta::{ServerMetaContext, provide_meta_context};
    use leptos_router::components::Router;
    use leptos_router::location::RequestUrl;
    use palicanon_ui_lib::Language;

    use super::*;

    /// [`Layout`] as the server renders it for `path`, the language set.
    fn render_layout(language: Language, path: &str) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            provide_meta_context();
            provide_context(RequestUrl::new(path));
            view! {
                <Router>
                    <Layout />
                </Router>
            }
            .to_html()
        })
    }

    /// The whole document of [`shell`] for `path`, `cookie` as the request's
    /// `Cookie` header, with the head the meta context collected injected
    /// the way `leptos_axum` does.
    fn render_document(path: &str, cookie: Option<&str>) -> String {
        let options = LeptosOptions::builder().output_name("learn-pali").build();
        render_document_with(options, path, cookie)
    }

    /// [`render_document`] with the given options.
    fn render_document_with(options: LeptosOptions, path: &str, cookie: Option<&str>) -> String {
        let (server, output) = ServerMetaContext::new();
        let body = Owner::new().with(|| {
            let mut request = axum::http::Request::builder().uri(path);
            if let Some(cookie) = cookie {
                request = request.header("cookie", cookie);
            }
            provide_context(request.body(()).expect("a request").into_parts().0);
            provide_context(server);
            provide_context(RequestUrl::new(path));
            shell(options).to_html()
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            // The injection waits a tick of the global executor; it is set
            // once per process, a second call is the expected refusal.
            let _ = any_spawner::Executor::init_tokio();
            output
                .inject_meta_context(futures::stream::iter([body]))
                .await
                .collect::<String>()
                .await
        })
    }

    /// The `<a …>` open tag around byte `at`.
    fn anchor_at(html: &str, at: usize) -> &str {
        let start = html[..at].rfind("<a ").expect(html);
        &html[start..at + html[at..].find('>').expect(html)]
    }

    /// Every `<a …>` open tag of `html`.
    fn anchors(html: &str) -> Vec<&str> {
        html.match_indices("<a ")
            .map(|(at, _)| &html[at..at + html[at..].find('>').expect(html)])
            .collect()
    }

    #[test]
    fn the_rail_lists_search_then_learn() {
        let html = render_layout(Language::En, "/");
        let tags = anchors(&html);
        let search = tags
            .iter()
            .position(|t| t.contains(r#"href="https://search.palicanon.app""#))
            .expect(&html);
        let learn = tags
            .iter()
            .position(|t| t.contains(r#"href="/""#))
            .expect(&html);
        assert!(search < learn, "Search first, Learn second: {html}");
        // The same two entries, in the same order, in the phone's navigation.
        let again = tags
            .iter()
            .rposition(|t| t.contains(r#"href="https://search.palicanon.app""#))
            .expect(&html);
        assert!(again > learn, "{html}");
    }

    /// An external href is a plain link and never the current entry; Learn
    /// is, on `/`.
    #[test]
    fn on_the_home_only_learn_is_current() {
        let html = render_layout(Language::En, "/");
        let current: Vec<usize> = html
            .match_indices(r#"aria-current="page""#)
            .map(|(at, _)| at)
            .collect();
        assert_eq!(current.len(), 2, "rail and phone navigation: {html}");
        for at in current {
            assert!(anchor_at(&html, at).contains(r#"href="/""#), "{html}");
        }
        for tag in anchors(&html) {
            if tag.contains(r#"href="https://search.palicanon.app""#) {
                assert!(!tag.contains("aria-current"), "{tag}");
            }
        }
    }

    #[test]
    fn on_another_path_no_entry_is_current() {
        let html = render_layout(Language::En, "/nope");
        assert_eq!(html.matches("aria-current").count(), 0, "{html}");
    }

    #[test]
    fn the_entries_are_labelled_in_every_language() {
        for (language, search, learn) in [
            (Language::En, "Search", "Learn"),
            (Language::Fr, "Recherche", "Apprendre"),
            (Language::Vi, "Tìm kiếm", "Học"),
        ] {
            let html = render_layout(language, "/");
            assert!(html.contains(&format!(">{search}</span>")), "{html}");
            assert!(html.contains(&format!(">{learn}</span>")), "{html}");
        }
    }

    /// The design's invariant: one language signal. One render of the real
    /// [`Layout`] per language (a server render is a one-shot string, so a
    /// reactive flip inside it is not observable): the rail's labels and
    /// the footer's labels, in the same HTML, are in the same language. A
    /// component reading a second, un-provided signal would stay English.
    #[test]
    fn the_rail_and_the_footer_follow_the_same_language_signal() {
        for (language, rail, footer) in [
            (
                Language::Fr,
                ["Recherche", "Apprendre"],
                ["Crédits", "Conditions", "Confidentialité", "Soutenir"],
            ),
            (
                Language::Vi,
                ["Tìm kiếm", "Học"],
                ["Tri ân", "Điều khoản", "Quyền riêng tư", "Ủng hộ"],
            ),
        ] {
            let html = render_layout(language, "/");
            for label in rail.into_iter().chain(footer) {
                assert!(html.contains(&format!(">{label}</")), "{label}: {html}");
            }
            for english in [
                "Search",
                "Learn",
                "Acknowledgements",
                "Terms",
                "Privacy",
                "Support",
            ] {
                assert!(
                    !html.contains(&format!(">{english}</")),
                    "{english} still English: {html}"
                );
            }
        }
    }

    /// Settings holds 'Your progress' after Appearance, with Export and
    /// Import; the import's confirmation sits outside Settings, after the
    /// shell (opening it closes Settings); the home holds neither.
    #[test]
    fn settings_holds_your_progress_and_the_confirmation_sits_outside() {
        let html = render_layout(Language::En, "/");
        let order = [
            html.find(r#"id="pc-settings""#).expect(&html),
            html.find(">Appearance</h3>").expect(&html),
            html.find(">Your progress</h3>").expect(&html),
            html.find(">Export<").expect(&html),
            html.find(">Import<").expect(&html),
            html.find(r#"data-page="home""#).expect(&html),
            html.find("</footer>").expect(&html),
            html.find(r#"id="pc-import""#).expect(&html),
        ];
        assert!(order.is_sorted(), "{order:?}: {html}");
        assert_eq!(html.matches(">Export<").count(), 1, "{html}");
        assert_eq!(html.matches(r#"type="file""#).count(), 1, "{html}");
    }

    #[test]
    fn the_home_route_renders_the_home() {
        let html = render_layout(Language::En, "/");
        assert!(html.contains(">Learn Pāli</h1>"), "{html}");
        assert!(html.contains(r#"data-page="home""#), "{html}");
        assert!(!html.contains("Not found."), "{html}");
    }

    /// The review runs in place on the home: `/review` is no route any
    /// more.
    #[test]
    fn the_review_path_is_not_found() {
        let html = render_layout(Language::En, "/review");
        assert!(html.contains("Not found."), "{html}");
        assert!(!html.contains("data-page"), "{html}");
    }

    /// The progress lives on the home: `/progress` is no route any more.
    #[test]
    fn the_progress_path_is_not_found() {
        let html = render_layout(Language::En, "/progress");
        assert!(html.contains("Not found."), "{html}");
        assert!(!html.contains("data-page"), "{html}");
    }

    /// The one page leads to neither `/progress` nor `/review`.
    #[test]
    fn the_page_links_to_no_removed_path() {
        let html = render_layout(Language::En, "/");
        assert!(!html.contains("/progress"), "{html}");
        assert!(!html.contains("/review"), "{html}");
    }

    /// The one route renders in every language, its marker present.
    #[test]
    fn the_home_renders_in_every_language() {
        for language in Language::ALL {
            let html = render_layout(language, "/");
            assert!(html.contains(r#"data-page="home""#), "{language:?}: {html}");
        }
    }

    #[test]
    fn an_unknown_path_renders_not_found_and_no_home() {
        let html = render_layout(Language::En, "/nope");
        assert!(html.contains("Not found."), "{html}");
        assert!(!html.contains("<h1"), "{html}");
    }

    #[test]
    fn the_document_has_its_title_and_stylesheet() {
        let html = render_document("/", None);
        assert!(html.contains("<title>PaliCanon — Learn</title>"), "{html}");
        assert!(html.contains(r#"href="/pkg/learn-pali.css""#), "{html}");
    }

    /// With `hash-files` on, the shell links the stylesheet by its hashed
    /// name (read from the hash file `cargo-leptos` writes), so a new build
    /// is never served from the browser's cache. The hash file here is a
    /// temporary one, named by an absolute path.
    #[test]
    fn the_stylesheet_is_linked_by_its_hashed_name() {
        let hash_file = std::env::temp_dir().join("learn-pali-test-hash.txt");
        std::fs::write(&hash_file, "js: aaa111\nwasm: bbb222\ncss: ccc333\n").expect("hash file");
        let options = LeptosOptions::builder()
            .output_name("learn-pali")
            .hash_files(true)
            .hash_file(hash_file.to_string_lossy().as_ref().into())
            .build();
        let html = render_document_with(options, "/", None);
        assert!(
            html.contains(r#"href="/pkg/learn-pali.ccc333.css""#),
            "{html}"
        );
        assert!(!html.contains(r#"href="/pkg/learn-pali.css""#), "{html}");
        assert_eq!(html.matches("learn-pali.ccc333.css").count(), 1, "{html}");
    }

    /// `<html …>` open tag of a rendered document.
    fn html_tag(html: &str) -> &str {
        let open = &html[html.find("<html").expect(html)..];
        &open[..open.find('>').expect(html)]
    }

    #[test]
    fn the_document_carries_lang_and_theme_from_the_cookies() {
        let html = render_document("/", Some("pc_lang=fr; pc_theme=dark"));
        let open = html_tag(&html);
        assert!(open.contains(r#"lang="fr""#), "{open}");
        assert!(open.contains(r#"data-theme="dark""#), "{open}");
    }

    #[test]
    fn without_cookies_the_document_is_english_and_follows_the_system_theme() {
        let html = render_document("/", None);
        let open = html_tag(&html);
        assert!(open.contains(r#"lang="en""#), "{open}");
        assert!(!open.contains("data-theme"), "{open}");
    }

    /// An unknown theme is dropped (the system theme), an unknown language
    /// is English: ui-lib's `theme_attribute` / `language_attribute`, pinned
    /// here as the frame's behaviour.
    #[test]
    fn unknown_cookie_values_fall_back_to_english_and_the_system_theme() {
        let html = render_document("/", Some("pc_theme=neon; pc_lang=xx"));
        let open = html_tag(&html);
        assert!(open.contains(r#"lang="en""#), "{open}");
        assert!(!open.contains("data-theme"), "{open}");
    }

    /// The byte just past the `</div>` that closes the `<div` opened at
    /// byte `open` of `html` (nested `<div>`s counted).
    fn div_end(html: &str, open: usize) -> usize {
        let mut depth = 0usize;
        let mut at = open;
        loop {
            let next_open = html[at..].find("<div").map(|i| at + i);
            let next_close = html[at..].find("</div>").map(|i| at + i).expect(html);
            match next_open {
                Some(o) if o < next_close => {
                    depth += 1;
                    at = o + "<div".len();
                }
                _ => {
                    depth -= 1;
                    at = next_close + "</div>".len();
                    if depth == 0 {
                        return at;
                    }
                }
            }
        }
    }

    /// The app sits in ui-lib's hydration root, `<div id="pc-app">`, the
    /// one child of `<body>` the server renders: `palicanon_ui_lib::hydrate`
    /// starts there, so what Cloudflare puts in `<body>` before or after it
    /// (the decoy link, the beacon) is never visited (040, ui-lib 063).
    #[test]
    fn the_body_holds_the_app_in_the_hydration_root() {
        let html = render_document("/", None);
        let start = crate::test_support::position(&html, "<body>") + "<body>".len();
        let body = &html[start..crate::test_support::position(&html, "</body>")];
        assert!(body.starts_with(r#"<div id="pc-app">"#), "{body}");
        assert_eq!(
            div_end(body, 0),
            body.len(),
            "the root closes the body: {body}"
        );
        assert_eq!(body.matches(r#"id="pc-app""#).count(), 1, "{body}");
        // The app inside it: the rail's first entry and the footer.
        assert!(
            body.contains(r#"href="https://search.palicanon.app""#),
            "{body}"
        );
        assert!(body.contains("<footer"), "{body}");
    }

    /// [`render_layout`] without the hydration markers between elements.
    fn plain_layout(path: &str) -> String {
        let mut html = render_layout(Language::En, path);
        while let Some(start) = html.find("<!--") {
            let end = start + html[start..].find("-->").expect("comment end") + 3;
            html.replace_range(start..end, "");
        }
        html
    }

    /// The frame, short page: the home, then the footer in the shell's
    /// slot (`mt-auto`), the last child of the page column, which sits in
    /// the shell's one screen-tall column — one `min-h-screen`, no second
    /// column (`min-h-dvh`) under it that would add a scroll of the top
    /// bar's height on a phone.
    #[test]
    fn frame_short_page_footer_is_the_last_child_of_the_page_column() {
        let html = plain_layout("/");
        let column = html.find("flex min-w-0 flex-1 flex-col").expect(&html);
        let h1 = html.find("<h1").expect(&html);
        let footer = html.find("<footer").expect(&html);
        assert!(column < h1 && h1 < footer, "{html}");
        assert!(html.contains(r#"<div class="mt-auto"><footer"#), "{html}");
        let tail = &html[footer..];
        let tail = &tail[tail.find("</footer>").expect(&html)..];
        assert!(
            tail.starts_with("</footer></div></div></div></div>"),
            "the footer's slot, then the page column, the row, the screen column close: {tail}"
        );
        assert_eq!(html.matches("min-h-screen").count(), 1, "{html}");
        assert!(!html.contains("min-h-dvh"), "{html}");
    }

    /// The frame, phone: the top bar and the navigation it opens come
    /// before the page; the footer stacks, centred (no `sm:` on those
    /// classes), the badge first, then the links.
    #[test]
    fn frame_phone_top_bar_then_page_and_a_stacked_centred_footer() {
        let html = plain_layout("/");
        let top_bar = html.find("<header").expect(&html);
        let bar_tag = &html[top_bar..top_bar + html[top_bar..].find('>').expect(&html)];
        assert!(bar_tag.contains("sm:hidden"), "{bar_tag}");
        assert!(top_bar < html.find("<h1").expect(&html), "{html}");
        let footer = html.find("<footer").expect(&html);
        let inner = footer + html[footer..].find("<div").expect(&html);
        let tag = &html[inner..inner + html[inner..].find('>').expect(&html)];
        let classes: Vec<&str> = tag.split([' ', '"']).collect();
        for class in ["flex", "flex-col", "items-center"] {
            assert!(classes.contains(&class), "{class}: {tag}");
        }
        let badge = html[footer..].find("title=").expect(&html);
        let nav = html[footer..].find("<nav").expect(&html);
        assert!(badge < nav, "badge, then links: {html}");
    }

    /// The frame, desktop: the rail precedes the page column in the same
    /// row (`sm:flex-row`); the footer is a three-column grid, badge left,
    /// links in the middle column.
    #[test]
    fn frame_desktop_rail_beside_the_page_and_a_three_column_footer() {
        let html = plain_layout("/");
        let row = html.find("flex flex-1 flex-col sm:flex-row").expect(&html);
        let rail = html.find(r#"<nav id="pc-rail""#).expect(&html);
        let column = html.find("flex min-w-0 flex-1 flex-col").expect(&html);
        assert!(row < rail && rail < column, "{html}");
        let rail_tag = &html[rail..rail + html[rail..].find('>').expect(&html)];
        assert!(
            rail_tag.contains("hidden") && rail_tag.contains("sm:block"),
            "{rail_tag}"
        );
        let footer = html.find("<footer").expect(&html);
        let footer_html = &html[footer..html.find("</footer>").expect(&html)];
        assert!(
            footer_html.contains("sm:grid sm:grid-cols-[1fr_auto_1fr]"),
            "{footer_html}"
        );
        assert!(
            footer_html.contains("sm:col-start-1"),
            "badge left: {footer_html}"
        );
        assert!(
            footer_html.contains("sm:col-start-2"),
            "links middle: {footer_html}"
        );
    }
}
