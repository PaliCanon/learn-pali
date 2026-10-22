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

// ---- The CI feature flags of the browser tests (task 059, F11) ----

/// Every `feature = "..."` named on `line`, in order.
fn features_named(line: &str) -> Vec<String> {
    let squeezed = squeeze(line);
    let mut found = Vec::new();
    let mut rest = squeezed.as_str();
    while let Some(at) = rest.find(r#"feature=""#) {
        rest = &rest[at + r#"feature=""#.len()..];
        let end = rest.find('"').expect("a closed string");
        found.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    found
}

/// The features the browser test file gates on, from its first inner
/// `#![cfg(all(target_arch = "wasm32", ...))]` line; `None` when the file
/// has no wasm gate (a native test).
fn wasm_gate(text: &str) -> Option<Vec<String>> {
    let gate = text
        .lines()
        .map(squeeze)
        .find(|line| line.starts_with("#![cfg("))?;
    // `not(target_arch = "wasm32")` is a native gate (this very file's).
    (gate.contains(r#"target_arch="wasm32""#) && !gate.contains(r#"not(target_arch="wasm32")"#))
        .then(|| features_named(&gate))
}

/// The package's `[features]` table, `name -> members`, read from
/// `Cargo.toml` (a member is a quoted string; `dep:` and `crate/feature`
/// members are not features of this package).
fn feature_table() -> Vec<(String, Vec<String>)> {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("Cargo.toml");
    let section = manifest
        .split("\n[features]")
        .nth(1)
        .expect("a [features] table")
        .split("\n[")
        .next()
        .expect("its end");
    let mut table: Vec<(String, Vec<String>)> = Vec::new();
    for line in section.lines().map(str::trim) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((name, rest)) = line.split_once('=') {
            table.push((name.trim().to_owned(), Vec::new()));
            add_members(&mut table, rest);
        } else {
            add_members(&mut table, line);
        }
    }
    table
}

/// Pushes the quoted members of `text` onto the last feature of `table`.
fn add_members(table: &mut [(String, Vec<String>)], text: &str) {
    for member in text.split('"').skip(1).step_by(2) {
        if !member.contains(':') && !member.contains('/') {
            table
                .last_mut()
                .expect("a feature")
                .1
                .push(member.to_owned());
        }
    }
}

/// `requested` and everything they switch on, through the table.
fn expand(requested: &[String], table: &[(String, Vec<String>)]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    let mut todo: Vec<String> = requested.to_vec();
    while let Some(name) = todo.pop() {
        if all.contains(&name) {
            continue;
        }
        if let Some((_, members)) = table.iter().find(|(feature, _)| *feature == name) {
            todo.extend(members.iter().cloned());
        }
        all.push(name);
    }
    all
}

/// The `--features` list of the workflow step that runs `--test <name>`,
/// or `None` when no step does.
fn ci_features_of(workflow: &str, name: &str) -> Option<Vec<String>> {
    let run = workflow
        .lines()
        .find(|line| line.trim_end().ends_with(&format!("--test {name}")))?;
    let mut words = run.split_whitespace();
    words.find(|word| *word == "--features")?;
    Some(words.next()?.split(',').map(str::to_owned).collect())
}

/// Each `(file stem, features its wasm gate names)` under `app/tests/`.
fn browser_test_files() -> Vec<(String, Vec<String>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("a UTF-8 test file");
        if let Some(features) = wasm_gate(&text) {
            let stem = file
                .file_stem()
                .expect("a stem")
                .to_string_lossy()
                .into_owned();
            found.push((stem, features));
        }
    }
    found
}

/// CI runs every browser test file with the features its gate names (task
/// 059, F11: a step with a missing flag compiles the file to nothing and
/// passes with zero tests). A feature a flag switches on counts (`csr`
/// brings `browser`); a file with a wasm gate and no `--test` step fails.
/// Green on arrival, a guard on a workflow that is right. It does not
/// catch a step that exists but is commented out of the job, a step in the
/// wrong job, or a `--features` flag spelled across two lines.
#[test]
fn ci_runs_each_browser_test_with_the_features_its_file_gates_on() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("ci.yml");
    let table = feature_table();
    let files = browser_test_files();
    let stems: Vec<&str> = files.iter().map(|(stem, _)| stem.as_str()).collect();
    for stem in ["csr", "web_sync", "web_add", "sync_triggers", "sync_hint"] {
        assert!(
            stems.contains(&stem),
            "the walk missed {stem}.rs: {stems:?}"
        );
    }
    for (stem, gates) in &files {
        let requested = ci_features_of(&workflow, stem)
            .unwrap_or_else(|| panic!("no CI step runs `--test {stem}` with --features"));
        let enabled = expand(&requested, &table);
        for gate in gates {
            assert!(
                enabled.contains(gate),
                "{stem}.rs gates on `{gate}`; its CI step enables only {enabled:?}"
            );
        }
    }
}

#[test]
fn the_ci_helpers_read_gates_steps_and_the_feature_table() {
    let gate = "//! doc\n\n#![cfg(all(target_arch = \"wasm32\", feature = \"csr\", feature=\"web-store\"))]\n";
    assert_eq!(
        wasm_gate(gate),
        Some(vec!["csr".to_owned(), "web-store".to_owned()])
    );
    assert_eq!(wasm_gate("#![cfg(not(target_arch = \"wasm32\"))]\n"), None);
    assert_eq!(wasm_gate("#![cfg(feature = \"ssr\")]\n"), None);
    assert_eq!(wasm_gate("fn main() {}\n"), None);
    let table = vec![
        ("csr".to_owned(), vec!["browser".to_owned()]),
        ("web-sync".to_owned(), vec!["web-store".to_owned()]),
        ("web-store".to_owned(), vec!["browser".to_owned()]),
    ];
    let mut enabled = expand(&["csr".to_owned(), "web-sync".to_owned()], &table);
    enabled.sort();
    assert_eq!(enabled, ["browser", "csr", "web-store", "web-sync"]);
    let step = "        run: cargo test -p x --features csr,web-sync --target t --test web_sync\n";
    assert_eq!(
        ci_features_of(step, "web_sync"),
        Some(vec!["csr".to_owned(), "web-sync".to_owned()])
    );
    assert_eq!(ci_features_of(step, "web"), None);
    assert_eq!(ci_features_of(step, "csr"), None);
}

/// The real manifest parses into the features the gates rely on: `csr`
/// switches `browser` on, `web-sync` switches `web-store` on.
#[test]
fn the_real_feature_table_makes_csr_bring_browser() {
    let table = feature_table();
    assert!(expand(&["csr".to_owned()], &table).contains(&"browser".to_owned()));
    let sync = expand(&["web-sync".to_owned()], &table);
    assert!(sync.contains(&"web-store".to_owned()) && sync.contains(&"browser".to_owned()));
}
