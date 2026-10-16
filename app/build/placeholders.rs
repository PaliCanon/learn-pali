// Vendored from ui-lib v0.3.0 build/placeholders.rs until ui-lib 068 publishes it as the palicanon-i18n-check crate.

//! Placeholder parity across a catalogue's locales, for the `build.rs` of
//! every crate with a `leptos_i18n` catalogue (the library's and the
//! gallery's include this file with `#[path]`).
//!
//! `leptos_i18n` fails nothing when a translation drops or renames a
//! `{{ name }}` placeholder: the key builds, and the page renders without
//! the value. Its parser reports missing and surplus keys only, so this
//! compares each key's placeholder names across the locales itself — a
//! plural form only some locales have (fr's `_many`) included, against
//! the default locale's `_other` form.

use std::collections::{BTreeMap, BTreeSet};

/// One line per key whose `{{ name }}` placeholders differ, in any locale,
/// from the first (default) locale's. `locales` are `(code, JSON text)`.
/// A plural form the default locale lacks (`items_many`) is compared with
/// its `_other` form (`reference_key`); any other key missing on one
/// side is left to `leptos_i18n`'s own check.
pub fn placeholder_mismatches(locales: &[(&str, &str)]) -> Vec<String> {
    let Some(((default_code, default_text), others)) = locales.split_first() else {
        return Vec::new();
    };
    let reference = placeholders_by_key(default_text);
    let mut found = Vec::new();
    for (code, text) in others {
        for (key, names) in placeholders_by_key(text) {
            let Some(compared) = reference_key(&key, &reference) else {
                continue;
            };
            let expected = &reference[&compared];
            if *expected != names {
                found.push(format!(
                    "{code}: \"{key}\" has placeholders {names:?}, {default_code} \"{compared}\" has {expected:?}"
                ));
            }
        }
    }
    found
}

/// The plural forms `leptos_i18n` reads from a key's suffix.
const PLURAL_FORMS: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// The default locale's key that `key` is compared with: itself, or, for a
/// plural form the default locale lacks (`items_many`), its `_other`
/// form — every locale has that one. `None`: nothing to compare with.
fn reference_key(key: &str, reference: &BTreeMap<String, BTreeSet<String>>) -> Option<String> {
    if reference.contains_key(key) {
        return Some(key.to_owned());
    }
    let (base, form) = key.rsplit_once('_')?;
    let other = format!("{base}_other");
    (PLURAL_FORMS.contains(&form) && reference.contains_key(&other)).then_some(other)
}

/// Every string of a locale file, by its dotted key path, with the set of
/// its placeholder names. Panics on a file that is not JSON: the build
/// stops there anyway.
fn placeholders_by_key(text: &str) -> BTreeMap<String, BTreeSet<String>> {
    let value: serde_json::Value = serde_json::from_str(text).expect("a locale file is JSON");
    let mut keys = BTreeMap::new();
    collect(&value, String::new(), &mut keys);
    keys
}

/// Walks `value`, recording each string under `path` (`a.b`, `list.0`).
fn collect(value: &serde_json::Value, path: String, keys: &mut BTreeMap<String, BTreeSet<String>>) {
    let join = |child: &str| {
        if path.is_empty() {
            child.to_owned()
        } else {
            format!("{path}.{child}")
        }
    };
    match value {
        serde_json::Value::String(text) => {
            keys.insert(path, names(text));
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                collect(child, join(key), keys);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect(child, join(&index.to_string()), keys);
            }
        }
        _ => {}
    }
}

/// The names of `text`'s `{{ name }}` / `{{ name, formatter }}`
/// placeholders, as `leptos_i18n` reads them (trimmed, formatter dropped).
fn names(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some((_, after_open)) = rest.split_once("{{") {
        let Some((inside, after_close)) = after_open.split_once("}}") else {
            break;
        };
        let name = inside.split(',').next().unwrap_or_default().trim();
        found.insert(name.to_owned());
        rest = after_close;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_placeholder_is_reported() {
        let found = placeholder_mismatches(&[
            ("en", r#"{"items": "{{ count }} items"}"#),
            ("fr", r#"{"items": "des articles"}"#),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        for part in ["fr", "items", "count"] {
            assert!(found[0].contains(part), "{part}: {found:?}");
        }
    }

    #[test]
    fn a_renamed_placeholder_is_reported() {
        let found = placeholder_mismatches(&[
            ("en", r#"{"items": "{{ count }} items"}"#),
            ("fr", r#"{"items": "{{ count }} articles"}"#),
            ("vi", r#"{"items": "{{ so }} mục"}"#),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("vi"), "{found:?}");
    }

    #[test]
    fn order_spacing_and_formatters_do_not_matter() {
        let found = placeholder_mismatches(&[
            ("en", r#"{"pair": "{{ a }} and {{b}}"}"#),
            ("fr", r#"{"pair": "{{b}} et {{ a, number }}"}"#),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn nested_keys_are_compared_by_their_path() {
        let found = placeholder_mismatches(&[
            ("en", r#"{"group": {"hello": "Hello {{ name }}"}}"#),
            ("fr", r#"{"group": {"hello": "Bonjour"}}"#),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("group.hello"), "{found:?}");
    }

    /// A plural form only one locale has (fr's `_many`, beside en's `_one`
    /// and `_other`) is no surplus key to `leptos_i18n`: it is compared
    /// with the default locale's `_other` form (T22).
    #[test]
    fn a_plural_form_only_one_locale_has_is_compared_with_other() {
        let en = r#"{"items_one": "{{ count }} item", "items_other": "{{ count }} items"}"#;
        let found = placeholder_mismatches(&[
            ("en", en),
            (
                "fr",
                r#"{"items_one": "{{ count }} article", "items_other": "{{ count }} articles", "items_many": "beaucoup d'articles"}"#,
            ),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        for part in ["fr", "items_many", "items_other", "count"] {
            assert!(found[0].contains(part), "{part}: {found:?}");
        }
        let kept = placeholder_mismatches(&[
            ("en", en),
            ("fr", r#"{"items_many": "{{ count }} d'articles"}"#),
        ]);
        assert!(kept.is_empty(), "{kept:?}");
    }

    #[test]
    fn the_library_catalogue_agrees() {
        let found = placeholder_mismatches(&[
            ("en", include_str!("../locales/en.json")),
            ("fr", include_str!("../locales/fr.json")),
            ("vi", include_str!("../locales/vi.json")),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }
}
