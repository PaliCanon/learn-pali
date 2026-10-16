//! The page footer: ui-lib's [`Footer`] with the site's links
//! (Acknowledgements · Terms · Privacy · Support, the main site's pages at
//! `https://palicanon.app/…`: their texts live there, none is copied here),
//! as on today's site — no Legal. This build's env badge sits here at the
//! footer's left, handed from `crate::app`'s `BADGE`. The shell's `footer`
//! slot owns the wrapper that pushes it to the bottom of a short page: this
//! component adds none.

use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{EnvBadge, Footer, FooterLink};

use super::BADGE;
use crate::catalogue::{Strings, strings};

/// The footer, handed to the shell's `footer` slot.
#[component]
pub fn SiteFooter() -> impl IntoView {
    let t = strings();
    move || {
        view! {
            <Footer
                links=links(t)
                credits=Vec::new()
                leading=|| view! { <EnvBadge env=BADGE.env sha=BADGE.sha /> }
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

    use super::BADGE;
    use super::*;

    fn render() -> String {
        Owner::new().with(|| view! { <SiteFooter /> }.to_html())
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

    /// The build badge sits at the footer's left, before the links — first
    /// in the phone's column, first grid column on desktop.
    #[test]
    fn the_footer_carries_the_build_badge_at_its_left() {
        let html = render();
        let badge = html
            .find(&format!(r#"title="{} · {}""#, BADGE.env, BADGE.sha))
            .expect(&html);
        let nav = html.find("<nav").expect(&html);
        assert!(badge < nav, "{html}");
        let start = html[..badge].rfind("<div").expect(&html);
        let tag = &html[start..start + html[start..].find('>').expect(&html)];
        assert!(tag.contains("sm:col-start-1"), "{tag}");
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
