//! This service's build script (copied from `ui-lab`'s gallery
//! `build.rs`, task 032's pattern), one job.
//!
//! **The string catalogue** ([`generate_catalogue`]): generates this
//! service's typed catalogue (`leptos_i18n`) from `locales/<code>.json`,
//! one file per [`palicanon_ui_lib::Language`] code. A key missing from
//! (or surplus in) any locale fails the build: `leptos_i18n` only warns
//! and falls back to English, which would ship a mixed-language page.
//! So does a `{{ name }}` placeholder dropped or renamed in a translation
//! (`placeholders.rs`): `leptos_i18n` would render it without its value.
//!
//! The build reads no git: the footer badge is the deploy's, not the
//! image's (`src/release.rs`), so the image is identical across refs.

use std::path::PathBuf;

use leptos_i18n_parser::parse_locales::options::{Config, ParseOptions};
use leptos_i18n_parser::parse_locales::parse_locales;

#[path = "build/placeholders.rs"]
mod placeholders;

/// The default locale, then the others: every `Language` code.
const LOCALES: [&str; 3] = ["en", "fr", "vi"];

fn main() {
    generate_catalogue();
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
