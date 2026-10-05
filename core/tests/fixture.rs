//! The committed fixture deck: generated once from the DPD by
//! `build-deck` (see the README) and read by the tests and the CLI.
//! No test reads the DPD database itself.

use std::fs;
use std::path::PathBuf;

use learn_core::deck::Deck;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn deck() -> Deck {
    Deck::from_json(&fixture("deck.json")).expect("the committed deck loads")
}

/// The keys of `terms.txt`: one per line, `#` comments and blanks skipped.
fn keys() -> Vec<String> {
    fixture("terms.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_deck_holds_the_terms_of_the_list_in_order_with_ranks_one_to_fifty() {
    let keys = keys();
    assert_eq!(keys.len(), 50);
    let deck = deck();
    let ids: Vec<&str> = deck.terms.iter().map(|t| t.id.0.as_str()).collect();
    assert_eq!(ids, keys.iter().map(String::as_str).collect::<Vec<_>>());
    let ranks: Vec<u32> = deck.terms.iter().map(|t| t.rank).collect();
    assert_eq!(ranks, (1..=50).collect::<Vec<u32>>());
}

#[test]
fn every_term_has_an_english_meaning() {
    for term in &deck().terms {
        assert!(!term.meaning.en.trim().is_empty(), "{}", term.id);
    }
}

#[test]
fn the_deck_exercises_forms_and_context() {
    let deck = deck();
    assert!(
        deck.terms
            .iter()
            .any(|t| t.forms.iter().any(|form| !form.rare)),
        "no term has a regular form"
    );
    assert!(
        deck.terms
            .iter()
            .any(|t| t.example.as_ref().is_some_and(|e| e.blank.is_some())),
        "no term has a blank"
    );
}

#[test]
fn the_source_names_the_licence() {
    let deck = deck();
    assert!(deck.source.contains("CC BY-NC-SA 4.0"), "{}", deck.source);
    assert!(deck.source.contains("Digital Pāḷi Dictionary"));
}

#[test]
fn the_deck_file_is_what_the_loader_writes() {
    let deck = deck();
    assert_eq!(fixture("deck.json"), deck.to_json() + "\n");
}
