//! The write facet of a script deck answered by choice (071, from 063's
//! design): the romanisation is shown and four signs offered, the right
//! one and three distractors, and the app judges. [`distractors`] is pure:
//! the same group first (the deck's `pos` label: a vowel among vowels, a
//! guttural among gutturals), topped up by the nearest signs by rank when
//! the group is small (the niggahīta has no group mate); never the answer,
//! never a sign twice; the order of the four is fixed by `seed`, so a card
//! re-rendered keeps its row.

use crate::deck::{Deck, Term, TermId};

/// How many choices a card offers, the answer included.
pub const CHOICES: usize = 4;

/// The three signs offered beside `term`'s, as term ids, in the order the
/// card shows them once mixed with the answer by [`choices`]. Fewer than
/// three when the deck has fewer than four terms.
#[must_use]
pub fn distractors(term: &Term, deck: &Deck, seed: u64) -> Vec<TermId> {
    let mut picked: Vec<&Term> = Vec::new();
    let mut same: Vec<&Term> = deck
        .terms
        .iter()
        .filter(|t| t.id != term.id && t.pos == term.pos)
        .collect();
    shuffle(&mut same, seed);
    picked.extend(same.into_iter().take(CHOICES - 1));
    if picked.len() < CHOICES - 1 {
        let mut near: Vec<&Term> = deck
            .terms
            .iter()
            .filter(|t| t.id != term.id && !picked.iter().any(|p| p.id == t.id))
            .collect();
        near.sort_by_key(|t| (t.rank.abs_diff(term.rank), t.rank));
        picked.extend(near.into_iter().take(CHOICES - 1 - picked.len()));
    }
    picked.into_iter().map(|t| t.id.clone()).collect()
}

/// The four choices of a card: the answer and its [`distractors`], in an
/// order fixed by `seed` (the answer is not always first).
#[must_use]
pub fn choices(term: &Term, deck: &Deck, seed: u64) -> Vec<TermId> {
    let mut all = distractors(term, deck, seed);
    all.push(term.id.clone());
    shuffle(
        &mut all,
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1),
    );
    all
}

/// A seed for a card: the term and the day it is asked, so the row is the
/// same across re-renders and differs from one day to the next.
#[must_use]
pub fn seed_for(term: &Term, day: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in term.id.0.bytes().chain(day.to_le_bytes()) {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A Fisher–Yates shuffle on a small xorshift stream from `seed`: the same
/// seed, the same order, on every platform.
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for i in (1..items.len()).rev() {
        #[allow(clippy::cast_possible_truncation, reason = "a slot index fits usize")]
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devanagari() -> Deck {
        Deck::from_json(include_str!("../fixtures/devanagari.json")).unwrap()
    }

    fn term<'a>(deck: &'a Deck, sign: &str) -> &'a Term {
        deck.terms.iter().find(|t| t.id.0 == sign).unwrap()
    }

    #[test]
    fn three_signs_of_the_same_group_never_the_answer_never_twice() {
        let deck = devanagari();
        let ka = term(&deck, "क");
        let picked = distractors(ka, &deck, 7);
        assert_eq!(picked.len(), 3);
        assert!(!picked.contains(&ka.id));
        let mut unique = picked.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 3, "{picked:?}");
        for id in &picked {
            assert_eq!(term(&deck, &id.0).pos, ka.pos, "the same group: {id:?}");
        }
    }

    #[test]
    fn a_sign_with_no_group_mate_takes_the_nearest_by_rank() {
        let deck = devanagari();
        let am = term(&deck, "अं");
        let picked = distractors(am, &deck, 3);
        assert_eq!(picked.len(), 3);
        assert!(!picked.contains(&am.id));
        let ranks: Vec<u32> = picked.iter().map(|id| term(&deck, &id.0).rank).collect();
        for rank in ranks {
            assert!(
                rank.abs_diff(am.rank) <= 3,
                "near by rank: {rank} vs {}",
                am.rank
            );
        }
    }

    #[test]
    fn the_four_choices_hold_the_answer_once_in_an_order_fixed_by_the_seed() {
        let deck = devanagari();
        let ka = term(&deck, "क");
        let a = choices(ka, &deck, 11);
        let again = choices(ka, &deck, 11);
        let other = choices(ka, &deck, 12);
        assert_eq!(a.len(), CHOICES);
        assert_eq!(a.iter().filter(|id| **id == ka.id).count(), 1);
        assert_eq!(a, again, "the same seed, the same row");
        assert_ne!(a, other, "another seed, another row (or another set)");
        let seeds_with_answer_first = (0..40u64)
            .filter(|s| choices(ka, &deck, *s)[0] == ka.id)
            .count();
        assert!(
            seeds_with_answer_first < 40,
            "the answer is not always first"
        );
    }

    #[test]
    fn the_seed_follows_the_term_and_the_day() {
        let deck = devanagari();
        let ka = term(&deck, "क");
        let kha = term(&deck, "ख");
        assert_eq!(seed_for(ka, 20_000), seed_for(ka, 20_000));
        assert_ne!(seed_for(ka, 20_000), seed_for(ka, 20_001));
        assert_ne!(seed_for(ka, 20_000), seed_for(kha, 20_000));
    }

    /// Golden: the row of क on seed 1 (063: a golden test pins the function).
    #[test]
    fn golden_row_for_ka_on_seed_one() {
        let deck = devanagari();
        let row: Vec<String> = choices(term(&deck, "क"), &deck, 1)
            .into_iter()
            .map(|id| id.0)
            .collect();
        // A change here is a change of the function: every learner's rows
        // move with it (review 071).
        assert_eq!(row, ["ङ", "घ", "ख", "क"]);
    }

    #[test]
    fn a_deck_too_small_for_four_gives_fewer_distractors_never_the_answer() {
        let mut deck = devanagari();
        deck.terms.truncate(3);
        let first = deck.terms[0].clone();
        let picked = distractors(&first, &deck, 5);
        assert_eq!(picked.len(), 2);
        assert!(!picked.contains(&first.id));
        assert_ne!(picked[0], picked[1]);
        assert_eq!(
            choices(&first, &deck, 5).len(),
            3,
            "fewer than CHOICES: the caller decides"
        );
    }

    #[test]
    fn no_two_signs_of_a_group_share_a_romanisation() {
        let deck = devanagari();
        let mut seen = std::collections::BTreeSet::new();
        for term in &deck.terms {
            assert!(
                seen.insert((term.pos.0.clone(), term.meaning.en.clone())),
                "{} and another {} sign read {}",
                term.id.0,
                term.pos.0,
                term.meaning.en
            );
        }
    }
}
