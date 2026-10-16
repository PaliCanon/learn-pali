//! This service's build script, two jobs, one function each (copied from
//! `ui-lab`'s gallery `build.rs`, task 032's pattern).
//!
//! **The string catalogue** ([`generate_catalogue`]): generates this
//! service's typed catalogue (`leptos_i18n`) from `locales/<code>.json`,
//! one file per [`palicanon_ui_lib::Language`] code. A key missing from
//! (or surplus in) any locale fails the build: `leptos_i18n` only warns
//! and falls back to English, which would ship a mixed-language page.
//! So does a `{{ name }}` placeholder dropped or renamed in a translation
//! (`placeholders.rs`): `leptos_i18n` would render it without its value.
//!
//! **The build's git identity** ([`capture_git_info`]): captures the git
//! branch and short commit this service was built from, as
//! `LEARN_PALI_GIT_BRANCH` / `LEARN_PALI_GIT_SHA` (read with `env!` in
//! `src/app.rs`), so the badge the server renders and the one the browser
//! hydrates always agree: both come from the very same build.
//!
//! The values themselves come from `git` (it already knows how to resolve
//! a worktree, a detached `HEAD`, a packed ref); only the paths Cargo
//! should watch for a rebuild (`cargo:rerun-if-changed`) are resolved here
//! by hand, because a worktree's `.git` is a *file* (`gitdir: <path>`)
//! pointing at the real one, and the branch's own ref lives in the
//! *common* git dir (`commondir`), shared by every worktree. The pure
//! decisions (`git`'s raw output in, a displayable value out) live in
//! [`git_info`], included here and, separately, as a module of the
//! crate itself, so `cargo test -p app` covers them.
//!
//! `git` missing, or this not being a git checkout at all: falls back to
//! `"unknown"` for both, with a build warning. The image build has no
//! `.git`: it passes `LEARN_PALI_GIT_BRANCH` and `LEARN_PALI_GIT_COMMIT`
//! (build args → env), which win when both are set
//! ([`git_info::badge_from_env`]); a local build sets none and asks git.

#[path = "src/git_info.rs"]
mod git_info;

use std::path::{Path, PathBuf};
use std::process::Command;

use leptos_i18n_parser::parse_locales::options::{Config, ParseOptions};
use leptos_i18n_parser::parse_locales::parse_locales;

#[path = "build/placeholders.rs"]
mod placeholders;

/// The default locale, then the others: every `Language` code.
const LOCALES: [&str; 3] = ["en", "fr", "vi"];

fn main() {
    generate_catalogue();
    capture_git_info();
}

/// Generates the typed string catalogue into `OUT_DIR/i18n/mod.rs`,
/// failing the build when the locales disagree on their keys.
fn generate_catalogue() {
    // `interpolate_display`: `td_string!` with arguments renders to a
    // string, for an attribute (an `aria-label` with a count in it).
    let config = Config::new(LOCALES[0])
        .and_then(|config| config.add_locales(&LOCALES[1..]))
        .expect("the locale list is valid")
        .parse_options(ParseOptions::new().interpolate_display(true));
    let parsed = parse_locales(None, config).expect("locales/*.json parse");
    for file in parsed.tracked_files.iter().flatten() {
        println!("cargo::rerun-if-changed={file}");
    }
    let (errors, warnings) = parsed.diag.borrow();
    let problems: Vec<String> = errors
        .iter()
        .map(ToString::to_string)
        .chain(warnings.iter().map(ToString::to_string))
        .collect();
    assert!(
        problems.is_empty(),
        "the catalogues disagree:\n{}",
        problems.join("\n")
    );
    drop((errors, warnings));
    assert_placeholder_parity();
    let code = leptos_i18n_codegen::gen_code(&parsed, None, false, None, true)
        .expect("the catalogue generates");
    let dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("i18n");
    std::fs::create_dir_all(&dir).expect("create OUT_DIR/i18n");
    std::fs::write(dir.join("mod.rs"), code.to_string()).expect("write the catalogue");
}

/// Every locale's `{{ name }}` placeholders match the default locale's,
/// key by key.
fn assert_placeholder_parity() {
    let texts: Vec<(&str, String)> = LOCALES
        .iter()
        .map(|code| {
            let path = format!("locales/{code}.json");
            (
                *code,
                std::fs::read_to_string(&path).expect("read a locale file"),
            )
        })
        .collect();
    let locales: Vec<(&str, &str)> = texts.iter().map(|(c, t)| (*c, t.as_str())).collect();
    let mismatches = placeholders::placeholder_mismatches(&locales);
    assert!(
        mismatches.is_empty(),
        "the catalogues' placeholders disagree:\n{}",
        mismatches.join("\n")
    );
}

