//! Merging two progress states: what the sync of task 050 joins.
//!
//! [`merge`] needs no clock and no review timestamp: it is a **join**. Per
//! term, the entry with the greater [`key`] `(lapses, stage, due)` wins
//! whole; on an equal key the two `passed` sets are unioned. A term only one
//! side started is kept as it is. `last_today` is the later of the two.
//!
//! Every answer keeps a term's key monotone: a fail adds a lapse; a climb
//! moves the stage up (at Year, the due day later); a pass that does not
//! climb keeps the key and grows `passed`. So the device that answered more
//! wins, and `merge` is commutative, associative and idempotent: a lost
//! write heals at the loser's next sync. Passes made under a lower key are
//! dropped on purpose (a fail or a climb clears `passed` on one device
//! too). One call lowers a key: [`Session::advance`](crate::session::Session::advance)
//! (study ahead) moves `due` back to today. Only the CLI calls it, and the
//! CLI does not sync; a synced host that called it would see the
//! advance undone (with its partial passes) by the next merge. `last_today` moves every device's day forward to the latest one
//! seen, as an import does.
//!
//! The order of the result is `b`'s terms, then `a`'s other terms in `a`'s
//! order: merging a state into itself, or into a merge it is already part
//! of, gives the very same state ([`State`] equality is ordered).

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::date::Date;
use crate::deck::TermId;
use crate::progress::{Progress, State};
use crate::stage::Stage;

/// The order of two entries of one term: `(lapses, stage, due)`, compared
/// in that order. The greater one is further along and wins a [`merge`].
pub fn key(progress: &Progress) -> (u32, Stage, Date) {
    (progress.lapses, progress.stage, progress.due)
}

/// The join of `a` and `b`; see the module documentation.
pub fn merge(a: &State, b: &State) -> State {
    let ours: HashMap<&TermId, &Progress> = a.progress.iter().map(|p| (&p.term, p)).collect();
    let theirs: HashMap<&TermId, &Progress> = b.progress.iter().map(|p| (&p.term, p)).collect();
    let mut progress: Vec<Progress> = b
        .progress
        .iter()
        .map(|entry| match ours.get(&entry.term) {
            Some(mine) => join(mine, entry),
            None => entry.clone(),
        })
        .collect();
    progress.extend(
        a.progress
            .iter()
            .filter(|entry| !theirs.contains_key(&entry.term))
            .cloned(),
    );
    State {
        version: State::VERSION,
        last_today: a.last_today.max(b.last_today),
        progress,
    }
}

