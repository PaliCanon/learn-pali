//! The home's placeholder content, until the learning scope is defined: the
//! brand (lotus and name, hidden on a phone where the shell's top bar
//! already shows them), a title and one line saying the lessons come next.

use leptos::prelude::*;
use leptos_i18n::td_string;

use crate::catalogue::strings;

/// The home: the brand, the title, the hint.
#[component]
pub fn Home() -> impl IntoView {
    let t = strings();
    view! {
        <div class="flex flex-col gap-6 px-6 pt-6 pb-12">
            <BrandLink />
            <h1 class="font-serif text-3xl font-semibold text-heading">
                {move || td_string!(t.locale(), home_title)}
            </h1>
            <p class="text-muted">{move || td_string!(t.locale(), home_hint)}</p>
        </div>
    }
}

/// The lotus and "PaliCanon", a link home. Hidden on a phone, where the
/// shell's top bar already shows them.
#[component]
fn BrandLink() -> impl IntoView {
    view! {
        <a
            href="/"
            class="hidden shrink-0 items-center gap-2 font-serif text-lg font-semibold text-heading select-none sm:flex"
        >
            <img src="/lotus.png" alt="" width="22" height="22" />
            "PaliCanon"
        </a>
    }
}

#[cfg(test)]
mod tests {
    use palicanon_ui_lib::Language;

    use super::*;

    fn render(language: Language) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            view! { <Home /> }.to_html()
        })
    }

    #[test]
    fn the_home_has_its_title_and_hint_in_every_language() {
        for (language, title, hint) in [
            (Language::En, "Learn Pāli", "The lessons are coming next."),
            (
                Language::Fr,
                "Apprendre le pāli",
                "Les leçons arrivent bientôt.",
            ),
            (Language::Vi, "Học tiếng Pāli", "Các bài học sẽ đến sau."),
        ] {
            let html = render(language);
            assert_eq!(html.matches("<h1").count(), 1, "{html}");
            assert!(html.contains(&format!(">{title}</h1>")), "{html}");
            assert!(html.contains(&format!(">{hint}</p>")), "{html}");
        }
    }

    #[test]
    fn the_brand_is_hidden_on_a_phone_and_shows_the_lotus() {
        let html = render(Language::En);
        let start = html.find("<a ").expect(&html);
        let tag = &html[start..start + html[start..].find('>').expect(&html)];
        assert!(tag.contains(r#"href="/""#), "{tag}");
        assert!(tag.contains("hidden"), "{tag}");
        assert!(tag.contains("sm:flex"), "{tag}");
        assert!(html.contains(r#"<img src="/lotus.png""#), "{html}");
        assert!(html.contains("PaliCanon"), "{html}");
    }

    #[test]
    fn the_home_renders_no_footer_of_its_own() {
        assert!(!render(Language::En).contains("<footer"));
    }
}
