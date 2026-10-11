//! A seeded shuffle: the order of a session's queue.
//!
//! Nothing in this crate reads a random source (as nothing reads a clock):
//! the caller draws a `seed` and the same seed always gives the same order.
//! The generator is `SplitMix64`, a few lines with no dependency, and the
//! shuffle is Fisher–Yates, so every order is possible. Any seed works, `0`
//! included (the generator adds a constant before it mixes).

/// A `SplitMix64` generator.
struct SplitMix64(u64);

impl SplitMix64 {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..bound` (`bound > 0`): the high bits of a 128-bit
    /// product, whose bias is below 2^-64 times `bound`.
    fn below(&mut self, bound: usize) -> usize {
        let product = u128::from(self.next()) * bound as u128;
        usize::try_from(product >> 64).expect("below `bound`, which is a usize")
    }
}

/// Puts `items` in a random order drawn from `seed` (Fisher–Yates): the
/// same seed gives the same order, and the items are only moved.
pub(crate) fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut rng = SplitMix64(seed);
    for end in (1..items.len()).rev() {
        items.swap(end, rng.below(end + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_order() {
        let mut a: Vec<u32> = (0..20).collect();
        let mut b = a.clone();
        shuffle(&mut a, 42);
        shuffle(&mut b, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn the_items_are_only_moved() {
        for seed in [0, 1, u64::MAX] {
            let mut items: Vec<u32> = (0..50).collect();
            shuffle(&mut items, seed);
            let mut sorted = items.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..50).collect::<Vec<u32>>(), "{seed}");
        }
    }

    #[test]
    fn different_seeds_give_different_orders_and_zero_is_a_good_seed() {
        let order = |seed| {
            let mut items: Vec<u32> = (0..10).collect();
            shuffle(&mut items, seed);
            items
        };
        assert_ne!(order(0), (0..10).collect::<Vec<u32>>());
        assert_ne!(order(0), order(1));
        assert_ne!(order(1), order(2));
    }

    #[test]
    fn nothing_or_one_item_is_left_alone() {
        let mut none: [u32; 0] = [];
        shuffle(&mut none, 5);
        let mut one = [9];
        shuffle(&mut one, 5);
        assert_eq!(one, [9]);
    }

    #[test]
    fn every_place_is_reached() {
        // 4 items, many seeds: each item lands at each place at least once.
        let mut seen = [[false; 4]; 4];
        for seed in 0..200 {
            let mut items = [0usize, 1, 2, 3];
            shuffle(&mut items, seed);
            for (place, item) in items.iter().enumerate() {
                seen[*item][place] = true;
            }
        }
        assert!(seen.iter().all(|row| row.iter().all(|reached| *reached)));
    }
}
