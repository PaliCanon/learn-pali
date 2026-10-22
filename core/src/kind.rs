//! The kind of a deck, and the facet rule it owns.
//!
//! A [`Deck`](crate::deck::Deck) has a [`DeckKind`]: `words` (the DPD deck,
//! today's rule) or `script` (an alphabet: a sign and its romanisation).
//! The kind owns the facet rule ([`DeckKind::facets`]), so the same term
//! asks different questions in a different kind of deck. The
//! [`Facet`] enum does not grow and the merge is
//! unchanged. The file names the kind in lower case; a deck with no `kind`
//! is a words deck, and a kind this build does not know is refused.

use serde::{Deserialize, Serialize};

use crate::deck::Term;
use crate::facet::{Facet, FacetSet, words_facets};

/// What a deck is made of, and so which facets its items are asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeckKind {
    /// Pāli words: Meaning and Recall always, Forms and Context when the
    /// term has them (Forms and Context as `facet` describes them). The kind of a deck whose file
    /// has no `kind`.
    #[default]
    Words,
    /// A script: a sign and its romanisation. Always exactly two facets,
    /// Meaning (read: the sign is shown, the romanisation revealed, the
    /// learner judges) and Recall (write: the romanisation is shown, four
    /// signs are offered, the app judges), whatever the item carries.
    Script,
}

impl DeckKind {
    /// True for [`DeckKind::Words`]: the deck file then omits its `kind`.
    #[must_use]
    pub fn is_words(&self) -> bool {
        *self == DeckKind::Words
    }

    /// The facets `term` is asked in a deck of this kind.
    #[must_use]
    pub fn facets(self, term: &Term) -> FacetSet {
        match self {
            DeckKind::Words => words_facets(term),
            DeckKind::Script => [Facet::Meaning, Facet::Recall].into_iter().collect(),
        }
    }
}
