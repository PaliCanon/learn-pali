//! The meanings of an example's words, and the key they are filed under.
//!
//! The builder resolves every word of every example in the DPD and files
//! its entries in [`crate::deck::Deck::words`] under [`word_key`] of the
//! word as written; the app keys a pressed word the same way, so the two
//! always meet.

use serde::{Deserialize, Serialize};

/// One dictionary entry of a written form: `āmantesi` → `āmantesi`, `aor`,
/// `aor 3rd sg`, `addressed; said (to); advised`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordEntry {
    /// The DPD headword as the DPD writes it (`atha 1`).
    pub headword: String,
    /// The headword's part of speech (`ind`, `aor`, `masc`).
    pub pos: String,
    /// What the form is in that headword's paradigm (`aor 3rd sg`), when
    /// the DPD says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
    /// The headword's meaning, short (English).
    pub meaning: String,
}

/// The key of a written word: lower case, `ṁ` written `ṃ`, quotes and
/// apostrophes stripped at both ends, a typographic apostrophe inside
/// written `'` (`Sampādethā’ti` → `sampādethā'ti`).
pub fn word_key(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches(is_quote);
    let mut key = String::with_capacity(trimmed.len());
    for c in trimmed.chars().flat_map(char::to_lowercase) {
        match c {
            'ṁ' => key.push('ṃ'),
            '\u{307}' | '\u{323}' if key.ends_with('m') => {
                key.pop();
                key.push('ṃ');
            }
            '’' | '‘' => key.push('\''),
            other => key.push(other),
        }
    }
    key
}

/// A quote or an apostrophe, straight or typographic.
fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '"' | '‘' | '’' | '“' | '”')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_lower_case() {
        assert_eq!(word_key("Atha"), "atha");
        assert_eq!(word_key("ĀMANTESI"), "āmantesi");
    }

    #[test]
    fn the_niggahita_is_always_written_with_the_dot_below() {
        assert_eq!(word_key("evaṁ"), "evaṃ");
        assert_eq!(word_key("EVAṀ"), "evaṃ");
        assert_eq!(word_key("evaṃ"), "evaṃ");
        // m with a combining dot above or below
        assert_eq!(word_key("evam\u{307}"), "evaṃ");
        assert_eq!(word_key("evam\u{323}"), "evaṃ");
    }

    #[test]
    fn quotes_and_apostrophes_at_the_ends_are_stripped() {
        for raw in [
            "'atha",
            "atha'",
            "‘atha’",
            "“atha”",
            "\"atha\"",
            "’atha",
            " atha ",
        ] {
            assert_eq!(word_key(raw), "atha", "{raw:?}");
        }
        assert_eq!(word_key("''"), "");
    }

    #[test]
    fn a_sandhi_apostrophe_inside_is_kept_and_written_straight() {
        assert_eq!(word_key("sampādethā'ti"), "sampādethā'ti");
        assert_eq!(word_key("Sampādethā’ti"), "sampādethā'ti");
        assert_eq!(word_key("na'y'idaṁ"), "na'y'idaṃ");
        assert_eq!(word_key("‘bhikkhavo’ti’"), "bhikkhavo'ti");
    }

    #[test]
    fn an_entry_without_grammar_omits_it() {
        let entry = WordEntry {
            headword: "atha 1".to_string(),
            pos: "ind".to_string(),
            grammar: None,
            meaning: "then".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("grammar"), "{json}");
        let back: WordEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }
}
