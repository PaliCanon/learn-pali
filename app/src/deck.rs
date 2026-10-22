//! The deck the site teaches: `core/fixtures/deck.json`, embedded in the
//! binary and the wasm with `include_str!` and parsed once, on first use
//! ([`deck`]). No fetch, so no network failure state in v1; the pipeline's
//! deck (task 004) replaces the file, and later the site serves it.

use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use learn_core::book::DeckId;
use learn_core::deck::{Deck, Term};

/// The embedded deck, parsed on first use. A parse failure is caught by
/// this module's test, never in a browser.
static DECK: LazyLock<Deck> = LazyLock::new(|| {
    Deck::from_json(include_str!("../../core/fixtures/deck.json"))
        .expect("the embedded deck parses (its test says so)")
});

/// The deck every page reads: the same `&'static` on every call.
pub(crate) fn deck() -> &'static Deck {
    &DECK
}

static DEVANAGARI: LazyLock<Deck> = LazyLock::new(|| {
    Deck::from_json(include_str!("../../core/fixtures/devanagari.json"))
        .expect("the embedded Devanāgarī deck parses (its test says so)")
});

/// The Devanāgarī alphabet deck (063b): a script deck built by
/// `build-deck devanagari` from the reviewed inventory table.
pub(crate) fn devanagari() -> &'static Deck {
    &DEVANAGARI
}

/// One deck this build teaches: its id in the progress [`Book`](learn_core::book::Book)
/// and the deck itself.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// The id of the deck's entry in the Book.
    pub(crate) id: DeckId,
    /// The deck, parsed once for the life of the page.
    pub(crate) deck: &'static Deck,
}

/// The decks compiled into the build, in the order they are offered; the
/// first is always the words deck, the default and the fallback of every
/// unknown selection. A cheaply cloned handle over a shared list.
#[derive(Debug, Clone)]
pub(crate) struct Decks(Rc<[Entry]>);

impl Decks {
    /// A registry of `entries`.
    ///
    /// # Panics
    /// When `entries` is empty or its first entry is not the words deck:
    /// the fallback of an unknown selection must exist.
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        assert!(
            entries.first().is_some_and(|entry| entry.id.is_words()),
            "the registry starts with the words deck"
        );
        Decks(entries.into())
    }

    /// How many decks the build teaches (at least one).
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// The decks, the words deck first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.0.iter()
    }

    /// The entry of `id`, if the build teaches it.
    pub(crate) fn get(&self, id: &DeckId) -> Option<&Entry> {
        self.0.iter().find(|entry| &entry.id == id)
    }

    /// The entry of `id`; the words entry when `id` is none or unknown (an
    /// id from a newer build, a stale setting).
    pub(crate) fn resolve(&self, id: Option<&DeckId>) -> &Entry {
        id.and_then(|id| self.get(id)).unwrap_or(&self.0[0])
    }
}

/// The build's decks: the words deck alone in 063a.
pub(crate) fn registry() -> Decks {
    Decks::new(vec![
        Entry {
            id: DeckId::words(),
            deck: deck(),
        },
        Entry {
            id: DeckId::devanagari(),
            deck: devanagari(),
        },
    ])
}

/// The terms of added words already pinned, one per distinct content.
static PINNED: Mutex<Vec<&'static Term>> = Mutex::new(Vec::new());

