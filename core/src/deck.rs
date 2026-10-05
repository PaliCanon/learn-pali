//! The deck: terms, their meanings, forms and example.
//!
//! A [`Deck`] is the JSON the builder writes and the app and the CLI read:
//! `{version, source, terms}`. A [`Term`] is one DPD headword; its
//! [`TermId`] is the DPD lemma key (`sati 1.1`) and is unique in a deck.
//! Loading ([`Deck::from_json`]) refuses what the rest of the crate relies
//! on not to meet: an unknown version, a term id twice, a deck with no term.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The deck's own key: the DPD lemma key (`sati 1.1`). Unique in a deck.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TermId(pub String);

impl fmt::Display for TermId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A part-of-speech code as the DPD writes it (`masc`, `fem`, `pr`, `ind`…).
/// Facets do not depend on it; it is shown on the Meaning prompt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pos(pub String);

/// What a term means. `en` is always there; `fr` and `vi` are omitted when
/// the source has none; `literal` is the DPD's literal meaning when given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meaning {
    /// The English sense.
    pub en: String,
    /// The French sense, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fr: Option<String>,
    /// The Vietnamese sense, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vi: Option<String>,
    /// The literal meaning, when the DPD gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
}

/// The root of a term: its key (`√gam`) and its meaning (`go`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    /// The DPD root key.
    pub key: String,
    /// The root's meaning.
    pub meaning: String,
}

/// An example sentence split around the word to recall: `before ___ after`.
/// `answer` is the text that was blanked, exactly as it stands in the
/// sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blank {
    /// The text before the blank.
    pub before: String,
    /// The word that was blanked.
    pub answer: String,
    /// The text after the blank.
    pub after: String,
}

/// An example of the term in a sentence. Its `blank` is what makes the
/// Context facet: an example without one is shown but never asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Example {
    /// The sentence, plain text (no markup).
    pub text: String,
    /// The text it is quoted from (`MN 1`).
    pub source: String,
    /// The sutta reference.
    pub sutta: String,
    /// The sentence with the word blanked, when the builder found it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blank: Option<Blank>,
}

/// One labelled inflected form: `fem acc pl` → `satiyo`, `satī`. `rare` is
/// a form the DPD greys: shown nowhere as a question, never asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Form {
    /// The grammatical label (`fem acc pl`).
    pub label: String,
    /// Every value of that label.
    pub values: Vec<String>,
    /// True when the label has no regular value (the DPD greys it).
    pub rare: bool,
}

/// One DPD headword with everything a card needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    /// The unique key (`sati 1.1`).
    pub id: TermId,
    /// The headword as shown (`sati`).
    pub lemma: String,
    /// The part of speech.
    pub pos: Pos,
    /// The senses.
    pub meaning: Meaning,
    /// How the word is built, when the DPD says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<String>,
    /// The root, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<Root>,
    /// An example, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<Example>,
    /// The inflected forms (empty for an indeclinable).
    #[serde(default)]
    pub forms: Vec<Form>,
    /// The order of entry: new terms are given by ascending rank.
    pub rank: u32,
}

/// Why a deck could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckError {
    /// The text is not a deck (the parser's message).
    Json(String),
    /// The `version` is not [`Deck::VERSION`].
    UnknownVersion(u32),
    /// The same [`TermId`] appears twice.
    DuplicateTerm(TermId),
    /// The deck has no term.
    Empty,
}

impl fmt::Display for DeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeckError::Json(message) => write!(f, "not a deck: {message}"),
            DeckError::UnknownVersion(version) => write!(
                f,
                "deck version {version} is not supported (this build reads version {})",
                Deck::VERSION
            ),
            DeckError::DuplicateTerm(id) => write!(f, "term {id} appears twice in the deck"),
            DeckError::Empty => f.write_str("the deck has no term"),
        }
    }
}

impl std::error::Error for DeckError {}

/// The deck file: a version, where the data comes from (the licence line),
/// and the terms in rank order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    /// The format version; [`Deck::VERSION`].
    pub version: u32,
    /// The data's origin and licence, shown to the learner.
    pub source: String,
    /// The terms; ids are unique, the list is never empty.
    pub terms: Vec<Term>,
}