/// The join of two entries of one term: the greater [`key`] wins whole; an
/// equal key unions `passed`.
fn join(a: &Progress, b: &Progress) -> Progress {
    match key(a).cmp(&key(b)) {
        Ordering::Greater => a.clone(),
        Ordering::Less => b.clone(),
        Ordering::Equal => Progress {
            passed: a.passed.union(&b.passed),
            ..a.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facet::Facet;

    fn day(days: u32) -> Date {
        Date::from_days(20_000 + days)
    }

    fn entry(term: &str, lapses: u32, stage: Stage, due: u32, passed: &[Facet]) -> Progress {
        Progress {
            term: TermId(term.to_string()),
            stage,
            passed: passed.iter().copied().collect(),
            due: day(due),
            lapses,
        }
    }

    fn state(last: Option<u32>, progress: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            last_today: last.map(day),
            progress,
        }
    }

    fn terms(state: &State) -> Vec<&str> {
        state.progress.iter().map(|p| p.term.0.as_str()).collect()
    }

    #[test]
    fn the_terms_are_the_union_of_both_sides() {
        let a = state(None, vec![entry("x", 0, Stage::Day, 0, &[])]);
        let b = state(None, vec![entry("y", 0, Stage::Day, 0, &[])]);
        let merged = merge(&a, &b);
        assert_eq!(terms(&merged), vec!["y", "x"]);
        assert_eq!(merged.get(&TermId("x".into())), a.progress.first());
        assert_eq!(merged.get(&TermId("y".into())), b.progress.first());
    }

    #[test]
    fn the_greater_key_wins_whole_lapses_first_then_stage_then_due() {
        let m = &[Facet::Meaning][..];
        let r = &[Facet::Recall][..];
        for (ours, theirs) in [
            // More lapses wins over a higher stage and a later due.
            (
                entry("x", 2, Stage::Day, 1, m),
                entry("x", 1, Stage::Year, 300, r),
            ),
            // Same lapses: the higher stage wins over a later due.
            (
                entry("x", 1, Stage::Month, 1, m),
                entry("x", 1, Stage::Week, 300, r),
            ),
            // Same lapses and stage: the later due wins.
            (
                entry("x", 1, Stage::Year, 400, m),
                entry("x", 1, Stage::Year, 30, r),
            ),
        ] {
            let a = state(None, vec![ours.clone()]);
            let b = state(None, vec![theirs.clone()]);
            assert_eq!(merge(&a, &b).progress, vec![ours.clone()], "{ours:?}");
            assert_eq!(merge(&b, &a).progress, vec![ours.clone()], "{ours:?}");
        }
    }

    #[test]
    fn an_equal_key_unions_what_was_passed() {
        let a = state(None, vec![entry("x", 1, Stage::Week, 9, &[Facet::Meaning])]);
        let b = state(
            None,
            vec![entry(
                "x",
                1,
                Stage::Week,
                9,
                &[Facet::Recall, Facet::Forms],
            )],
        );
        let expected = entry(
            "x",
            1,
            Stage::Week,
            9,
            &[Facet::Meaning, Facet::Recall, Facet::Forms],
        );
        assert_eq!(merge(&a, &b).progress, vec![expected.clone()]);
        assert_eq!(merge(&b, &a).progress, vec![expected]);
    }

    #[test]
    fn last_today_is_the_later_of_the_two() {
        for (a, b, expected) in [
            (None, None, None),
            (Some(3), None, Some(3)),
            (None, Some(4), Some(4)),
            (Some(3), Some(4), Some(4)),
            (Some(5), Some(4), Some(5)),
        ] {
            let merged = merge(&state(a, vec![]), &state(b, vec![]));
            assert_eq!(merged.last_today, expected.map(day), "{a:?} {b:?}");
            assert_eq!(merged.version, State::VERSION);
        }
    }

    #[test]
    fn the_order_is_bs_terms_then_as_others_in_as_order() {
        let e = |term| entry(term, 0, Stage::Day, 0, &[]);
        let a = state(None, vec![e("p"), e("q"), e("r"), e("s")]);
        let b = state(None, vec![e("r"), e("z"), e("p")]);
        assert_eq!(terms(&merge(&a, &b)), vec!["r", "z", "p", "q", "s"]);
    }

    #[test]
    fn merging_with_nothing_changes_nothing() {
        let a = state(
            Some(2),
            vec![
                entry("x", 1, Stage::Week, 9, &[Facet::Meaning]),
                entry("y", 0, Stage::Day, 1, &[]),
            ],
        );
        assert_eq!(merge(&a, &State::empty()), a);
        assert_eq!(merge(&State::empty(), &a), a);
        assert_eq!(merge(&a, &a), a);
    }

    // ---- Properties, over generated states ----

    /// A `SplitMix64` generator (the same as the shuffle's): the cases are
    /// the same at every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
    }

    /// A state over a pool of five terms, each started or not, in a random
    /// order, with values drawn from small ranges so that equal keys (and
    /// so the union of `passed`) come up often.
    fn generated(rng: &mut Rng) -> State {
        let mut pool: Vec<&str> = vec!["a", "b", "c", "d", "e"];
        for end in (1..pool.len()).rev() {
            let at = usize::try_from(rng.below(end as u64 + 1)).unwrap();
            pool.swap(end, at);
        }
        let mut progress = Vec::new();
        for term in pool {
            if rng.below(3) == 0 {
                continue;
            }
            let stage =
                [Stage::Day, Stage::Week, Stage::Year][usize::try_from(rng.below(3)).unwrap()];
            let mut passed = Vec::new();
            for facet in Facet::ALL {
                if rng.below(2) == 0 {
                    passed.push(facet);
                }
            }
            let lapses = u32::try_from(rng.below(2)).unwrap();
            let due = u32::try_from(rng.below(2)).unwrap();
            progress.push(entry(term, lapses, stage, due, &passed));
        }
        let last = match rng.below(3) {
            0 => None,
            n => Some(u32::try_from(n).unwrap()),
        };
        state(last, progress)
    }

    /// The state with its terms in a fixed order: equality as a map.
    fn canonical(mut state: State) -> State {
        state.progress.sort_by(|x, y| x.term.cmp(&y.term));
        state
    }

    /// `merged` holds every term of `side`, never behind it: a greater
    /// key, or the same key with at least what it passed.
    fn covers(merged: &State, side: &State) -> bool {
        side.progress.iter().all(|ours| {
            merged.get(&ours.term).is_some_and(|joined| {
                key(joined) > key(ours)
                    || (key(joined) == key(ours) && joined.passed.is_superset(&ours.passed))
            })
        })
    }

    const CASES: u64 = 3000;

    #[test]
    fn merge_is_commutative() {
        let mut rng = Rng(1);
        for _ in 0..CASES {
            let (a, b) = (generated(&mut rng), generated(&mut rng));
            assert_eq!(
                canonical(merge(&a, &b)),
                canonical(merge(&b, &a)),
                "{a:?} {b:?}"
            );
        }
    }

    #[test]
    fn merge_is_associative() {
        let mut rng = Rng(2);
        for _ in 0..CASES {
            let (a, b, c) = (
                generated(&mut rng),
                generated(&mut rng),
                generated(&mut rng),
            );
            assert_eq!(
                canonical(merge(&merge(&a, &b), &c)),
                canonical(merge(&a, &merge(&b, &c))),
                "{a:?} {b:?} {c:?}"
            );
        }
    }

    /// Idempotent, exactly (order included): a state merged with itself,
    /// or merged again with a side already in it, is the same state.
    #[test]
    fn merge_is_idempotent() {
        let mut rng = Rng(3);
        for _ in 0..CASES {
            let (a, b) = (generated(&mut rng), generated(&mut rng));
            assert_eq!(merge(&a, &a), a);
            let once = merge(&a, &b);
            assert_eq!(merge(&once, &b), once, "{a:?} {b:?}");
        }
    }

    /// The join is an upper bound: nothing either side reached is lost,
    /// and nothing appears that neither side holds.
    #[test]
    fn merge_covers_both_sides_and_adds_nothing() {
        let mut rng = Rng(4);
        for _ in 0..CASES {
            let (a, b) = (generated(&mut rng), generated(&mut rng));
            let merged = merge(&a, &b);
            assert!(covers(&merged, &a) && covers(&merged, &b), "{a:?} {b:?}");
            assert!(
                merged
                    .progress
                    .iter()
                    .all(|p| a.get(&p.term).is_some() || b.get(&p.term).is_some())
            );
            assert_eq!(merged.last_today, a.last_today.max(b.last_today));
            assert!(State::from_json(&merged.to_json()).is_ok(), "no term twice");
        }
    }
}
