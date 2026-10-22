//! The deck chooser, at the top of the home: one chip per deck the build
//! teaches, each with its own due count, so cards due in another deck are
//! seen from the one on screen. Pressing a chip selects that deck
//! ([`Backend::select`]): the home reads its progress again and every
//! screen works on it.
//!
//! With one deck the chooser renders nothing, so a build that teaches the
//! words deck alone looks as it always did. On a phone the chips wrap onto
//! further rows; each is a native button at least 44 px high (`Size::Md`),
//! with no hover-only interaction.

use learn_core::book::DeckId;
use learn_core::deck::Deck;
use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Language, Size, ToggleButton, Variant, language_signal};

use crate::backend::Backend;
use crate::catalogue::{Locale, strings};
use crate::components::data_block::StoreWrites;
use crate::learn::home_state::due_cards;
use crate::store::Load;
use crate::today::{browser_today, session_today};

/// The name of the deck `id` in `language`: the words deck has its string;
/// any other deck is named by its id until it has one.
pub(crate) fn deck_label(id: &DeckId, language: Language) -> String {
    if id.is_words() {
        td_string!(Locale::from(language), deck_words).to_owned()
    } else if *id == DeckId::devanagari() {
        td_string!(Locale::from(language), deck_devanagari).to_owned()
    } else {
        id.0.clone()
    }
}

/// How many cards are due in the deck `id` (`deck`), read from the whole
/// Book; 0 when the store holds nothing usable for it.
fn due_in(backend: Backend, id: &DeckId, deck: &Deck) -> usize {
    match backend.load_book() {
        Load::Ok(book) => {
            let state = book.state(id);
            let today = session_today(browser_today(), &state);
            due_cards(deck, &state, today)
        }
        _ => 0,
    }
}

/// Selects `id` and tells the home the stored progress to show changed
/// (`writes`, when the layout provides it): the home reads it again.
fn pick(backend: Backend, writes: Option<StoreWrites>, id: &DeckId) {
    backend.select(id);
    if let Some(StoreWrites(writes)) = writes {
        writes.update(|writes| *writes += 1);
    }
}

/// Whether the chooser is worth showing: the build teaches more than one
/// deck. The home mounts [`DeckChooser`] only then, so a single deck leaves
/// no node of the chooser's in the markup, not even a marker.
pub(crate) fn offered(backend: Backend) -> bool {
    backend.decks().len() > 1
}

/// The chips, one per deck, or nothing while the build teaches one deck.
#[component]
pub(crate) fn DeckChooser() -> impl IntoView {
    let backend = Backend::current();
    let t = strings();
    let writes = use_context::<StoreWrites>();
    offered(backend).then(|| {
        view! {
            <div
                role="group"
                aria-label=move || td_string!(t.locale(), deck_chooser_label)
                class="flex flex-wrap items-center justify-center gap-2"
            >
                {backend
                    .decks()
                    .iter()
                    .map(|entry| {
                        view! {
                            <DeckChip
                                backend=backend
                                writes=writes
                                id=entry.id.clone()
                                deck=entry.deck
                            />
                        }
                    })
                    .collect_view()}
            </div>
        }
    })
}

/// One chip: the deck's name and its due count, pressed while the deck is
/// the selected one. The button flips its own `pressed`; two watchers keep
/// it equal to 'this deck is selected' and turn a press into a selection
/// (pressing the selected chip again leaves it pressed).
#[component]
fn DeckChip(
    /// The host's backend.
    backend: Backend,
    /// What a selection tells the home.
    writes: Option<StoreWrites>,
    /// The deck's id.
    id: DeckId,
    /// The deck, to count its due cards.
    deck: &'static Deck,
) -> impl IntoView {
    let pressed = RwSignal::new(untrack(|| backend.selected()) == id);
    let mine = id.clone();
    Effect::watch(
        move || backend.selected(),
        move |selected, _, _| pressed.set(*selected == mine),
        false,
    );
    let mine = id.clone();
    Effect::watch(
        move || pressed.get(),
        move |on, _, _| {
            let selected = backend.selected() == mine;
            if *on && !selected {
                pick(backend, writes, &mine);
            } else if !*on && selected {
                pressed.set(true);
            }
        },
        false,
    );
    let name = id.clone();
    let due = move || {
        if let Some(StoreWrites(writes)) = writes {
            writes.track();
        }
        due_in(backend, &id, deck)
    };
    view! {
        <ToggleButton pressed=pressed variant=Variant::Chip size=Size::Md>
            <span>{move || deck_label(&name, language_signal().get())}</span>
            <span class="tabular-nums">{due}</span>
        </ToggleButton>
    }
}

