//! The deck the site teaches: `core/fixtures/deck.json`, embedded in the
//! binary and the wasm with `include_str!` and parsed once, on first use
//! ([`deck`]). No fetch, so no network failure state in v1; the pipeline's
//! deck (task 004) replaces the file, and later the site serves it.

use std::sync::{LazyLock, Mutex};

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
    use learn_core::deck::TermId;

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