/// `term` (from [`learn_core::progress::State::term`]) as a `&'static`: the
/// deck's own, or an added word pinned for the life of the page.
///
/// A card and a list row hold `&'static Term` so they stay `Copy`; an added
/// word lives in the progress, which is not static. It is pinned once (the
/// same term gives the same reference): the page leaks one small term per
/// distinct word the learner added, never one per render.
///
/// The key is the term's content, not its id: a word a merge changed (the
/// same id, another sense or example) is pinned again and served as it is
/// now, and a word whose id the deck has but whose content is not the
/// deck's is not served as the deck's term.
pub(crate) fn pinned(term: &Term) -> &'static Term {
    if let Some(known) = deck().term(&term.id)
        && known == term
    {
        return known;
    }
    let mut pinned = PINNED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(known) = pinned.iter().find(|known| **known == term) {
        return known;
    }
    let leaked: &'static Term = Box::leak(Box::new(term.clone()));
    pinned.push(leaked);
    leaked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::second_deck;
    use learn_core::deck::TermId;

    fn zz() -> Entry {
        Entry {
            id: DeckId("zz".into()),
            deck: second_deck(),
        }
    }

    fn words_entry() -> Entry {
        Entry {
            id: DeckId::words(),
            deck: deck(),
        }
    }

    #[test]
    fn the_registry_holds_words_first_then_devanagari() {
        let decks = registry();
        assert_eq!(decks.len(), 2);
        assert!(std::ptr::eq(decks.resolve(None).deck, deck()));
        let ids: Vec<&str> = decks.iter().map(|entry| entry.id.0.as_str()).collect();
        assert_eq!(ids, ["words", "devanagari"]);
        assert!(std::ptr::eq(
            decks.get(&DeckId::devanagari()).unwrap().deck,
            devanagari()
        ));
    }

    #[test]
    fn the_embedded_devanagari_deck_is_a_script_deck_of_88_signs() {
        let deck = devanagari();
        assert_eq!(deck.kind, learn_core::kind::DeckKind::Script);
        assert_eq!(deck.terms.len(), 88);
        assert_eq!(deck.terms[0].lemma, "अ");
        assert_eq!(deck.terms[0].meaning.en, "a");
        assert!(deck.terms.iter().all(|term| term.example.is_none()));
        assert!(deck.terms.iter().all(|term| term.lemma == term.id.0));
    }

    #[test]
    #[should_panic(expected = "starts with the words deck")]
    fn a_registry_refuses_an_empty_list() {
        let _ = Decks::new(Vec::new());
    }

    #[test]
    #[should_panic(expected = "starts with the words deck")]
    fn a_registry_refuses_a_first_entry_that_is_not_words() {
        let _ = Decks::new(vec![zz(), words_entry()]);
    }

    #[test]
    fn a_registry_resolves_a_known_id_and_falls_back_to_words() {
        let decks = Decks::new(vec![words_entry(), zz()]);
        assert_eq!(decks.len(), 2);
        let zz_id = DeckId("zz".into());
        assert_eq!(decks.get(&zz_id).map(|entry| &entry.id), Some(&zz_id));
        assert_eq!(decks.resolve(Some(&zz_id)).id, zz_id);
        assert!(decks.resolve(Some(&DeckId("nope".into()))).id.is_words());
        assert!(decks.resolve(None).id.is_words());
        assert!(decks.get(&DeckId("nope".into())).is_none());
    }

    #[test]
    fn the_embedded_deck_parses_with_its_fifty_terms() {
        assert_eq!(deck().terms.len(), 50);
    }

    #[test]
    fn the_deck_is_parsed_once_and_shared() {
        assert!(std::ptr::eq(deck(), deck()));
        assert_eq!(deck().to_json(), deck().to_json());
    }

    #[test]
    fn a_deck_term_is_the_decks_own_and_an_added_word_is_pinned_once() {
        let kho = deck().term(&TermId("kho".into())).unwrap();
        assert!(std::ptr::eq(pinned(kho), kho));
        let word = crate::test_support::added_word();
        assert!(std::ptr::eq(pinned(&word), pinned(&word.clone())));
    }

    #[test]
    fn a_term_changed_by_a_merge_is_not_served_stale() {
        let word = crate::test_support::added_word();
        let before = pinned(&word);
        let mut changed = word.clone();
        changed.meaning.en = "faulty".into();
        let after = pinned(&changed);
        assert_eq!(before.meaning.en, "blameworthy");
        assert_eq!(after.meaning.en, "faulty");
        assert!(std::ptr::eq(after, pinned(&changed)), "and pinned once");
        // The first content is still served for the first term.
        assert_eq!(pinned(&word).meaning.en, "blameworthy");
    }

    #[test]
    fn a_term_shadowed_by_a_deck_update_is_not_served_as_the_decks() {
        // An added word whose id the deck now has, with other content than
        // the deck's: it is the word that was asked, not the deck term.
        let deck_term = deck().term(&TermId("kho".into())).unwrap();
        let mut shadowed = crate::test_support::added_word();
        shadowed.id = deck_term.id.clone();
        let served = pinned(&shadowed);
        assert!(!std::ptr::eq(served, deck_term));
        assert_eq!(served, &shadowed);
        assert!(std::ptr::eq(pinned(deck_term), deck_term));
    }
}
