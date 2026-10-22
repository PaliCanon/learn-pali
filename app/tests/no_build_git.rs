//! The badge is the deploy's, not the image's (task 060): the image is
//! identical across refs, so nothing in the build reads git. These tests
//! read `build.rs`, the `Dockerfile` and every source file and fail when
//! one of them mentions the git capture again, or when `src/git_info.rs`
//! comes back.
//!
//! What they do not catch: the same capture under another name (a renamed
//! variable, a `git` call spelled differently); they guard the words the
//! old capture used.

#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

/// The words the removed capture was made of.
const FORBIDDEN: [&str; 7] = [
    "LEARN_PALI_GIT",
    "git_info",
    "capture_git_info",
    "rev-parse",
    "git status",
    "commondir",
    "\"cargo:rustc-env",
];

/// The crate's directory (`app/`).
fn app_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the source directory is readable") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_files(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
}

/// The forbidden words found in `path`, one line each.
fn offences(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).expect("the file is readable");
    FORBIDDEN
        .iter()
        .filter(|word| text.contains(**word))
        .map(|word| format!("{} mentions `{word}`", path.display()))
        .collect()
}

#[test]
fn the_build_script_and_the_dockerfile_do_not_read_git() {
    let found: Vec<String> = ["build.rs", "Dockerfile"]
        .iter()
        .flat_map(|name| offences(&app_dir().join(name)))
        .collect();
    assert_eq!(found, Vec::<String>::new(), "the image must not read git");
}

#[test]
fn no_source_file_reads_the_build_time_git() {
    let mut files = Vec::new();
    rust_files(&app_dir().join("src"), &mut files);
    files.sort();
    let found: Vec<String> = files.iter().flat_map(|file| offences(file)).collect();
    assert_eq!(
        found,
        Vec::<String>::new(),
        "no source reads the build's git"
    );
}

#[test]
fn the_git_info_module_is_gone() {
    let path = app_dir().join("src/git_info.rs");
    assert!(!path.exists(), "{} must not exist", path.display());
}