/// Sets `LEARN_PALI_GIT_BRANCH` / `LEARN_PALI_GIT_SHA` for the env
/// badge, and watches the refs they come from.
fn capture_git_info() {
    println!("cargo:rerun-if-env-changed=LEARN_PALI_GIT_BRANCH");
    println!("cargo:rerun-if-env-changed=LEARN_PALI_GIT_COMMIT");
    // The image build is told (build args → env) and has no `.git`.
    let told = git_info::badge_from_env(
        std::env::var("LEARN_PALI_GIT_BRANCH").ok(),
        std::env::var("LEARN_PALI_GIT_COMMIT").ok(),
    );
    if let Some((branch, sha)) = told {
        println!("cargo:rustc-env=LEARN_PALI_GIT_BRANCH={branch}");
        println!("cargo:rustc-env=LEARN_PALI_GIT_SHA={sha}");
        return;
    }
    let branch_raw = git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let sha_raw = git(&["rev-parse", "--short", "HEAD"]);
    if branch_raw.is_none() || sha_raw.is_none() {
        println!("cargo:warning=env_badge: `git` unavailable, using \"unknown\"");
    }
    let (branch, sha) = git_info::branch_and_sha(branch_raw.clone(), sha_raw);
    let sha = git_info::sha_with_dirty_marker(&sha, is_dirty());

    println!("cargo:rustc-env=LEARN_PALI_GIT_BRANCH={branch}");
    println!("cargo:rustc-env=LEARN_PALI_GIT_SHA={sha}");

    watch_git_refs(branch_raw.as_deref());
}

/// Runs `git <args>` from the crate's directory; `None` on anything but a
/// clean, non-empty result (`git` missing, not a repository, detached
/// `HEAD` reporting `"HEAD"` is kept — [`git_info::display_branch`]
/// decides what that shows).
fn git(args: &[&str]) -> Option<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let out = Command::new("git")
        .args(args)
        .current_dir(manifest_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Uncommitted changes in the worktree (tracked or not) at build time.
fn is_dirty() -> bool {
    let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return false;
    };
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(manifest_dir)
        .output();
    out.is_ok_and(|out| out.status.success() && git_info::is_dirty_output(&out.stdout))
}

/// Asks Cargo to rerun this build script when the resolved `HEAD` or index
/// changes and when `branch_raw`'s own ref changes (a commit advances it,
/// from elsewhere); silently does nothing it cannot resolve.
fn watch_git_refs(branch_raw: Option<&str>) {
    let Some(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from) else {
        return;
    };
    let Some(dot_git) = find_dot_git(&manifest_dir) else {
        return;
    };
    let git_dir = resolve_git_dir(&dot_git);
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    println!("cargo:rerun-if-changed={}", git_dir.join("index").display());

    let Some(branch_raw) = branch_raw.filter(|b| *b != "HEAD") else {
        return;
    };
    let common_dir = resolve_common_dir(&git_dir);
    let ref_path = common_dir.join("refs/heads").join(branch_raw);
    if ref_path.exists() {
        println!("cargo:rerun-if-changed={}", ref_path.display());
    }
}

/// Walks up from `start` to find a `.git` entry (a directory for a plain
/// checkout, a file — `gitdir: <path>` — for a worktree).
fn find_dot_git(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        let candidate = dir.join(".git");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// The real git directory `dot_git` points to: itself when it is already a
/// directory, or [`git_info::parse_gitdir_pointer`] of its content when it
/// is a worktree's `.git` file.
fn resolve_git_dir(dot_git: &Path) -> PathBuf {
    if dot_git.is_dir() {
        return dot_git.to_path_buf();
    }
    let content = std::fs::read_to_string(dot_git).unwrap_or_default();
    let parent = dot_git.parent().unwrap_or(dot_git);
    git_info::parse_gitdir_pointer(&content, parent)
}

/// The git directory every worktree shares (refs, objects): `git_dir`
/// itself, unless its `commondir` file names another one.
fn resolve_common_dir(git_dir: &Path) -> PathBuf {
    let commondir_file = git_dir.join("commondir");
    let Ok(content) = std::fs::read_to_string(&commondir_file) else {
        return git_dir.to_path_buf();
    };
    git_info::parse_commondir(&content, git_dir)
}
