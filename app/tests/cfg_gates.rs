//! The grep gates of the build modes (task 048): the site builds with
//! `ssr` and `hydrate`, the PaliCanon app with `csr` alone, and the code
//! that touches the DOM is shared by both client modes through the private
//! `browser` feature. These tests read the sources and fail on:
//!
//! - any `feature = "hydrate"` outside `src/lib.rs` (the hydrate entry and
//!   the site's frame stay there; the rest of the DOM glue is `browser`);
//! - any `feature = "csr"` in `src/` (`csr` only turns on the dependencies'
//!   client mode);
//! - any `feature = "web-store"` outside `src/lib.rs` and `src/web.rs` (the
//!   `localStorage` backend and the file input live in that one module);
//!
//! and on `src/lib.rs` losing the plain gate of the module `web`, which
//! keeps every trace of the web backend out of a `csr` build.

#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

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

/// `line` with every whitespace removed.
fn squeeze(line: &str) -> String {
    line.split_whitespace().collect()
}

/// Whether `line` names `feature = "<name>"`, however it is spaced.
fn names_feature(line: &str, name: &str) -> bool {
    squeeze(line).contains(&format!(r#"feature="{name}""#))
}

/// The `file:line: text` of each line under `src/` that names `name`,
/// leaving out the files in `allowed` (relative to `src/`).
fn offenders(name: &str, allowed: &[&str]) -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut found = Vec::new();
    for file in files {
        let relative = file.strip_prefix(&src).expect("under src/");
        if allowed.iter().any(|ok| relative == Path::new(ok)) {
            continue;
        }
        let text = fs::read_to_string(&file).expect("a UTF-8 source file");
        for (number, line) in text.lines().enumerate() {
            if names_feature(line, name) {
                found.push(format!(
                    "{}:{}: {}",
                    relative.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    found
}

#[test]
fn no_hydrate_feature_outside_lib_rs() {
    let found = offenders("hydrate", &["lib.rs"]);
    assert_eq!(
        found.len(),
        0,
        "code that csr needs is gated on `hydrate`; use `browser`:\n{}",
        found.join("\n")
    );
}

#[test]
fn no_csr_feature_in_the_sources() {
    let found = offenders("csr", &[]);
    assert_eq!(
        found.len(),
        0,
        "`csr` gates no source; the shared DOM glue is `browser`:\n{}",
        found.join("\n")
    );
}

#[test]
fn no_web_store_feature_outside_the_web_module() {
    let found = offenders("web-store", &["lib.rs", "web.rs"]);
    assert_eq!(
        found.len(),
        0,
        "the web backend is gated in src/web.rs and declared in src/lib.rs only:\n{}",
        found.join("\n")
    );
}

/// The `#[cfg(...)]` line directly above the first line of `text` that
/// starts with `decl`, squeezed.
fn gate_above(text: &str, decl: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let at = lines.iter().position(|line| line.starts_with(decl))?;
    // Doc lines, attributes, plain comments and blank lines may sit between
    // the gate and the declaration; the first line of anything else ends it.
    lines[..at]
        .iter()
        .rev()
        .take_while(|line| {
            line.is_empty()
                || line.starts_with("//")
                || line.starts_with("#[")
                || line.starts_with("#![")
        })
        .find(|line| line.starts_with("#[cfg("))
        .map(|line| squeeze(line))
}

#[test]
fn a_gate_is_found_across_a_blank_line_and_a_comment() {
    let text = "use a;\n\n#[cfg(feature = \"x\")]\n\n// why\nmod web;\n";
    assert_eq!(
        gate_above(text, "mod web;").as_deref(),
        Some(r#"#[cfg(feature="x")]"#)
    );
    assert_eq!(gate_above("use a;\nmod web;\n", "mod web;"), None);
    assert_eq!(gate_above("use a;\n", "mod web;"), None);
}

#[test]
fn the_web_module_is_gated_on_ssr_or_web_store_alone() {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("lib.rs");
    let gate = gate_above(&text, "mod web;");
    assert_eq!(
        gate.as_deref(),
        Some(r#"#[cfg(any(feature="ssr",feature="web-store"))]"#),
        "a csr build must hold nothing of the web backend"
    );
}

#[test]
fn the_gates_see_through_spacing() {
    for line in [
        r#"#[cfg(feature = "hydrate")]"#,
        r#"#[cfg(feature="hydrate")]"#,
        r#"#[cfg(feature  =  "hydrate")]"#,
        "#[cfg(any(feature =\t\"hydrate\", test))]",
        r#"#[cfg(not(feature="hydrate"))]"#,
    ] {
        assert!(names_feature(line, "hydrate"), "{line}");
    }
    for line in [
        r#"#[cfg(feature = "browser")]"#,
        r#"#[cfg(feature = "web-store")]"#,
        "// hydrate with feature flags",
    ] {
        assert!(!names_feature(line, "hydrate"), "{line}");
    }
}
