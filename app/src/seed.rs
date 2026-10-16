//! The seed of a session's random order: drawn in the browser each time a
//! session starts ([`browser_seed`]), so every session comes in a new
//! order. `learn_core` takes the seed as an input (it draws nothing), and
//! the server has no session to start, so it gets a fixed seed.

/// A random `u64` from the browser (`Math.random()` twice, 32 bits each);
/// on the server, the fixed seed `0` (never called there).
pub(crate) fn browser_seed() -> u64 {
    #[cfg(feature = "browser")]
    {
        (random_u32() << 32) | random_u32()
    }
    #[cfg(not(feature = "browser"))]
    {
        0
    }
}

/// 32 random bits: `Math.random()` is in `[0, 1)`, so scaled by 2^32 it is
/// below 2^32.
#[cfg(feature = "browser")]
fn random_u32() -> u64 {
    bits_of(js_sys::Math::random())
}

/// The 32 bits a unit-interval draw stands for: `unit` in `[0, 1)` scaled
/// by 2^32 and floored; a value outside the interval is clamped into it.
#[cfg_attr(
    not(any(feature = "browser", test)),
    expect(
        dead_code,
        reason = "read by `random_u32` in the browser builds (and the tests)"
    )
)]
fn bits_of(unit: f64) -> u64 {
    let scaled = (unit * 4_294_967_296.0).floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped into 0..=u32::MAX just above"
    )]
    {
        scaled.clamp(0.0, 4_294_967_295.0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_interval_maps_onto_32_bits() {
        assert_eq!(bits_of(0.0), 0);
        assert_eq!(bits_of(0.5), 1 << 31);
        assert_eq!(bits_of(0.999_999_999_9), u64::from(u32::MAX));
    }

    #[test]
    fn a_draw_outside_the_interval_is_clamped_not_wrapped() {
        assert_eq!(bits_of(-1.0), 0);
        assert_eq!(bits_of(1.0), u64::from(u32::MAX));
        assert_eq!(bits_of(f64::NAN), 0);
    }

    #[test]
    fn the_server_seed_is_fixed() {
        // The browser's draw is `Math.random()`, which no native test runs;
        // `bits_of` above is what is tested of it.
        #[cfg(not(feature = "browser"))]
        assert_eq!(browser_seed(), 0);
    }
}
