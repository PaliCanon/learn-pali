//! The service's shell around the pages: the HTML document ([`shell`],
//! server-rendered) and the app root ([`App`], what hydration mounts),
//! `AppShell`'s rail with the services (Search, Learn), the theme and
//! languages.

use leptos::prelude::*;
use leptos_i18n::td_string;
#[cfg(feature = "ssr")]
use leptos_meta::{HashedStylesheet, MetaTags};
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Router, Routes};
use leptos_router::hooks::use_location;
use palicanon_ui_lib::{AppShell, IconName, NavEntry, NavList, NavSection, provide_language};
#[cfg(feature = "ssr")]
use palicanon_ui_lib::{HydrationRoot, language_attribute, theme_attribute};

use crate::catalogue::strings;
use crate::dictionary::web_dictionary;
use crate::fragment::{LearnProvider, LearnRoutes, LearnSettings};
#[cfg(feature = "ssr")]
use crate::release::{ENV_META, ReleaseInfo, SHA_META};
use crate::{web_files, web_hint, web_store, web_sync};

mod site_footer;
use site_footer::SiteFooter;

/// The HTML document around the app, rendered on the server. `data-theme`
/// and `lang` follow the request's cookies ([`theme_attribute`],
/// [`language_attribute`]), so the first paint never flashes the wrong
/// one. The stylesheet and the scripts are linked by their hashed names
/// (`hash-files` in `Cargo.toml`), so a new build is never served from
/// the browser's cache. The app sits in ui-lib's [`HydrationRoot`]
/// (`<div id="pc-app">`), where the browser's entry (`hydrate` in
/// `lib.rs`) starts: what Cloudflare adds to `<body>` before or after it
/// (a decoy link first, the beacon after) is never visited (task 040).
/// The deploy's release ([`ReleaseInfo::server`]) is provided to the app
/// (the footer's badge) and written in `<head>` for the browser.
#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    document(options, ReleaseInfo::server().clone())
}

/// [`shell`] for a given release (a function of its argument, so a test
/// renders every case): `release` is provided to the app, written in
/// `<head>` as two plain metas ([`ENV_META`], [`SHA_META`]; the sha one
/// always, empty for a tag) and read back by the browser before it mounts.
#[cfg(feature = "ssr")]
fn document(options: LeptosOptions, release: ReleaseInfo) -> impl IntoView {
    provide_context(release.clone());
    view! {
        <!DOCTYPE html>
        <html lang=language_attribute() data-theme=theme_attribute()>
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name=ENV_META content=release.env />
                <meta name=SHA_META content=release.sha />
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

/// The rail's entries: the services, Search (the main site), Learn (this
/// one, the current entry on `/`), then Map. The same three, in the same
/// order and with the same hrefs, as search-front's and vizualisation's rails.
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
                // `IconName::Motion` stands in until ui-lib has a Map icon, as in
                // vizualisation.
                NavEntry {
                    label: td_string!(t.locale(), nav_map),
                    icon: IconName::Motion,
                    href: "https://map.palicanon.app",
                },
            ],
        }]
    });
    NavList::new(sections, current)
}

