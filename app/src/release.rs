//! Which deploy is running: the release the footer's badge shows (task 060).
//!
//! The badge is a property of the deploy revision, not of the build. The
//! deploy writes [`ENV_VAR`] (the tag name `v0.1.8`, `main`, or the
//! dispatched branch) and [`SHA_VAR`] (empty for a tag, else the short
//! commit) as environment variables of the Cloud Run revision. The server
//! reads them once at start ([`ReleaseInfo::server`]), writes them in the
//! document's `<head>` as two `<meta>` tags ([`ENV_META`], [`SHA_META`])
//! and renders the badge from the same value; the browser reads the two
//! metas before it mounts (`ReleaseInfo::from_document`), so the client
//! never recomputes anything and cannot disagree with the server.

/// The environment variable the deploy sets to the release's name: a tag
/// (`v0.1.8`), `main`, or a branch.
pub const ENV_VAR: &str = "LEARN_PALI_ENV";
/// The environment variable the deploy sets to the short commit: empty for
/// a tag, else seven characters.
pub const SHA_VAR: &str = "LEARN_PALI_SHA";
/// The `<meta name>` the document carries the release's name in.
pub const ENV_META: &str = "learn-release-env";
/// The `<meta name>` the document carries the short commit in (always
/// written, `content=""` for a tag).
pub const SHA_META: &str = "learn-release-sha";

/// Which deploy this is, as the footer's badge shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// The release's name: a tag, `main`, a branch, `dev` for a local run,
    /// `unknown` when the client found no metas.
    pub env: String,
    /// The short commit; empty for a tag, a local run and `unknown`.
    pub sha: String,
}

impl ReleaseInfo {
    /// The one wording of a release, shared by both ends of the seam so
    /// they cannot drift: the env trimmed (blank: `None`), the sha trimmed
    /// and cut to 7 characters.
    fn normalised(env: &str, sha: &str) -> Option<Self> {
        let env = env.trim();
        if env.is_empty() {
            return None;
        }
        Some(Self {
            env: env.to_owned(),
            sha: sha.trim().chars().take(7).collect(),
        })
    }

    /// Server side, from the two variables' values. A blank or absent env
    /// is a local run: env `dev`, no sha. Otherwise the env is trimmed and
    /// the sha, absent or blank as `""`, trimmed and cut to 7 characters.
    pub fn from_vars(env: Option<String>, sha: Option<String>) -> Self {
        Self::normalised(&env.unwrap_or_default(), &sha.unwrap_or_default()).unwrap_or_else(|| {
            Self {
                env: "dev".to_owned(),
                sha: String::new(),
            }
        })
    }

    /// Client side, from the metas' contents, worded exactly like the
    /// server ([`ReleaseInfo::from_vars`]): the env trimmed, the sha trimmed
    /// and cut to 7 characters (the sha may be `""`). Anything else, a meta
    /// missing or a blank env: [`ReleaseInfo::unknown`].
    pub fn from_metas(env: Option<String>, sha: Option<String>) -> Self {
        match (env, sha) {
            (Some(env), Some(sha)) => Self::normalised(&env, &sha).unwrap_or_else(Self::unknown),
            _ => Self::unknown(),
        }
    }

    /// What a client shows when the document carries no release: env
    /// `unknown`, no sha. Never `dev`, which says a local run.
    pub fn unknown() -> Self {
        Self {
            env: "unknown".to_owned(),
            sha: String::new(),
        }
    }

    /// The line the server prints at start, e.g. `release: env=v0.1.8
    /// sha=-` (`-` for no sha).
    pub fn log_line(&self) -> String {
        let sha = if self.sha.is_empty() { "-" } else { &self.sha };
        format!("release: env={} sha={sha}", self.env)
    }

