//! The new-terms control, on the home and the summary: a Segmented picks
//! how many (+5 by default, +10, Custom with an inline number field,
//! 1 ≤ n ≤ the new terms left), then one 'Add' Button acts (Q5). The
//! summary shows [`NewTerms`], picker and 'Add' together; the home's dialog
//! shows [`NewTermsPicker`] in its body and [`AddButton`] in its footer,
//! sharing one [`Pick`].

use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, Segmented, SegmentedOption, Size, Variant};

use crate::catalogue::strings;

/// What the Segmented picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    /// +5, the default.
    Five,
    /// +10.
    Ten,
    /// A number typed in the field.
    Custom,
}

/// What the picker holds: the choice and the text typed for Custom.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pick {
    /// What is picked.
    choice: RwSignal<Choice>,
    /// The text typed in the number field.
    custom: RwSignal<String>,
}

impl Pick {
    /// +5 picked, nothing typed.
    pub(crate) fn new() -> Self {
        Pick {
            choice: RwSignal::new(Choice::Five),
            custom: RwSignal::new(String::new()),
        }
    }

    /// 'Add' pressed: calls `on_add` with the amount picked, unless nothing
    /// is left.
    fn add(self, new_left: usize, on_add: Callback<usize>) {
        press_add(
            self.choice.get_untracked(),
            &self.custom.get_untracked(),
            new_left,
            on_add,
        );
    }
}

/// Picks a number of new terms and adds them: a Segmented (+5 · +10 ·
/// Custom, +5 by default), the number field when Custom, one 'Add' Button.
#[component]
pub fn NewTerms(
    /// Called with the number picked when 'Add' is pressed.
    #[prop(into)]
    on_add: Callback<usize>,
    /// The deck's terms not started yet: Custom's upper bound.
    new_left: usize,
) -> impl IntoView {
    view! { <NewTermsForm pick=Pick::new() new_left=new_left on_add=on_add /> }
}

/// The number to add: the choice, or the typed text for Custom, kept in
/// `1..=new_left` (0 when nothing is left).
fn amount(choice: Choice, text: &str, new_left: usize) -> usize {
    let wanted = match choice {
        Choice::Five => 5,
        Choice::Ten => 10,
        Choice::Custom => text.trim().parse().unwrap_or(1),
    };
    if new_left == 0 {
        0
    } else {
        wanted.clamp(1, new_left)
    }
}

/// 'Add' pressed: calls `on_add` with the [`amount`], unless nothing is left.
fn press_add(choice: Choice, text: &str, new_left: usize, on_add: Callback<usize>) {
    let n = amount(choice, text, new_left);
    if n > 0 {
        on_add.run(n);
    }
}

/// The Segmented, the number field when Custom, and 'Add'.
#[component]
fn NewTermsForm(
    /// What is picked.
    pick: Pick,
    /// The deck's terms not started yet.
    new_left: usize,
    /// Called with the amount when 'Add' is pressed.
    on_add: Callback<usize>,
) -> impl IntoView {
    view! {
        <div data-new-terms="" class="flex flex-wrap items-center gap-3">
            <PickerFields pick=pick new_left=new_left />
            <AddButton pick=pick new_left=new_left on_add=on_add />
        </div>
    }
}

/// The picker alone, its 'Add' elsewhere (the home's dialog footer): the
/// Segmented and the number field when Custom.
#[component]
pub(crate) fn NewTermsPicker(
    /// What is picked, shared with the [`AddButton`].
    pick: Pick,
    /// The deck's terms not started yet: Custom's upper bound.
    new_left: usize,
) -> impl IntoView {
    view! {
        <div data-new-terms="" class="flex flex-wrap items-center gap-3">
            <PickerFields pick=pick new_left=new_left />
        </div>
    }
}

/// 'Add', Primary: calls `on_add` with the amount `pick` holds; disabled
/// when nothing is left.
#[component]
pub(crate) fn AddButton(
    /// What is picked.
    pick: Pick,
    /// The deck's terms not started yet.
    new_left: usize,
    /// Called with the amount when pressed.
    on_add: Callback<usize>,
) -> impl IntoView {
    let t = strings();
    view! {
        <Button
            variant=Variant::Primary
            size=Size::Md
            disabled=Signal::derive(move || new_left == 0)
            on_click=move |_| pick.add(new_left, on_add)
        >
            {move || td_string!(t.locale(), new_add)}
        </Button>
    }
}

/// The Segmented (+5 · +10 · Custom) and, when Custom, the number field.
#[component]
fn PickerFields(pick: Pick, new_left: usize) -> impl IntoView {
    let t = strings();
    let Pick { choice, custom } = pick;
    view! {
        {move || {
            let locale = t.locale();
            view! {
                <Segmented
                    aria_label=td_string!(locale, new_count_label)
                    options=vec![
                        SegmentedOption {
                            label: "+5",
                            value: Choice::Five,
                        },
                        SegmentedOption {
                            label: "+10",
                            value: Choice::Ten,
                        },
                        SegmentedOption {
                            label: td_string!(locale, new_custom),
                            value: Choice::Custom,
                        },
                    ]
                    value=choice
                    on_change=move |picked| choice.set(picked)
                />
            }
        }}
        {move || {
            (choice.get() == Choice::Custom)
                .then(|| view! { <NumberField custom=custom new_left=new_left /> })
        }}
    }
}