/// The shell around the page: `AppShell`'s rail, theme and languages, its
/// Settings with 'Your progress' after Appearance (export, import), and
/// the Learn routes (the home at `/`, which holds the progress and runs the
/// review in place), the footer in the shell's slot. The site's progress
/// lives in the browser ([`web_store`], [`web_files`]) and syncs through
/// the learner's Google Drive ([`web_sync`]); the provider
/// renders the import's confirmation after the shell, outside Settings.
#[component]
fn Layout() -> impl IntoView {
    let location = use_location();
    view! {
        <LearnProvider
            store=web_store()
            files=web_files()
            sync=web_sync()
            hint=Some(web_hint())
            dictionary=Some(web_dictionary())
        >
            <AppShell
                nav=nav(location.pathname.into())
                settings_extra=move || view! { <LearnSettings /> }
                footer=|| view! { <SiteFooter /> }
            >
                <Routes fallback=|| "Not found.">
                    <LearnRoutes />
                </Routes>
            </AppShell>
        </LearnProvider>
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
            provide_context(ReleaseInfo::from_vars(
                Some("main".to_owned()),
                Some("34c489c".to_owned()),
            ));
            view! {
                <Router>
                    <Layout />
                </Router>
            }
            .to_html()
        })
    }

    /// The whole document of [`document`] for `path`, `cookie` as the
    /// request's `Cookie` header, on a local run's release, with the head
    /// the meta context collected injected the way `leptos_axum` does.
    fn render_document(path: &str, cookie: Option<&str>) -> String {
        let options = LeptosOptions::builder().output_name("learn-pali").build();
        render_document_with(options, path, cookie, ReleaseInfo::from_vars(None, None))
    }

    /// [`render_document`] on `path` `/`, for the given release.
    fn render_release(env: &str, sha: &str) -> String {
        let options = LeptosOptions::builder().output_name("learn-pali").build();
        let release = ReleaseInfo::from_vars(Some(env.to_owned()), Some(sha.to_owned()));
        render_document_with(options, "/", None, release)
    }

    /// [`render_document`] with the given options and release.
    fn render_document_with(
        options: LeptosOptions,
        path: &str,
        cookie: Option<&str>,
        release: ReleaseInfo,
    ) -> String {
        let (server, output) = ServerMetaContext::new();
        let body = Owner::new().with(|| {
            let mut request = axum::http::Request::builder().uri(path);
            if let Some(cookie) = cookie {
                request = request.header("cookie", cookie);
            }
            provide_context(request.body(()).expect("a request").into_parts().0);
            provide_context(server);
            provide_context(RequestUrl::new(path));
            document(options, release).to_html()
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
    fn the_rail_lists_search_then_learn_then_map() {
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
        let map = tags
            .iter()
            .position(|t| t.contains(r#"href="https://map.palicanon.app""#))
            .expect(&html);
        assert!(
            search < learn && learn < map,
            "Search first, Learn second, Map third: {html}"
        );
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
            if tag.contains(r#"href="https://search.palicanon.app""#)
                || tag.contains(r#"href="https://map.palicanon.app""#)
            {
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
        for (language, search, learn, map) in [
            (Language::En, "Search", "Learn", "Map"),
            (Language::Fr, "Recherche", "Apprendre", "Carte"),
            (Language::Vi, "Tìm kiếm", "Học", "Bản đồ"),
        ] {
            let html = render_layout(language, "/");
            assert!(html.contains(&format!(">{search}</span>")), "{html}");
            assert!(html.contains(&format!(">{learn}</span>")), "{html}");
            assert!(html.contains(&format!(">{map}</span>")), "{html}");
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
        // The file chooser is the host's (`FileTransfer::pick_import`): the
        // web impl makes its input when Import is pressed, none is in the
        // markup (048).
        assert_eq!(html.matches(r#"type="file""#).count(), 0, "{html}");
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

    /// The one page leads to neither `/progress` nor `/review`. Read on its
    /// link targets (`href`, `action`), never on its text: the footer's
    /// badge prints the release's name, and a dispatched branch may be named
    /// `…/progress-…`.
    #[test]
    fn the_page_links_to_no_removed_path() {
        let html = render_layout(Language::En, "/");
        let targets: Vec<&str> = ["href=\"", "action=\""]
            .iter()
            .flat_map(|attribute| html.split(attribute).skip(1))
            .map(|rest| &rest[..rest.find('"').expect("a closed attribute")])
            .collect();
        assert!(targets.contains(&"/"), "the links are read: {targets:?}");
        for target in &targets {
            let path = target.split(['?', '#']).next().unwrap_or_default();
            for removed in ["/progress", "/review"] {
                assert!(
                    path != removed && !path.starts_with(&format!("{removed}/")),
                    "{target}: {html}"
                );
            }
        }
    }

    /// The server is built without `web-sync`: it has no sync transport,
    /// so its first paint shows no sync UI (never a Sign in that does
    /// nothing); the browser shows it once hydrated.
    #[test]
    fn the_server_renders_no_sync_ui() {
        let html = render_layout(Language::En, "/");
        assert!(
            html.contains(">Your progress</h3>"),
            "Settings is there: {html}"
        );
        assert!(!html.contains("Sync with Google Drive"), "{html}");
        assert!(!html.contains("data-sync"), "{html}");
        assert!(!html.contains(">Sync<"), "no chip: {html}");
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
        let html = render_document_with(options, "/", None, ReleaseInfo::from_vars(None, None));
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

    /// The selected deck is a cookie too (073): the server paints the
    /// chosen deck, not the words deck with a switch after hydration.
    #[test]
    fn the_document_paints_the_deck_the_cookie_selects() {
        let html = render_document("/", Some("pc_learn_deck=devanagari"));
        let at = html
            .find("Devan\u{101}gar\u{12b}")
            .expect("the chooser names the deck");
        let chip = &html[html[..at].rfind("<button").expect("a chip")..at];
        assert!(chip.contains(r#"aria-pressed="true""#), "{chip}");
        let words = html.find(">Words<").expect("the words chip");
        let chip = &html[html[..words].rfind("<button").expect("a chip")..words];
        assert!(chip.contains(r#"aria-pressed="false""#), "{chip}");
        assert_eq!(
            html.matches(r#"aria-pressed="true""#).count(),
            1,
            "one deck selected: {html}"
        );
    }

    /// A cookie naming a deck this build does not teach, or empty (the
    /// cookie is shared by the subdomains): the words deck, no panic.
    #[test]
    fn a_cookie_naming_no_deck_of_this_build_paints_the_words_deck() {
        for cookie in ["pc_learn_deck=esperanto", "pc_learn_deck="] {
            let html = render_document("/", Some(cookie));
            let words = html.find(">Words<").expect("the words chip");
            let chip = &html[html[..words].rfind("<button").expect("a chip")..words];
            assert!(chip.contains(r#"aria-pressed="true""#), "{cookie}: {chip}");
            assert_eq!(
                html.matches(r#"aria-pressed="true""#).count(),
                1,
                "{cookie}"
            );
        }
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

    /// The `<meta name content>` of the document's head, as the server
    /// writes it.
    fn meta(name: &str, content: &str) -> String {
        format!(r#"<meta name="{name}" content="{content}">"#)
    }

    /// The document's `<head>` … `</head>`.
    fn head(html: &str) -> &str {
        let start = crate::test_support::position(html, "<head>");
        &html[start..crate::test_support::position(html, "</head>")]
    }

    /// The release's four cases, the deploy's variables to the metas and
    /// the badge's title: a tag (no sha), `main`, a dispatched branch, a
    /// local run (a blank env drops a sha with it).
    const CASES: [(&str, &str, &str, &str, &str); 5] = [
        ("v0.1.8", "", "v0.1.8", "", "v0.1.8"),
        ("main", "34c489c", "main", "34c489c", "main · 34c489c"),
        ("feat/x", "34c489c", "feat/x", "34c489c", "feat/x · 34c489c"),
        ("", "", "dev", "", "dev"),
        ("  ", "34c489c", "dev", "", "dev"),
    ];

    #[test]
    fn the_head_carries_the_release_in_two_metas_after_the_viewport() {
        for (env, sha, want_env, want_sha, _) in CASES {
            let html = render_release(env, sha);
            let head = head(&html);
            let viewport = head.find(r#"name="viewport""#).expect(head);
            let env_at = head.find(&meta(ENV_META, want_env)).expect(head);
            let sha_at = head.find(&meta(SHA_META, want_sha)).expect(head);
            assert!(viewport < env_at && env_at < sha_at, "{env}: {head}");
            assert_eq!(head.matches(r#"name="learn-release-"#).count(), 2, "{head}");
        }
    }

    /// A tag has no sha, and the sha meta is there all the same: the client
    /// tells "a tag" from "no metas" by its presence.
    #[test]
    fn a_tag_writes_the_sha_meta_empty() {
        let html = render_release("v0.1.8", "");
        assert!(
            head(&html).contains(r#"<meta name="learn-release-sha" content="">"#),
            "{html}"
        );
    }

    /// The badge is rendered from the same value as the metas: on the whole
    /// document, a tag's has no sha and no separator, `main` and a branch
    /// have both, a local run is `dev`.
    #[test]
    fn the_footer_badge_shows_the_same_release() {
        for (env, sha, _, _, title) in CASES {
            let html = render_release(env, sha);
            let footer = html.find("<footer").expect(&html);
            let at = footer + html[footer..].find("title=").expect(&html);
            assert!(
                html[at..].starts_with(&format!(r#"title="{title}""#)),
                "{title}: {html}"
            );
            let nav = footer + html[footer..].find("<nav").expect(&html);
            if !sha.is_empty() && !env.trim().is_empty() {
                assert!(html[at..nav].contains(sha), "{html}");
            } else {
                assert!(!html[at..nav].contains('·'), "no separator: {html}");
            }
        }
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