    /// The release of this process, read once from [`ENV_VAR`] and
    /// [`SHA_VAR`] (the deploy sets both on the revision): what the
    /// document's metas and the footer's badge are written from.
    #[cfg(feature = "ssr")]
    pub fn server() -> &'static ReleaseInfo {
        static SERVER: std::sync::OnceLock<ReleaseInfo> = std::sync::OnceLock::new();
        SERVER.get_or_init(|| {
            Self::from_vars(std::env::var(ENV_VAR).ok(), std::env::var(SHA_VAR).ok())
        })
    }

    /// The browser's reader: the contents of the two `<meta>` tags the
    /// server wrote ([`ENV_META`], [`SHA_META`]), through
    /// [`ReleaseInfo::from_metas`]. Called before the app mounts.
    #[cfg(feature = "browser")]
    pub fn from_document() -> Self {
        let content = |name: &str| {
            leptos::prelude::document()
                .query_selector(&format!(r#"meta[name="{name}"]"#))
                .ok()
                .flatten()
                .and_then(|meta| meta.get_attribute("content"))
        };
        Self::from_metas(content(ENV_META), content(SHA_META))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unnecessary_wraps)]
    fn some(value: &str) -> Option<String> {
        Some(value.to_owned())
    }

    fn info(env: &str, sha: &str) -> ReleaseInfo {
        ReleaseInfo {
            env: env.to_owned(),
            sha: sha.to_owned(),
        }
    }

    #[test]
    fn a_tag_has_its_name_and_no_sha() {
        assert_eq!(
            ReleaseInfo::from_vars(some("v0.1.8"), None),
            info("v0.1.8", "")
        );
    }

    #[test]
    fn main_has_its_name_and_a_sha_cut_to_seven_characters() {
        assert_eq!(
            ReleaseInfo::from_vars(some("main"), some("0123456789abcdef")),
            info("main", "0123456")
        );
        assert_eq!(
            ReleaseInfo::from_vars(some("main"), some("34c489c")),
            info("main", "34c489c")
        );
    }

    #[test]
    fn a_blank_env_is_a_local_run_and_drops_the_sha() {
        assert_eq!(
            ReleaseInfo::from_vars(some("  "), some("34c489c")),
            info("dev", "")
        );
        assert_eq!(
            ReleaseInfo::from_vars(some(""), some("34c489c")),
            info("dev", "")
        );
    }

    #[test]
    fn an_absent_env_is_a_local_run() {
        assert_eq!(ReleaseInfo::from_vars(None, None), info("dev", ""));
        assert_eq!(
            ReleaseInfo::from_vars(None, some("34c489c")),
            info("dev", "")
        );
    }

    #[test]
    fn a_tag_with_a_blank_sha_has_none() {
        assert_eq!(
            ReleaseInfo::from_vars(some("v0.1.8"), some("   ")),
            info("v0.1.8", "")
        );
        assert_eq!(
            ReleaseInfo::from_vars(some("v0.1.8"), some("")),
            info("v0.1.8", "")
        );
    }

    #[test]
    fn the_values_are_trimmed() {
        assert_eq!(
            ReleaseInfo::from_vars(some(" feat/x \n"), some("  34c489c \n")),
            info("feat/x", "34c489c")
        );
        // Trimmed before it is cut: the spaces do not eat the seven.
        assert_eq!(
            ReleaseInfo::from_vars(some("main"), some("  0123456789")),
            info("main", "0123456")
        );
    }

    #[test]
    fn the_metas_are_taken_as_given() {
        assert_eq!(
            ReleaseInfo::from_metas(some("main"), some("34c489c")),
            info("main", "34c489c")
        );
        assert_eq!(
            ReleaseInfo::from_metas(some("feat/x"), some("34c489c")),
            info("feat/x", "34c489c")
        );
    }

    #[test]
    fn the_metas_are_trimmed_like_the_variables() {
        assert_eq!(
            ReleaseInfo::from_metas(some(" main "), some("34c489c")),
            info("main", "34c489c")
        );
        assert_eq!(
            ReleaseInfo::from_metas(some("v0.1.8"), some("  ")),
            info("v0.1.8", "")
        );
        assert_eq!(
            ReleaseInfo::from_metas(some(" feat/x\n"), some(" 34c489c ")),
            info("feat/x", "34c489c")
        );
    }

    #[test]
    fn a_long_sha_meta_is_cut_to_seven_characters() {
        assert_eq!(
            ReleaseInfo::from_metas(some("main"), some("34c489c0123456789")),
            info("main", "34c489c")
        );
    }

    /// Holds for non-blank envs only: a blank env differs on purpose (the
    /// client says `unknown`, the server `dev`), see the two tests below.
    #[test]
    fn from_metas_and_from_vars_agree_for_every_non_blank_env() {
        let envs = ["main", " main ", "v0.1.8", " feat/x\n", "feat/x"];
        let shas = [
            "",
            "  ",
            "34c489c",
            " 34c489c ",
            "34c489c0123456789",
            "  0123456789",
        ];
        for env in envs {
            for sha in shas {
                assert_eq!(
                    ReleaseInfo::from_metas(some(env), some(sha)),
                    ReleaseInfo::from_vars(some(env), some(sha)),
                    "env {env:?} sha {sha:?}"
                );
            }
        }
    }

    #[test]
    fn a_tag_meta_has_an_empty_sha() {
        assert_eq!(
            ReleaseInfo::from_metas(some("v0.1.8"), some("")),
            info("v0.1.8", "")
        );
    }

    #[test]
    fn a_missing_meta_is_unknown() {
        assert_eq!(ReleaseInfo::from_metas(None, None), ReleaseInfo::unknown());
        assert_eq!(
            ReleaseInfo::from_metas(None, some("34c489c")),
            ReleaseInfo::unknown()
        );
        assert_eq!(
            ReleaseInfo::from_metas(some("main"), None),
            ReleaseInfo::unknown()
        );
    }

    /// A blank env differs on purpose: a client without a release is a
    /// fault (`unknown`), a server without one is a local run (`dev`).
    #[test]
    fn a_whitespace_only_env_is_unknown_for_the_client() {
        assert_eq!(
            ReleaseInfo::from_metas(some("   "), some("34c489c")),
            ReleaseInfo::unknown()
        );
    }

    /// The server side of the same pair: see the client test above.
    #[test]
    fn a_whitespace_only_env_is_dev_for_the_server() {
        assert_eq!(
            ReleaseInfo::from_vars(some("   "), some("34c489c")),
            info("dev", "")
        );
    }

    #[test]
    fn a_blank_env_meta_is_unknown() {
        assert_eq!(
            ReleaseInfo::from_metas(some(""), some("34c489c")),
            ReleaseInfo::unknown()
        );
        assert_eq!(
            ReleaseInfo::from_metas(some("  "), some("")),
            ReleaseInfo::unknown()
        );
    }

    #[test]
    fn unknown_is_not_dev() {
        assert_eq!(ReleaseInfo::unknown(), info("unknown", ""));
        assert_ne!(ReleaseInfo::unknown(), ReleaseInfo::from_vars(None, None));
    }

    #[test]
    fn the_log_line_names_the_env_and_the_sha() {
        assert_eq!(info("v0.1.8", "").log_line(), "release: env=v0.1.8 sha=-");
        assert_eq!(
            info("main", "34c489c").log_line(),
            "release: env=main sha=34c489c"
        );
    }
}