impl Deck {
    /// The only format version this crate reads and writes.
    pub const VERSION: u32 = 1;

    /// Reads a deck, refusing an unknown version, a duplicate term id and an
    /// empty deck.
    ///
    /// # Errors
    /// [`DeckError::Json`], [`DeckError::UnknownVersion`],
    /// [`DeckError::DuplicateTerm`] or [`DeckError::Empty`].
    pub fn from_json(text: &str) -> Result<Deck, DeckError> {
        let json_error = |error: serde_json::Error| DeckError::Json(error.to_string());
        // The version is read first: a future version may have another shape.
        let raw: serde_json::Value = serde_json::from_str(text).map_err(json_error)?;
        if let Some(version) = raw.get("version").and_then(serde_json::Value::as_u64)
            && version != u64::from(Deck::VERSION)
        {
            return Err(DeckError::UnknownVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let deck: Deck = serde_json::from_value(raw).map_err(json_error)?;
        if deck.terms.is_empty() {
            return Err(DeckError::Empty);
        }
        let mut seen = std::collections::HashSet::new();
        for term in &deck.terms {
            if !seen.insert(&term.id) {
                return Err(DeckError::DuplicateTerm(term.id.clone()));
            }
        }
        Ok(deck)
    }

    /// The deck as pretty JSON, terms in their stored order.
    ///
    /// # Panics
    /// Never in practice: the types serialise without failing.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a deck always serialises")
    }

    /// The term with this id, if the deck has it.
    pub fn term(&self, id: &TermId) -> Option<&Term> {
        self.terms.iter().find(|term| &term.id == id)
    }
}

/// Builders of terms and decks shared by the tests of the crate.
#[cfg(test)]
pub(crate) mod samples {
    use super::{Blank, Deck, Example, Form, Meaning, Pos, Term, TermId};

    /// An indeclinable: Meaning and Recall only.
    pub(crate) fn indeclinable(id: &str, rank: u32) -> Term {
        Term {
            id: TermId(id.to_string()),
            lemma: id.split(' ').next().unwrap().to_string(),
            pos: Pos("ind".to_string()),
            meaning: Meaning {
                en: format!("sense of {id}"),
                fr: None,
                vi: None,
                literal: None,
            },
            construction: None,
            root: None,
            example: None,
            forms: vec![],
            rank,
        }
    }

    /// A regular form (`label`, one plain value).
    pub(crate) fn form(label: &str, value: &str) -> Form {
        Form {
            label: label.to_string(),
            values: vec![value.to_string()],
            rare: false,
        }
    }

    /// A rare form.
    pub(crate) fn rare_form(label: &str, value: &str) -> Form {
        Form {
            label: label.to_string(),
            values: vec![value.to_string()],
            rare: true,
        }
    }

    /// An example, blanked or not.
    pub(crate) fn example(blank: bool) -> Example {
        Example {
            text: "evaṃ me sutaṃ".to_string(),
            source: "MN 1".to_string(),
            sutta: "mn1".to_string(),
            blank: blank.then(|| Blank {
                before: "evaṃ ".to_string(),
                answer: "me".to_string(),
                after: " sutaṃ".to_string(),
            }),
        }
    }

    /// A noun with two regular forms and a blanked example: four facets.
    pub(crate) fn noun(id: &str, rank: u32) -> Term {
        Term {
            pos: Pos("fem".to_string()),
            construction: Some("√sar + ti".to_string()),
            forms: vec![form("fem nom sg", "sati"), form("fem acc pl", "satiyo")],
            example: Some(example(true)),
            ..indeclinable(id, rank)
        }
    }

