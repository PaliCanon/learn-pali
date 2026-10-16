//! Facets: the aspects of a term that are asked.
//!
//! A term is learned through facets: **Meaning** (pāli to sense) and
//! **Recall** (sense to pāli) always; **Forms** (a labelled inflected form)
//! when the term has at least one regular form; **Context** (the example
//! with the word blanked) when the example has a blank. [`facets_of`] is that
//! rule. A [`FacetSet`] is a small set of facets, written in JSON as a list
//! of names (`["Meaning","Forms"]`).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::deck::Term;

/// One aspect of a term, serialised as its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Facet {
    /// Pāli to sense.
    Meaning,
    /// Sense to pāli.
    Recall,
    /// A labelled inflected form.
    Forms,
    /// The example sentence with the word blanked.
    Context,
}

impl Facet {
    /// Every facet, in the order they are asked.
    pub const ALL: [Facet; 4] = [Facet::Meaning, Facet::Recall, Facet::Forms, Facet::Context];
}

/// A set of facets, iterated in [`Facet::ALL`] order whatever the insertion
/// order, serialised as a list of names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FacetSet(u8);

impl FacetSet {
    /// The empty set.
    pub fn empty() -> FacetSet {
        FacetSet(0)
    }

    fn bit(facet: Facet) -> u8 {
        1 << (facet as u8)
    }

    /// Adds a facet (adding it twice changes nothing).
    pub fn insert(&mut self, facet: Facet) {
        self.0 |= FacetSet::bit(facet);
    }

    /// True when the facet is in the set.
    pub fn contains(&self, facet: Facet) -> bool {
        self.0 & FacetSet::bit(facet) != 0
    }

    /// True when every facet of `other` is in `self` (the empty set is a
    /// subset of every set, and every set a superset of itself).
    pub fn is_superset(&self, other: &FacetSet) -> bool {
        other.0 & !self.0 == 0
    }

    /// The facets in both sets.
    #[must_use]
    pub fn intersection(&self, other: &FacetSet) -> FacetSet {
        FacetSet(self.0 & other.0)
    }

    /// The facets in either set.
    #[must_use]
    pub fn union(&self, other: &FacetSet) -> FacetSet {
        FacetSet(self.0 | other.0)
    }

    /// The facets of `self` that are not in `other`.
    #[must_use]
    pub fn difference(&self, other: &FacetSet) -> FacetSet {
        FacetSet(self.0 & !other.0)
    }

    /// How many facets are in the set.
    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }

    /// True when the set has no facet.
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The facets, in [`Facet::ALL`] order.
    pub fn iter(&self) -> impl Iterator<Item = Facet> + '_ {
        Facet::ALL.into_iter().filter(|facet| self.contains(*facet))
    }
}

impl FromIterator<Facet> for FacetSet {
    fn from_iter<I: IntoIterator<Item = Facet>>(facets: I) -> FacetSet {
        let mut set = FacetSet::empty();
        for facet in facets {
            set.insert(facet);
        }
        set
    }
}

