//! The pure decisions behind the env badge's build-time values: `git`'s
//! raw output (branch, short sha, a dirty worktree) in, a displayable
//! value out, plus a worktree's `.git` *file* content resolved to a real
//! path. `build.rs` calls `git` and the filesystem, then feeds their raw
//! output here; this file is included from both `build.rs` (`#[path]`,
//! since a build script is its own crate, compiled before the library it
//! builds) and this crate's own module tree, so `cargo test -p app`
//! covers it — `build.rs` itself is not a test target. Copied from
//! `ui-lab`'s gallery (task 032's pattern); its tests travel with it.

use std::path::{Path, PathBuf};

/// What `git rev-parse --abbrev-ref HEAD` says, as the badge should show
/// it: git's own name for a detached `HEAD` reads as `"detached"`, not
/// the literal `"HEAD"`.
pub(crate) fn display_branch(raw: &str) -> String {
    if raw == "HEAD" {
        "detached".to_string()
    } else {
        raw.to_string()
    }
}

/// The badge's `env` and `sha`, from `git`'s raw output for each (`None`
/// when `git` failed or found no repository): both `"unknown"` unless
/// both succeeded — a sha without a branch (or the reverse) is not a
/// value worth trusting half of.
pub(crate) fn branch_and_sha(branch: Option<String>, sha: Option<String>) -> (String, String) {
    match (branch, sha) {
        (Some(branch), Some(sha)) => (display_branch(&branch), sha),
        _ => ("unknown".to_string(), "unknown".to_string()),
    }
}

/// Whether `git status --porcelain`'s raw stdout means the worktree had
/// uncommitted changes (tracked or not) at build time.
pub(crate) fn is_dirty_output(raw: &[u8]) -> bool {
    !raw.is_empty()
}

/// The badge's `env` and `sha` when the build is *told* where it comes from
/// (`LEARN_PALI_GIT_BRANCH`, `LEARN_PALI_GIT_COMMIT`: the image build has no
/// `.git`): the commit is cut to git's short length, the branch kept as
/// given (a tag build passes the tag). `None` unless both are present and
/// not blank, so the caller falls back to asking git — half a pair is not
/// trusted, as in [`branch_and_sha`].
pub(crate) fn badge_from_env(
    branch: Option<String>,
    commit: Option<String>,
) -> Option<(String, String)> {
    let branch = branch
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())?;
    let commit = commit
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())?;
    Some((branch, commit.chars().take(7).collect()))
}

/// `sha`, with a trailing `*` when `dirty`.
pub(crate) fn sha_with_dirty_marker(sha: &str, dirty: bool) -> String {
    if dirty {
        format!("{sha}*")
    } else {
        sha.to_string()
    }
}

/// The real git directory a worktree's `.git` *file*'s content points to
/// (`gitdir: <path>`), resolved against `dot_git_parent` when the path is
/// relative.
pub(crate) fn parse_gitdir_pointer(content: &str, dot_git_parent: &Path) -> PathBuf {
    let target = content
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .unwrap_or_default();
    let path = PathBuf::from(target);
    if path.is_absolute() {
        path
    } else {
        dot_git_parent.join(path)
    }
}

