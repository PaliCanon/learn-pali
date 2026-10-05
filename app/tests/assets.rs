//! The service's static pieces: the two assets, the stylesheet's inputs,
//! `.gitignore` and the README's run and check instructions.

use std::path::{Path, PathBuf};

fn app(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn root(path: &str) -> String {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

#[test]
fn the_favicon_and_the_lotus_exist_and_are_not_empty() {
    for asset in ["public/favicon.ico", "public/lotus.png"] {
        let meta = std::fs::metadata(app(asset)).unwrap_or_else(|e| panic!("{asset}: {e}"));
        assert!(meta.len() > 0, "{asset} is empty");
    }
}

#[test]
fn the_stylesheet_imports_ui_libs_theme_and_scans_both_sources() {
    let css = std::fs::read_to_string(app("style/main.css")).expect("main.css");
    assert!(css.contains(r#"@import "tailwindcss";"#), "{css}");
    assert!(
        css.contains(r#"@import "../../../ui-lib/style/theme.css";"#),
        "{css}"
    );
    assert!(css.contains(r#"@source "../../../ui-lib/src";"#), "{css}");
    assert!(css.contains(r#"@source "../src";"#), "{css}");
}

#[test]
fn gitignore_lists_target_and_a2m() {
    let ignore = root(".gitignore");
    let lines: Vec<&str> = ignore.lines().collect();
    assert!(lines.contains(&"/target"), "{ignore}");
    assert!(lines.contains(&".a2m/"), "{ignore}");
}

#[test]
fn the_readme_names_the_port_and_every_check_command() {
    let readme = root("README.md");
    assert!(readme.contains("3201"), "port 3201");
    for command in [
        "cargo fmt --all --check",
        "cargo clippy -p learn_pali --features ssr --all-targets -- -D warnings",
        "cargo clippy -p learn_pali --lib --features hydrate --target wasm32-unknown-unknown -- -D warnings",
        "cargo test -p learn_pali --features ssr",
        r#"RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p learn_pali --features ssr"#,
        "cargo clippy -p learn_core -p learn_cli -p build_deck --all-targets -- -D warnings",
        "cargo clippy -p learn_core --lib --target wasm32-unknown-unknown -- -D warnings",
        "cargo test -p learn_core -p learn_cli -p build_deck",
        r#"RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p learn_pali --features ssr -p learn_core -p learn_cli -p build_deck"#,
        "cargo leptos build",
        "cargo deny check advisories bans sources licenses",
    ] {
        assert!(readme.contains(command), "README lacks: {command}");
    }
}

#[test]
fn the_readme_shows_the_loop_the_deck_command_and_the_deck_licence() {
    let readme = root("README.md");
    for needle in [
        "cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json new 5",
        "cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json review",
        "cargo run -p learn_cli -- --deck core/fixtures/deck.json --state /tmp/state.json stats",
        "--today 2026-10-13 review",
        "cargo run -p build_deck -- ",
        "--strict",
        "50 terms, 41 with forms, 50 with a blank, 0 warnings, 0 notes, empty cells: 3",
        "Digital Pāḷi Dictionary v0.4.20260926",
        "CC BY-NC-SA 4.0",
        "MIT OR Apache-2.0",
    ] {
        assert!(readme.contains(needle), "README lacks: {needle}");
    }
}