impl Serialize for FacetSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for FacetSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FacetSet, D::Error> {
        Ok(Vec::<Facet>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

/// The facets a term can be asked: Meaning and Recall always; Forms when it
/// has a form that is not rare; Context when its example has a blank.
pub fn facets_of(term: &Term) -> FacetSet {
    let mut facets: FacetSet = [Facet::Meaning, Facet::Recall].into_iter().collect();
    if term.forms.iter().any(|form| !form.rare) {
        facets.insert(Facet::Forms);
    }
    if term
        .example
        .as_ref()
        .is_some_and(|example| example.blank.is_some())
    {
        facets.insert(Facet::Context);
    }
    facets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::samples::{example, form, indeclinable, noun, rare_form};

    fn set(facets: &[Facet]) -> FacetSet {
        facets.iter().copied().collect()
    }

    #[test]
    fn an_indeclinable_has_two_facets() {
        let facets = facets_of(&indeclinable("kho", 1));
        assert_eq!(facets.len(), 2);
        assert_eq!(facets, set(&[Facet::Meaning, Facet::Recall]));
    }

    #[test]
    fn a_noun_with_regular_forms_and_a_blank_has_four() {
        let facets = facets_of(&noun("sati 1.1", 1));
        assert_eq!(facets.len(), 4);
        assert_eq!(facets, set(&Facet::ALL));
    }

    /// (has a regular form, has a rare form, example: none / no blank /
    /// blank) and the facets that combination gives.
    type Case = (bool, bool, Option<bool>, Vec<Facet>);

    fn presence_cases() -> Vec<Case> {
        let (m, r, f, c) = (Facet::Meaning, Facet::Recall, Facet::Forms, Facet::Context);
        vec![
            (false, false, None, vec![m, r]),
            (true, false, None, vec![m, r, f]),
            (false, true, None, vec![m, r]),
            (true, true, None, vec![m, r, f]),
            (false, false, Some(false), vec![m, r]),
            (true, false, Some(false), vec![m, r, f]),
            (false, true, Some(false), vec![m, r]),
            (false, false, Some(true), vec![m, r, c]),
            (true, false, Some(true), vec![m, r, f, c]),
            (false, true, Some(true), vec![m, r, c]),
            (true, true, Some(true), vec![m, r, f, c]),
        ]
    }

    #[test]
    fn every_presence_combination_of_forms_and_blank() {
        for (regular, rare, blank, expected) in presence_cases() {
            let mut term = indeclinable("x", 1);
            if regular {
                term.forms.push(form("nom sg", "x"));
            }
            if rare {
                term.forms.push(rare_form("voc sg", "xa"));
            }
            term.example = blank.map(example);
            assert_eq!(
                facets_of(&term),
                set(&expected),
                "{regular} {rare} {blank:?}"
            );
        }
    }

    #[test]
    fn forms_all_rare_means_no_forms_facet() {
        let mut term = indeclinable("x", 1);
        term.forms = vec![rare_form("nom sg", "x"), rare_form("acc sg", "xaṃ")];
        assert!(!facets_of(&term).contains(Facet::Forms));
    }

    #[test]
    fn an_example_without_a_blank_has_no_context() {
        let mut term = noun("x", 1);
        term.example = Some(example(false));
        let facets = facets_of(&term);
        assert!(!facets.contains(Facet::Context));
        assert!(facets.contains(Facet::Forms));
    }

    #[test]
    fn set_operations() {
        let mut facets = FacetSet::empty();
        assert!(facets.is_empty());
        facets.insert(Facet::Forms);
        facets.insert(Facet::Forms);
        assert_eq!(facets.len(), 1);
        assert!(facets.contains(Facet::Forms) && !facets.contains(Facet::Meaning));

        let all = set(&Facet::ALL);
        assert!(all.is_superset(&facets) && !facets.is_superset(&all));
        assert!(facets.is_superset(&facets) && facets.is_superset(&FacetSet::empty()));
        let some = set(&[Facet::Meaning, Facet::Forms]);
        assert_eq!(all.intersection(&some), some);
        assert_eq!(some.intersection(&set(&[Facet::Recall])), FacetSet::empty());
        assert_eq!(all.difference(&some), set(&[Facet::Recall, Facet::Context]));
        assert_eq!(some.difference(&all), FacetSet::empty());
        let recall = set(&[Facet::Recall]);
        assert_eq!(
            some.union(&recall),
            set(&[Facet::Meaning, Facet::Recall, Facet::Forms])
        );
        assert_eq!(recall.union(&some), some.union(&recall));
        assert_eq!(some.union(&some), some);
        assert_eq!(some.union(&FacetSet::empty()), some);
    }

    #[test]
    fn iteration_is_in_asking_order_whatever_the_insertion_order() {
        let facets = set(&[Facet::Context, Facet::Meaning, Facet::Forms]);
        assert_eq!(
            facets.iter().collect::<Vec<_>>(),
            vec![Facet::Meaning, Facet::Forms, Facet::Context]
        );
    }

    #[test]
    fn serde_is_a_list_of_names() {
        let facets = set(&[Facet::Forms, Facet::Meaning]);
        let json = serde_json::to_string(&facets).unwrap();
        assert_eq!(json, r#"["Meaning","Forms"]"#);
        assert_eq!(serde_json::from_str::<FacetSet>(&json).unwrap(), facets);
        assert_eq!(serde_json::to_string(&FacetSet::empty()).unwrap(), "[]");
        assert_eq!(
            serde_json::from_str::<FacetSet>(r#"["Context","Context"]"#).unwrap(),
            set(&[Facet::Context])
        );
        assert!(serde_json::from_str::<FacetSet>(r#"["Sound"]"#).is_err());
    }
}