/// The git directory every worktree shares (refs, objects): `commondir`'s
/// content, resolved against `git_dir` when relative.
pub(crate) fn parse_commondir(content: &str, git_dir: &Path) -> PathBuf {
    let target = PathBuf::from(content.trim());
    if target.is_absolute() {
        target
    } else {
        git_dir.join(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_name_passes_through_display_branch_unchanged() {
        assert_eq!(display_branch("main"), "main");
        assert_eq!(display_branch("feat/env-badge"), "feat/env-badge");
    }

    #[test]
    fn git_s_own_head_name_for_a_detached_checkout_reads_as_detached() {
        assert_eq!(display_branch("HEAD"), "detached");
    }

    #[test]
    fn a_branch_and_a_sha_both_present_show_as_is() {
        assert_eq!(
            branch_and_sha(Some("main".to_string()), Some("a1b2c3f".to_string())),
            ("main".to_string(), "a1b2c3f".to_string())
        );
    }

    #[test]
    fn a_detached_head_shows_detached_not_head() {
        assert_eq!(
            branch_and_sha(Some("HEAD".to_string()), Some("a1b2c3f".to_string())),
            ("detached".to_string(), "a1b2c3f".to_string())
        );
    }

    #[test]
    fn no_git_falls_back_to_unknown_for_both() {
        assert_eq!(
            branch_and_sha(None, None),
            ("unknown".to_string(), "unknown".to_string())
        );
    }

    #[test]
    fn a_half_result_is_not_trusted_either() {
        assert_eq!(
            branch_and_sha(Some("main".to_string()), None),
            ("unknown".to_string(), "unknown".to_string())
        );
    }

    #[test]
    fn empty_status_output_is_clean() {
        assert!(!is_dirty_output(b""));
    }

    #[test]
    fn any_status_line_is_dirty() {
        assert!(is_dirty_output(b" M gallery/src/lib.rs\n"));
        assert!(is_dirty_output(b"?? new_file.rs\n"));
    }

    #[test]
    fn the_dirty_marker_is_a_trailing_star() {
        assert_eq!(sha_with_dirty_marker("a1b2c3f", true), "a1b2c3f*");
        assert_eq!(sha_with_dirty_marker("a1b2c3f", false), "a1b2c3f");
    }

    #[test]
    fn a_worktree_s_git_file_points_at_an_absolute_gitdir() {
        let resolved = parse_gitdir_pointer(
            "gitdir: /repo/.git/worktrees/mine\n",
            Path::new("/irrelevant"),
        );
        assert_eq!(resolved, PathBuf::from("/repo/.git/worktrees/mine"));
    }

    #[test]
    fn a_relative_gitdir_pointer_resolves_against_the_dot_git_s_parent() {
        let resolved = parse_gitdir_pointer("gitdir: ../.git/worktrees/mine\n", Path::new("/wt"));
        assert_eq!(resolved, PathBuf::from("/wt/../.git/worktrees/mine"));
    }

    #[test]
    fn an_absolute_commondir_is_kept_as_is() {
        let resolved = parse_commondir("/repo/.git\n", Path::new("/repo/.git/worktrees/mine"));
        assert_eq!(resolved, PathBuf::from("/repo/.git"));
    }

    #[test]
    fn a_relative_commondir_resolves_against_the_worktree_s_git_dir() {
        let resolved = parse_commondir("../..\n", Path::new("/repo/.git/worktrees/mine"));
        assert_eq!(resolved, PathBuf::from("/repo/.git/worktrees/mine/../.."));
    }

    #[test]
    fn a_build_given_branch_and_commit_win_and_the_commit_is_shortened() {
        assert_eq!(
            badge_from_env(
                Some("main".to_string()),
                Some("0123456789abcdef0123456789abcdef01234567".to_string())
            ),
            Some(("main".to_string(), "0123456".to_string()))
        );
    }

    #[test]
    fn a_tag_build_shows_the_tag_as_its_branch() {
        assert_eq!(
            badge_from_env(Some("v0.1.0".to_string()), Some("abc1234".to_string())),
            Some(("v0.1.0".to_string(), "abc1234".to_string()))
        );
    }

    #[test]
    fn one_missing_or_blank_value_falls_back_to_git() {
        assert_eq!(badge_from_env(None, None), None);
        assert_eq!(badge_from_env(Some("main".to_string()), None), None);
        assert_eq!(badge_from_env(None, Some("abc1234".to_string())), None);
        assert_eq!(
            badge_from_env(Some("  ".to_string()), Some("abc1234".to_string())),
            None
        );
        assert_eq!(
            badge_from_env(Some("main".to_string()), Some(String::new())),
            None
        );
    }
}
