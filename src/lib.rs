//! learn-pali — learning Pāli, a PaliCanon service.
//!
//! The crate is a placeholder until the service's first task defines its
//! scope (`.task/`): it holds the repository's rules (`CLAUDE.md`,
//! `clippy.toml`, `deny.toml`) and its CI, nothing else yet.

/// The service's name, as its pages and logs will show it.
#[must_use]
pub const fn name() -> &'static str {
    "learn-pali"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_the_repository_s() {
        assert_eq!(name(), "learn-pali");
    }
}