    /// A deck of the given terms.
    pub(crate) fn deck(terms: Vec<Term>) -> Deck {
        Deck {
            version: Deck::VERSION,
            source: "test deck".to_string(),
            terms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::samples::{deck, indeclinable, noun};
    use super::*;

    fn two_terms() -> Deck {
        deck(vec![noun("sati 1.1", 1), indeclinable("kho", 2)])
    }

    #[test]
    fn json_round_trip_keeps_everything() {
        let original = two_terms();
        let again = Deck::from_json(&original.to_json()).unwrap();
        assert_eq!(again, original);
    }

    #[test]
    fn absent_options_are_omitted_and_optional_on_read() {
        let json = two_terms().to_json();
        let kho = json.split("\"kho\"").nth(1).unwrap();
        assert!(!kho.contains("\"fr\""), "{kho}");
        assert!(!kho.contains("\"example\""), "{kho}");
        let minimal = r#"{"version":1,"source":"s","terms":[{"id":"a","lemma":"a","pos":"ind","meaning":{"en":"x"},"rank":1}]}"#;
        let deck = Deck::from_json(minimal).unwrap();
        assert_eq!(deck.terms[0].forms.len(), 0);
        assert_eq!(deck.terms[0].example, None);
    }

    #[test]
    fn fixture_shaped_deck_round_trips() {
        let text = r#"{
          "version": 1,
          "source": "Digital Pāḷi Dictionary v0.4.20260926, CC BY-NC-SA 4.0",
          "terms": [{
            "id": "sati 1.1", "lemma": "sati", "pos": "fem",
            "meaning": {"en": "mindfulness", "literal": "remembering"},
            "construction": "√sar + ti",
            "root": {"key": "√sar", "meaning": "remember"},
            "example": {"text": "sati upaṭṭhitā", "source": "MN 10", "sutta": "mn10",
                        "blank": {"before": "", "answer": "sati", "after": " upaṭṭhitā"}},
            "forms": [{"label": "fem acc pl", "values": ["satiyo", "satī"], "rare": false},
                      {"label": "fem instr pl", "values": ["satibhi"], "rare": true}],
            "rank": 1
          }]
        }"#;
        let deck = Deck::from_json(text).unwrap();
        assert_eq!(deck.terms[0].forms[0].values, vec!["satiyo", "satī"]);
        assert_eq!(Deck::from_json(&deck.to_json()).unwrap(), deck);
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let mut other = two_terms();
        other.version = 2;
        assert_eq!(
            Deck::from_json(&other.to_json()),
            Err(DeckError::UnknownVersion(2))
        );
    }

    #[test]
    fn the_version_is_checked_before_the_shape() {
        assert_eq!(
            Deck::from_json(r#"{"version":2}"#),
            Err(DeckError::UnknownVersion(2))
        );
        assert_eq!(
            Deck::from_json(r#"{"version":2,"terms":"another shape"}"#),
            Err(DeckError::UnknownVersion(2))
        );
    }

    #[test]
    fn a_duplicate_term_id_is_refused() {
        let twice = deck(vec![noun("sati 1.1", 1), indeclinable("sati 1.1", 2)]);
        assert_eq!(
            Deck::from_json(&twice.to_json()),
            Err(DeckError::DuplicateTerm(TermId("sati 1.1".to_string())))
        );
    }

    #[test]
    fn an_empty_deck_is_refused() {
        assert_eq!(
            Deck::from_json(&deck(vec![]).to_json()),
            Err(DeckError::Empty)
        );
    }

    #[test]
    fn text_that_is_not_a_deck_is_a_json_error() {
        for text in ["", "{}", "[1]", "{\"version\":1}"] {
            assert!(
                matches!(Deck::from_json(text), Err(DeckError::Json(_))),
                "{text:?}"
            );
        }
    }

    #[test]
    fn each_error_says_what_is_wrong() {
        assert!(DeckError::UnknownVersion(7).to_string().contains('7'));
        let id = TermId("kho".to_string());
        assert!(
            DeckError::DuplicateTerm(id.clone())
                .to_string()
                .contains("kho")
        );
        assert_eq!(id.to_string(), "kho");
        assert_ne!(DeckError::Empty.to_string(), "");
        assert_ne!(DeckError::Json("boom".to_string()).to_string(), "");
    }

    #[test]
    fn term_lookup_by_id() {
        let deck = two_terms();
        let found = deck.term(&TermId("kho".to_string())).unwrap();
        assert_eq!(found.rank, 2);
        assert!(deck.term(&TermId("missing".to_string())).is_none());
    }
}
