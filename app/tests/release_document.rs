//! The browser reads the release from the two `<meta>` tags the server
//! wrote in `<head>` (task 060): `ReleaseInfo::from_document`, with every
//! meta present, none, only one of them, and a blank env.
//!
//! Run with `wasm-bindgen-test-runner` and a headless Chrome:
//! `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
//! CHROMEDRIVER=<chromedriver> cargo test --locked -p learn_pali --features
//! csr --target wasm32-unknown-unknown --test release_document`.

#![cfg(all(target_arch = "wasm32", feature = "csr"))]

use learn_pali::release::{ENV_META, ReleaseInfo, SHA_META};
use leptos::prelude::document;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// Every release meta of the page removed, so a case starts from none.
fn clear() {
    for name in [ENV_META, SHA_META] {
        while let Some(meta) = document()
            .query_selector(&format!(r#"meta[name="{name}"]"#))
            .expect("a selector")
        {
            meta.remove();
        }
    }
}

/// A `<meta name content>` added at the end of `<head>`.
fn add_meta(name: &str, content: &str) {
    document()
        .query_selector("head")
        .expect("a selector")
        .expect("the document's head")
        .insert_adjacent_html(
            "beforeend",
            &format!(r#"<meta name="{name}" content="{content}">"#),
        )
        .expect("a meta added");
}

/// [`ReleaseInfo::from_document`] over a head holding exactly `metas`.
fn read(metas: &[(&str, &str)]) -> ReleaseInfo {
    clear();
    for (name, content) in metas {
        add_meta(name, content);
    }
    let info = ReleaseInfo::from_document();
    clear();
    info
}

fn info(env: &str, sha: &str) -> ReleaseInfo {
    ReleaseInfo {
        env: env.to_owned(),
        sha: sha.to_owned(),
    }
}

#[wasm_bindgen_test]
fn both_metas_are_read() {
    assert_eq!(
        read(&[(ENV_META, "main"), (SHA_META, "34c489c")]),
        info("main", "34c489c")
    );
}

#[wasm_bindgen_test]
fn a_tag_has_an_empty_sha_meta() {
    assert_eq!(
        read(&[(ENV_META, "v0.1.8"), (SHA_META, "")]),
        info("v0.1.8", "")
    );
}

#[wasm_bindgen_test]
fn no_meta_is_unknown() {
    assert_eq!(read(&[]), ReleaseInfo::unknown());
}

#[wasm_bindgen_test]
fn only_the_env_meta_is_unknown() {
    assert_eq!(read(&[(ENV_META, "main")]), ReleaseInfo::unknown());
}

#[wasm_bindgen_test]
fn only_the_sha_meta_is_unknown() {
    assert_eq!(read(&[(SHA_META, "34c489c")]), ReleaseInfo::unknown());
}

#[wasm_bindgen_test]
fn a_blank_env_meta_is_unknown() {
    assert_eq!(
        read(&[(ENV_META, " "), (SHA_META, "34c489c")]),
        ReleaseInfo::unknown()
    );
}