#[cfg(test)]
mod tests {
    use learn_core::book::{Book, DeckId};
    use learn_core::progress::State;
    use learn_core::session::Session;
    use palicanon_ui_lib::Language;

    use super::*;
    use crate::components::data_block::StoreWrites;
    use crate::deck::{Decks, deck};
    use crate::learn::home_state::due_cards;
    use crate::store::Load;
    use crate::test_support::{
        FakeFiles, FakeStore, backend_with_decks, second_deck, today, two_decks,
    };

    fn zz() -> DeckId {
        DeckId("zz".into())
    }

    /// A state with the first `n` terms of `deck` started today.
    fn started(deck: &'static learn_core::deck::Deck, n: usize) -> State {
        let mut session = Session::start(deck, State::empty(), today(), 0).expect("a session");
        assert_eq!(session.request_new(n), n);
        session.into_state()
    }

    /// Words with 3 terms started and `zz` with 2: both have cards due.
    fn book() -> Book {
        let mut book = Book::from_state(started(deck(), 3));
        book.set(zz(), started(second_deck(), 2));
        book
    }

    /// The chooser as the server renders it, over `decks` and `book`,
    /// `selected` remembered by the store (the words deck when none).
    fn render(decks: Decks, book: Load<Book>, selected: Option<&str>) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(Language::En);
                let (store, files) = (FakeStore::with_book(book), FakeFiles::default());
                if let Some(id) = selected {
                    store.remember_selected(id);
                }
                provide_context(backend_with_decks(&store, &files, decks));
                view! { <DeckChooser /> }.to_html()
            })
            .replace("<!>", "")
    }

    /// The chips' markup, `<button ...>...</button>`, in order.
    fn chips(html: &str) -> Vec<&str> {
        html.split("<button")
            .skip(1)
            .map(|chip| chip.split("</button>").next().expect(html))
            .collect()
    }

    /// The due count a chip shows: the text of its last `<span>`.
    fn shown_count(chip: &str) -> usize {
        let at = chip.rfind("tabular-nums\">").expect(chip) + "tabular-nums\">".len();
        let end = at + chip[at..].find('<').expect(chip);
        chip[at..end].parse().expect(chip)
    }

    #[test]
    fn with_one_deck_the_chooser_renders_nothing() {
        let html = render(crate::test_support::one_deck(), Load::Ok(book()), None);
        assert!(!html.contains(r#"role="group""#), "{html}");
        assert_eq!(chips(&html).len(), 0, "{html}");
    }

    #[test]
    fn the_chooser_is_offered_from_two_decks() {
        Owner::new().with(|| {
            let (store, files) = (FakeStore::with_book(Load::Empty), FakeFiles::default());
            assert!(!offered(backend_with_decks(
                &store,
                &files,
                crate::test_support::one_deck()
            )));
            assert!(offered(backend_with_decks(&store, &files, two_decks())));
        });
    }

    #[test]
    fn with_two_decks_it_renders_two_chips_the_selected_one_pressed() {
        let html = render(two_decks(), Load::Ok(book()), None);
        assert!(html.contains(r#"role="group""#), "{html}");
        assert!(html.contains(r#"aria-label="Deck""#), "{html}");
        let chips = chips(&html);
        assert_eq!(chips.len(), 2, "{html}");
        assert!(chips[0].contains("Words"), "{html}");
        assert!(chips[0].contains(r#"aria-pressed="true""#), "{html}");
        assert!(chips[1].contains("zz"), "{html}");
        assert!(chips[1].contains(r#"aria-pressed="false""#), "{html}");
    }

    #[test]
    fn the_pressed_chip_is_the_stored_selection() {
        let html = render(two_decks(), Load::Ok(book()), Some("zz"));
        let chips = chips(&html);
        assert!(chips[0].contains(r#"aria-pressed="false""#), "{html}");
        assert!(chips[1].contains(r#"aria-pressed="true""#), "{html}");
    }

    #[test]
    fn a_stored_id_the_registry_lacks_presses_the_words_chip() {
        let html = render(two_decks(), Load::Ok(book()), Some("klingon"));
        let chips = chips(&html);
        assert!(chips[0].contains(r#"aria-pressed="true""#), "{html}");
        assert!(chips[1].contains(r#"aria-pressed="false""#), "{html}");
    }

    #[test]
    fn each_chip_shows_its_own_decks_due_count() {
        let book = book();
        let words = due_cards(deck(), &book.state(&DeckId::words()), today());
        let other = due_cards(second_deck(), &book.state(&zz()), today());
        assert_ne!(words, other, "the fixture tells the decks apart");
        assert!(words > 0 && other > 0);
        let html = render(two_decks(), Load::Ok(book), Some("zz"));
        let chips = chips(&html);
        assert_eq!(shown_count(chips[0]), words, "{html}");
        assert_eq!(shown_count(chips[1]), other, "{html}");
    }

    #[test]
    fn a_deck_with_nothing_due_shows_a_zero() {
        let html = render(two_decks(), Load::Empty, None);
        let chips = chips(&html);
        assert_eq!(shown_count(chips[0]), 0, "{html}");
        assert_eq!(shown_count(chips[1]), 0, "{html}");
    }

    #[test]
    fn on_a_phone_the_chips_wrap_and_each_is_a_tap_target() {
        let html = render(two_decks(), Load::Ok(book()), None);
        let group = &html[html.find(r#"role="group""#).expect(&html)..];
        let group = &group[..group.find('>').expect(&html)];
        assert!(group.contains("flex-wrap"), "{group}");
        for chip in chips(&html) {
            let class = chip.split(r#"class=""#).nth(1).expect(chip);
            let class = &class[..class.find('"').expect(chip)];
            assert!(
                class.split(' ').any(|token| token == "h-11"),
                "44 px: {class}"
            );
        }
    }

    #[test]
    fn pressing_a_chip_selects_its_deck_and_moves_the_writes() {
        Owner::new().with(|| {
            let (store, files) = (FakeStore::new(Load::Empty), FakeFiles::default());
            let backend = backend_with_decks(&store, &files, two_decks());
            let writes = StoreWrites(RwSignal::new(0));
            assert_eq!(backend.selected(), DeckId::words());
            pick(backend, Some(writes), &zz());
            assert_eq!(backend.selected(), zz());
            assert_eq!(store.selects(), vec![zz()]);
            assert_eq!(writes.0.get_untracked(), 1, "the home reads again");
            pick(backend, None, &DeckId::words());
            assert_eq!(backend.selected(), DeckId::words());
            assert_eq!(writes.0.get_untracked(), 1, "no layout, no counter");
        });
    }

    #[test]
    fn the_words_deck_is_named_in_every_language_and_another_by_its_id() {
        let words = DeckId::words();
        assert_eq!(deck_label(&words, Language::En), "Words");
        assert_eq!(deck_label(&words, Language::Fr), "Mots");
        assert_eq!(deck_label(&words, Language::Vi), "Từ");
        let devanagari = DeckId::devanagari();
        assert_eq!(deck_label(&devanagari, Language::En), "Devanāgarī");
        assert_eq!(deck_label(&devanagari, Language::Fr), "Dévanāgarī");
        assert_eq!(deck_label(&devanagari, Language::Vi), "Chữ Devanāgarī");
        assert_eq!(deck_label(&zz(), Language::En), "zz");
    }

    #[test]
    fn the_group_is_named_in_every_locale_without_an_em_dash() {
        let files = [
            include_str!("../../locales/en.json"),
            include_str!("../../locales/fr.json"),
            include_str!("../../locales/vi.json"),
        ];
        for text in files {
            for key in ["deck_chooser_label", "deck_words"] {
                let row = text
                    .lines()
                    .find(|line| line.contains(&format!("\"{key}\"")))
                    .unwrap_or_else(|| panic!("{key} is missing"));
                assert!(!row.contains('\u{2014}'), "{row}");
            }
        }
    }
}
