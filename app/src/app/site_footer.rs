//! The page footer: ui-lib's [`Footer`] with the site's links
//! (Acknowledgements · Terms · Privacy · Support, the main site's pages at
//! `https://palicanon.app/…`: their texts live there, none is copied here),
//! as on today's site — no Legal. This deploy's release badge sits here at the
//! footer's left, from the [`ReleaseInfo`] the document provides. The shell's `footer`
//! slot owns the wrapper that pushes it to the bottom of a short page: this
//! component adds none.

use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{EnvBadge, Footer, FooterLink};

use crate::catalogue::{Strings, strings};
use crate::release::ReleaseInfo;

/// The footer, handed to the shell's `footer` slot.
#[component]
pub fn SiteFooter() -> impl IntoView {
    let t = strings();
    let info = use_context::<ReleaseInfo>().unwrap_or_else(ReleaseInfo::unknown);
    move || {
        let info = info.clone();
        view! {
            <Footer
                links=links(t)
                credits=Vec::new()
                leading=move || view! { <EnvBadge env=info.env.clone() sha=info.sha.clone() /> }
            />
        }
    }
}

/// The four pages, in today's order, labelled in `t`'s language.
fn links(t: Strings) -> Vec<FooterLink> {
    let locale = t.locale();
    vec![
        FooterLink {
            label: td_string!(locale, footer_acknowledgements),
            href: concat!("https://palicanon.app", "/acknowledgements"),
        },
        FooterLink {
            label: td_string!(locale, footer_terms),
            href: concat!("https://palicanon.app", "/terms"),
        },
        FooterLink {
            label: td_string!(locale, footer_privacy),
            href: concat!("https://palicanon.app", "/privacy"),
        },
        FooterLink {
            label: td_string!(locale, footer_support),
            href: concat!("https://palicanon.app", "/support"),
        },
    ]
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use palicanon_ui_lib::{AppShell, Language};

    use super::*;

    /// The footer with `info` provided as the release context.
    fn render_for(info: ReleaseInfo) -> String {
        Owner::new().with(|| {
            provide_context(info);
            view! { <SiteFooter /> }.to_html()
        })
    }

    /// The footer on a `main` deploy.
    fn render() -> String {
        render_for(ReleaseInfo::from_vars(
            Some("main".to_owned()),
            Some("34c489c".to_owned()),
        ))
    }

    /// Without the hydration markers (`<!--…-->`) between elements.
    fn plain(html: &str) -> String {
        let mut plain = html.to_owned();
        while let Some(start) = plain.find("<!--") {
            let end = start + plain[start..].find("-->").expect("comment end") + 3;
            plain.replace_range(start..end, "");
        }
        plain
    }

    #[test]
    fn the_footer_links_the_four_pages_of_the_main_site_and_no_legal() {
        let html = render();
        let mut last = 0;
        for (href, label) in [
            ("https://palicanon.app/acknowledgements", "Acknowledgements"),
            ("https://palicanon.app/terms", "Terms"),
            ("https://palicanon.app/privacy", "Privacy"),
            ("https://palicanon.app/support", "Support"),
        ] {
            let at = html.find(&format!(r#"<a href="{href}""#)).expect(&html);
            assert!(at > last, "{html}");
            last = at;
            assert!(html.contains(&format!(">{label}</a>")), "{html}");
        }
        assert_eq!(html.matches("<a ").count(), 4, "exactly four links: {html}");
        assert!(
            !html.contains("/legal") && !html.contains("Legal"),
            "{html}"
        );
    }

    #[test]
    fn the_footer_labels_follow_the_language() {
        for (language, labels) in [
            (
                Language::Fr,
                ["Crédits", "Conditions", "Confidentialité", "Soutenir"],
            ),
            (
                Language::Vi,
                ["Tri ân", "Điều khoản", "Quyền riêng tư", "Ủng hộ"],
            ),
        ] {
            let html = Owner::new().with(|| {
                palicanon_ui_lib::provide_language().set(language);
                view! { <SiteFooter /> }.to_html()
            });
            for label in labels {
                assert!(html.contains(&format!(">{label}</a>")), "{html}");
            }
        }
    }

    /// The release badge sits at the footer's left, before the links: first
    /// in the phone's column, first grid column on desktop.
    #[test]
    fn the_footer_carries_the_release_badge_at_its_left() {
        let html = render();
        let badge = html.find(r#"title="main · 34c489c""#).expect(&html);
        let nav = html.find("<nav").expect(&html);
        assert!(badge < nav, "{html}");
        let start = html[..badge].rfind("<div").expect(&html);
        let tag = &html[start..start + html[start..].find('>').expect(&html)];
        assert!(tag.contains("sm:col-start-1"), "{tag}");
    }

    /// A tag's badge is its name alone: no sha, no separator.
    #[test]
    fn a_tags_badge_has_no_sha_and_no_separator() {
        let html = render_for(ReleaseInfo::from_vars(Some("v0.1.8".to_owned()), None));
        assert!(html.contains(r#"title="v0.1.8""#), "{html}");
        assert!(html.contains(">v0.1.8<"), "{html}");
        assert!(!html.contains('·'), "{html}");
    }

    /// With no release provided (a host that did not), the badge says
    /// `unknown`, never `dev`.
    #[test]
    fn without_a_release_the_badge_says_unknown() {
        let html = Owner::new().with(|| view! { <SiteFooter /> }.to_html());
        assert!(html.contains(r#"title="unknown""#), "{html}");
        assert!(!html.contains(r#"title="dev""#), "{html}");
        assert!(!html.contains('·'), "{html}");
    }

    /// No credits: the footer renders no credits list.
    #[test]
    fn the_footer_has_no_credits() {
        assert!(!render().contains("<ul"), "{}", render());
    }

    /// The shell's slot owns the wrapper: no second one here.
    #[test]
    fn the_footer_adds_no_wrapper_of_its_own() {
        let html = render();
        assert!(html.starts_with("<footer"), "{html}");
        assert!(!html.contains("mt-auto"), "{html}");
        assert!(!html.contains("min-h-dvh"), "{html}");
    }

    /// Long page: 200 paragraphs, the footer after the last one, in the
    /// shell's slot (`mt-auto`, last child of the page column).
    #[test]
    fn a_long_page_has_the_footer_after_its_content() {
        let html = plain(&Owner::new().with(|| {
            view! {
                <AppShell footer=|| {
                    view! { <SiteFooter /> }
                }>
                    {(0..200).map(|n| view! { <p>{format!("line {n}")}</p> }).collect_view()}
                </AppShell>
            }
            .to_html()
        }));
        let last = html.find("line 199").expect(&html);
        let footer = html.find("<footer").expect(&html);
        assert!(last < footer, "{html}");
        assert!(html.contains(r#"<div class="mt-auto"><footer"#), "{html}");
        assert_eq!(html.matches("<footer").count(), 1, "{html}");
    }
}
