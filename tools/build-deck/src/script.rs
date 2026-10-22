//! A script deck from a reviewed inventory table (063b: the Devanāgarī
//! alphabet for Pāli).
//!
//! The table is a TSV with a header `sign roman group count`; a line
//! starting with `#` is a comment. Each row becomes one [`Term`] of a
//! [`DeckKind::Script`] deck: the sign is both the id and the lemma, the
//! group is the part of speech (`vowel`, `velar`, … `conjunct`), the
//! romanisation is the meaning in every language, and the rank is the
//! row's order, so `new` takes the signs in the table's order. A sign is
//! Devanāgarī only (U+0900–U+097F); a row with another script, an empty
//! field or a sign seen twice refuses the whole table.

use learn_core::book::DeckId;
use learn_core::deck::{Deck, Meaning, Pos, Term, TermId};
use learn_core::kind::DeckKind;

/// One row of the inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The sign, in Devanāgarī.
    pub sign: String,
    /// Its romanisation (`kha`, `kkh`).
    pub roman: String,
    /// Its group, the deck's part of speech.
    pub group: String,
}

/// The rows of a TSV inventory, in order.
///
/// # Errors
/// A row with fewer than three fields, an empty sign, roman or group, a
/// sign outside the Devanagari block, or a sign already seen.
pub fn parse_inventory(text: &str) -> Result<Vec<Item>, String> {
    let mut items: Vec<Item> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') || line.starts_with("sign\t") {
            continue;
        }
        let item = parse_row(line).map_err(|e| format!("line {}: {e}", number + 1))?;
        if items.iter().any(|seen| seen.sign == item.sign) {
            return Err(format!(
                "line {}: sign {} seen twice",
                number + 1,
                item.sign
            ));
        }
        items.push(item);
    }
    if items.is_empty() {
        return Err("the inventory has no row".to_string());
    }
    Ok(items)
}

fn parse_row(line: &str) -> Result<Item, String> {
    let mut fields = line.split('\t').map(str::trim);
    let (sign, roman, group) = (
        fields.next().unwrap_or_default(),
        fields.next().unwrap_or_default(),
        fields.next().unwrap_or_default(),
    );
    if sign.is_empty() || roman.is_empty() || group.is_empty() {
        return Err("a row needs sign, roman and group".to_string());
    }
    if !sign.chars().all(is_devanagari) {
        return Err(format!("sign {sign} is not Devanāgarī"));
    }
    Ok(Item {
        sign: sign.to_string(),
        roman: roman.to_string(),
        group: group.to_string(),
    })
}

/// Whether `c` is in the Unicode Devanagari block.
fn is_devanagari(c: char) -> bool {
    ('\u{0900}'..='\u{097F}').contains(&c)
}

/// The script deck of `items`, in their order; `source` names the inventory.
#[must_use]
pub fn script_deck(items: &[Item], source: &str) -> Deck {
    let terms = items
        .iter()
        .zip(1u32..)
        .map(|(item, rank)| Term {
            id: TermId(item.sign.clone()),
            lemma: item.sign.clone(),
            pos: Pos(item.group.clone()),
            meaning: Meaning {
                en: item.roman.clone(),
                fr: Some(item.roman.clone()),
                vi: Some(item.roman.clone()),
                literal: None,
            },
            construction: None,
            root: None,
            example: None,
            forms: Vec::new(),
            rank,
        })
        .collect();
    Deck {
        version: Deck::VERSION,
        source: source.to_string(),
        terms,
        words: std::collections::BTreeMap::default(),
        kind: DeckKind::Script,
    }
}

/// The id the site registers the Devanāgarī deck under.
#[must_use]
pub fn devanagari_id() -> DeckId {
    DeckId::devanagari()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "# a comment\nsign\troman\tgroup\tcount\nअ\ta\tvowel\t\nक\tka\tvelar\t\nक्ख\tkkh\tconjunct\t2251\n";

    #[test]
    fn a_table_parses_in_order_skipping_comments_and_the_header() {
        let items = parse_inventory(TABLE).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].sign, "अ");
        assert_eq!(items[2].roman, "kkh");
        assert_eq!(items[2].group, "conjunct");
    }

    #[test]
    fn a_latin_sign_is_refused_with_its_line() {
        let err = parse_inventory("sign\troman\tgroup\nka\tka\tvelar\n").unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
        assert!(err.contains("not Devanāgarī"), "{err}");
    }

    #[test]
    fn a_short_row_an_empty_field_and_a_twice_seen_sign_are_refused() {
        assert!(parse_inventory("क\tka\n").unwrap_err().contains("needs"));
        assert!(
            parse_inventory("क\t\tvelar\n")
                .unwrap_err()
                .contains("needs")
        );
        let twice = parse_inventory("क\tka\tvelar\nक\tka\tvelar\n").unwrap_err();
        assert!(twice.contains("seen twice"), "{twice}");
        assert_eq!(
            parse_inventory("# nothing\n").unwrap_err(),
            "the inventory has no row"
        );
    }

    #[test]
    fn the_deck_is_a_script_deck_whose_terms_follow_the_table() {
        let deck = script_deck(&parse_inventory(TABLE).unwrap(), "test table");
        assert_eq!(deck.kind, DeckKind::Script);
        assert_eq!(deck.source, "test table");
        let ranks: Vec<u32> = deck.terms.iter().map(|t| t.rank).collect();
        assert_eq!(ranks, vec![1, 2, 3]);
        let kkh = &deck.terms[2];
        assert_eq!(kkh.id.0, "क्ख");
        assert_eq!(kkh.lemma, "क्ख");
        assert_eq!(kkh.pos.0, "conjunct");
        assert_eq!(kkh.meaning.en, "kkh");
        assert_eq!(kkh.meaning.fr.as_deref(), Some("kkh"));
        assert!(kkh.example.is_none());
        assert_eq!(kkh.forms.len(), 0);
    }

    #[test]
    fn the_deck_round_trips_through_json_as_a_script_deck() {
        let deck = script_deck(&parse_inventory(TABLE).unwrap(), "t");
        let text = deck.to_json();
        assert!(
            text.contains("\"kind\":\"script\"") || text.contains("\"kind\": \"script\""),
            "{text}"
        );
        assert_eq!(Deck::from_json(&text).unwrap(), deck);
    }

    #[test]
    fn the_committed_inventory_holds_the_reviewed_88_signs() {
        // 8 vowels, 32 consonants and the niggahīta (the 33 of the grammars),
        // 7 vowel signs on क, 40 conjuncts.
        let items = parse_inventory(include_str!("../devanagari.tsv")).unwrap();
        assert_eq!(items.len(), 88);
        let count = |g: &str| items.iter().filter(|i| i.group == g).count();
        assert_eq!(count("vowel"), 8);
        assert_eq!(
            [
                "velar",
                "palatal",
                "retroflex",
                "dental",
                "labial",
                "semivowel",
                "sibilant",
                "aspirate",
                "lateral"
            ]
            .iter()
            .map(|g| count(g))
            .sum::<usize>(),
            32
        );
        assert_eq!(count("niggahita"), 1);
        assert_eq!(count("vowel-sign"), 7);
        assert_eq!(count("conjunct"), 40);
        assert_eq!(devanagari_id().0, "devanagari");
    }
}