/// Custom's inline number field, 1 to `new_left`.
#[component]
fn NumberField(custom: RwSignal<String>, new_left: usize) -> impl IntoView {
    let t = strings();
    view! {
        <input
            type="number"
            inputmode="numeric"
            min="1"
            max=new_left
            aria-label=move || td_string!(t.locale(), new_count_label)
            class="w-24 rounded-lg border border-line bg-chip px-3 py-2 text-heading focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-gold"
            prop:value=move || custom.get()
            on:input=move |ev| custom.set(event_target_value(&ev))
        />
    }
}

/// These tests render markup on the server: they need `ssr`.
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::sync::{Arc, Mutex};

    use palicanon_ui_lib::Language;

    use super::*;

    fn form(language: Language, choice: Choice, custom: &str, new_left: usize) -> String {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(language);
            let pick = Pick {
                choice: RwSignal::new(choice),
                custom: RwSignal::new(custom.to_owned()),
            };
            let on_add = Callback::new(|_: usize| {});
            view! { <NewTermsForm pick=pick new_left=new_left on_add=on_add /> }.to_html()
        })
    }

    fn tag_before(html: &str, tag: &str, text: &str) -> String {
        let at = html
            .find(text)
            .unwrap_or_else(|| panic!("{text:?} in {html}"));
        let start = html[..at].rfind(&format!("<{tag}")).expect(html);
        let end = start + html[start..].find('>').expect(html);
        html[start..=end].to_owned()
    }

    fn has_attr(tag: &str, name: &str) -> bool {
        tag.trim_end_matches('>')
            .split_whitespace()
            .any(|token| token == name || token.starts_with(&format!("{name}=")))
    }

    #[test]
    fn it_offers_plus_five_plus_ten_and_custom_with_plus_five_picked() {
        let html = form(Language::En, Choice::Five, "", 20);
        assert!(html.contains(r#"role="radiogroup""#), "{html}");
        for label in ["+5", "+10", "Custom"] {
            assert!(html.contains(label), "{label}: {html}");
        }
        assert!(
            tag_before(&html, "button", "+5").contains(r#"aria-checked="true""#),
            "{html}"
        );
        assert!(
            tag_before(&html, "button", "+10").contains(r#"aria-checked="false""#),
            "{html}"
        );
    }

    #[test]
    fn the_number_field_shows_for_custom_only() {
        for choice in [Choice::Five, Choice::Ten] {
            let html = form(Language::En, choice, "", 20);
            assert!(!html.contains("<input"), "{choice:?}: {html}");
            assert!(html.contains(">Add<"), "{html}");
        }
        let html = form(Language::En, Choice::Custom, "7", 20);
        let input = tag_before(&html, "input", "type=\"number\"");
        assert!(input.contains(r#"min="1""#), "{input}");
        assert!(input.contains(r#"max="20""#), "{input}");
        assert!(html.contains(">Add<"), "{html}");
    }

    #[test]
    fn add_is_disabled_when_nothing_is_left() {
        let none = form(Language::En, Choice::Five, "", 0);
        assert!(
            has_attr(&tag_before(&none, "button", ">Add<"), "disabled"),
            "{none}"
        );
        let some = form(Language::En, Choice::Five, "", 3);
        assert!(
            !has_attr(&tag_before(&some, "button", ">Add<"), "disabled"),
            "{some}"
        );
    }

    #[test]
    fn the_labels_follow_the_language() {
        let fr = form(Language::Fr, Choice::Custom, "", 20);
        assert!(!fr.contains("Nouveaux termes"), "no label, HC: {fr}");
        for label in ["Autre", "Ajouter"] {
            assert!(fr.contains(label), "{label}: {fr}");
        }
        let vi = form(Language::Vi, Choice::Five, "", 20);
        for label in ["Tùy chọn", "Thêm"] {
            assert!(vi.contains(label), "{label}: {vi}");
        }
    }

    #[test]
    fn the_amount_is_the_choice_or_the_typed_number_kept_in_range() {
        for (choice, text, left, want) in [
            (Choice::Five, "", 20, 5),
            (Choice::Five, "", 3, 3),
            (Choice::Ten, "", 20, 10),
            (Choice::Ten, "", 7, 7),
            (Choice::Custom, "7", 20, 7),
            (Choice::Custom, " 4 ", 20, 4),
            (Choice::Custom, "0", 20, 1),
            (Choice::Custom, "99", 20, 20),
            (Choice::Custom, "", 20, 1),
            (Choice::Custom, "abc", 20, 1),
            (Choice::Custom, "-3", 20, 1),
            (Choice::Custom, "2.5", 20, 1),
            (Choice::Five, "", 0, 0),
            (Choice::Custom, "7", 0, 0),
        ] {
            assert_eq!(
                amount(choice, text, left),
                want,
                "{choice:?} {text:?} {left}"
            );
        }
    }

    #[test]
    fn pressing_add_calls_back_with_the_amount_and_never_with_nothing_left() {
        Owner::new().with(|| {
            let got = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&got);
            let on_add = Callback::new(move |n: usize| sink.lock().expect("lock").push(n));
            press_add(Choice::Custom, "12", 20, on_add);
            press_add(Choice::Custom, "99", 20, on_add);
            press_add(Choice::Five, "", 0, on_add);
            assert_eq!(*got.lock().expect("lock"), vec![12, 20]);
        });
    }
}
