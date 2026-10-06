//! The deck the site teaches: `core/fixtures/deck.json`, embedded in the
//! binary and the wasm with `include_str!` and parsed once, on first use
//! ([`deck`]). No fetch, so no network failure state in v1; the pipeline's
//! deck (task 004) replaces the file, and later the site serves it.

use std::sync::LazyLock;

use learn_core::deck::Deck;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_deck_parses_with_its_fifty_terms() {
        assert_eq!(deck().terms.len(), 50);
    }

    #[test]
    fn the_deck_is_parsed_once_and_shared() {
        assert!(std::ptr::eq(deck(), deck()));
        assert_eq!(deck().to_json(), deck().to_json());
    }
}
