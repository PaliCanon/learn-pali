//! The learner's progress in the browser: `localStorage[pc_learn]` ↔
//! `learn_core::State` JSON.
//!
//! The pure decisions, tested natively: [`classify`] (what a stored text
//! is), [`validate_import`] (an imported file, all or nothing),
//! [`export_name`]. The browser glue — [`load`], [`save`], [`download`],
//! [`read_file`], [`on_storage_change`] — exists in every build so the
//! pages compile without `cfg`; under `ssr` it is a no-op, never called
//! (the store is read in an Effect, after hydration).
//!
//! The deck is never authoritative over the state: a term the deck does
//! not know stays (an orphan), listed apart, never counted.

use learn_core::date::Date;
use learn_core::progress::{State, StateError};
use leptos::html;
use leptos::prelude::NodeRef;

/// The `localStorage` key of the progress.
pub(crate) const KEY: &str = "pc_learn";

/// What the store holds, read.
#[cfg_attr(
    not(any(feature = "hydrate", test)),
    expect(
        dead_code,
        reason = "read by the hydrate glue (and the tests); the server never touches the storage"
    )
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Load {
    /// A state this build reads, with at least one entry.
    Ok(State),
    /// Nothing yet: no key, or a state with no entry.
    Empty,
    /// Not a state this build reads (bad JSON, version 0, a duplicate
    /// term); `raw` kept for 'Download the raw copy'.
    Invalid {
        /// The stored text, as is.
        raw: String,
    },
    /// Saved by a newer version (A3: a version above this build's):
    /// read-only, never overwritten (Q3); `raw` kept for the download.
    NewerVersion {
        /// The stored text, as is.
        raw: String,
    },
}

/// Why an imported file was refused; the progress is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportError {
    /// Not a state (bad JSON, or a version below this build's).
    NotAState,
    /// A state saved by a newer version.
    Newer,
    /// The same term twice.
    DuplicateTerm,
}

/// What the stored text `raw` is (`None`: no key). A valid state is
/// [`Load::Ok`] even with no entry: [`Load::Empty`] is the absence of the
/// key alone, and the home decides what an empty state shows.
#[cfg_attr(
    not(any(feature = "hydrate", test)),
    expect(
        dead_code,
        reason = "read by the hydrate glue (and the tests); the server never touches the storage"
    )
)]
pub(crate) fn classify(raw: Option<&str>) -> Load {
    let Some(raw) = raw else {
        return Load::Empty;
    };
    match State::from_json(raw) {
        Ok(state) => Load::Ok(state),
        // `from_json` reads the `version` before the shape: a newer
        // version is told apart whatever else it holds.
        Err(StateError::UnknownVersion(version)) if version > State::VERSION => {
            Load::NewerVersion {
                raw: raw.to_owned(),
            }
        }
        Err(_) => Load::Invalid {
            raw: raw.to_owned(),
        },
    }
}

/// An imported file's text as a whole state, or why it is refused; never
/// a partial state.
///
/// # Errors
/// [`ImportError::NotAState`] (not JSON, not a state, or a version below
/// this build's), [`ImportError::Newer`] (a version above), or
/// [`ImportError::DuplicateTerm`].
pub(crate) fn validate_import(text: &str) -> Result<State, ImportError> {
    State::from_json(text).map_err(|error| match error {
        StateError::UnknownVersion(version) if version > State::VERSION => ImportError::Newer,
        StateError::DuplicateTerm(_) => ImportError::DuplicateTerm,
        StateError::Json(_) | StateError::UnknownVersion(_) => ImportError::NotAState,
    })
}

/// The export's file name: `pc_learn-<YYYY-MM-DD>.json`.
pub(crate) fn export_name(date: Date) -> String {
    format!("{KEY}-{date}.json")
}

/// Reads the stored progress ([`classify`] of `localStorage[pc_learn]`);
/// [`Load::Empty`] on the server, and when the browser refuses storage.
pub(crate) fn load() -> Load {
    #[cfg(feature = "hydrate")]
    {
        browser::load()
    }
    #[cfg(not(feature = "hydrate"))]
    {
        Load::Empty
    }
}

/// Writes `state` to `localStorage[pc_learn]`, synchronously (before the
/// next screen); nothing on the server. A refused write (private mode, a
/// full quota) is not reported here: the progress stays in memory.
pub(crate) fn save(state: &State) {
    #[cfg(feature = "hydrate")]
    browser::save(state);
    #[cfg(not(feature = "hydrate"))]
    let _ = state;
}

/// Offers `text` as a file download named `name` (a Blob URL, revoked once
/// the click is made); nothing on the server.
pub(crate) fn download(name: &str, text: &str) {
    #[cfg(feature = "hydrate")]
    browser::download(name, text);
    #[cfg(not(feature = "hydrate"))]
    let _ = (name, text);
}

/// Reads the file chosen in `input` as text and calls `on_text` with it
/// (`FileReader`'s onload); a failed read calls it with empty text (onerror),
/// which the import validation refuses; nothing when no file is chosen, nor
/// on the server.
pub(crate) fn read_file(input: NodeRef<html::Input>, on_text: impl FnOnce(String) + 'static) {
    #[cfg(feature = "hydrate")]
    browser::read_file(input, on_text);
    #[cfg(not(feature = "hydrate"))]
    let _ = (input, on_text);
}

/// Calls `on_change` when another tab changes `pc_learn` (the `storage`
/// event; other keys are ignored), for as long as the current reactive
/// owner lives (the listener is removed in its cleanup); nothing on the
/// server.
pub(crate) fn on_storage_change(on_change: impl Fn() + 'static) {
    #[cfg(feature = "hydrate")]
    browser::on_storage_change(on_change);
    #[cfg(not(feature = "hydrate"))]
    let _ = on_change;
}

/// The browser side, compiled only for the hydrate build; no native test
/// (the orchestrator's CDP probe and the user's check cover it).
#[cfg(feature = "hydrate")]
mod browser {
    use super::{KEY, Load, classify};
    use learn_core::progress::State;
    use leptos::html;
    use leptos::prelude::{GetUntracked, NodeRef, on_cleanup, window_event_listener};
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use web_sys::{Blob, BlobPropertyBag, FileReader, HtmlAnchorElement, Storage, Url};

    /// `window.localStorage`, `None` when the browser refuses it.
    fn storage() -> Option<Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    /// See [`super::load`].
    pub(super) fn load() -> Load {
        let raw = storage().and_then(|storage| storage.get_item(KEY).ok().flatten());
        classify(raw.as_deref())
    }

    /// See [`super::save`].
    pub(super) fn save(state: &State) {
        if let Some(storage) = storage() {
            // A refused write keeps the progress in memory only.
            let _ = storage.set_item(KEY, &state.to_json());
        }
    }

    /// The Blob URL of `text` (`application/json`).
    fn json_url(text: &str) -> Option<String> {
        let parts = js_sys::Array::of1(&text.into());
        let options = BlobPropertyBag::new();
        options.set_type("application/json");
        let blob = Blob::new_with_str_sequence_and_options(&parts, &options).ok()?;
        Url::create_object_url_with_blob(&blob).ok()
    }

    /// See [`super::download`].
    pub(super) fn download(name: &str, text: &str) {
        let Some(url) = json_url(text) else {
            return;
        };
        let anchor = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.create_element("a").ok())
            .and_then(|element| element.dyn_into::<HtmlAnchorElement>().ok());
        if let Some(anchor) = anchor {
            anchor.set_href(&url);
            anchor.set_download(name);
            anchor.click();
        }
        let _ = Url::revoke_object_url(&url);
    }

    /// See [`super::read_file`]. The one-shot onload and onerror closures
    /// share the callback: whichever fires first takes it, so it runs
    /// once. A read that fails calls it with empty text, which the import
    /// validation refuses as `NotAState`: the page says why instead of
    /// staying silent. Hydrate-only glue, so no native test (F7, review
    /// #4006).
    pub(super) fn read_file(input: NodeRef<html::Input>, on_text: impl FnOnce(String) + 'static) {
        let Some(file) = input
            .get_untracked()
            .and_then(|input| input.files())
            .and_then(|files| files.item(0))
        else {
            return;
        };
        let Ok(reader) = FileReader::new() else {
            return;
        };
        let callback = Rc::new(RefCell::new(Some(on_text)));
        let source = reader.clone();
        let on_loaded = Rc::clone(&callback);
        let onload = wasm_bindgen::closure::Closure::once_into_js(move || {
            let text = source.result().ok().and_then(|text| text.as_string());
            if let Some(on_text) = on_loaded.borrow_mut().take() {
                on_text(text.unwrap_or_default());
            }
        });
        let onerror = wasm_bindgen::closure::Closure::once_into_js(move || {
            if let Some(on_text) = callback.borrow_mut().take() {
                on_text(String::new());
            }
        });
        reader.set_onload(Some(onload.unchecked_ref()));
        reader.set_onerror(Some(onerror.unchecked_ref()));
        let _ = reader.read_as_text(&file);
    }

    /// See [`super::on_storage_change`].
    pub(super) fn on_storage_change(on_change: impl Fn() + 'static) {
        let handle = window_event_listener(leptos::ev::storage, move |event| {
            if event.key().as_deref() == Some(KEY) {
                on_change();
            }
        });
        on_cleanup(move || handle.remove());
    }
}

#[cfg(test)]
mod tests {
    use super::{ImportError, Load, classify, export_name, validate_import};
    use learn_core::date::Date;
    use learn_core::deck::TermId;
    use learn_core::progress::{Progress, State};

    fn one_term_state() -> State {
        let today = Date::parse("2026-10-05").unwrap();
        let mut state = State::empty();
        state.last_today = Some(today);
        state
            .progress
            .push(Progress::new(TermId("sati 1.1".into()), today));
        state
    }

    const NEWER: &str = r#"{"version": 2, "something": "else"}"#;
    /// A state that serialises well but holds one term twice.
    fn duplicate() -> String {
        let mut state = one_term_state();
        state.progress.push(state.progress[0].clone());
        state.to_json()
    }

    #[test]
    fn no_key_is_empty() {
        assert_eq!(classify(None), Load::Empty);
    }

    #[test]
    fn a_valid_state_is_ok_even_with_no_progress() {
        let empty = State::empty();
        assert_eq!(classify(Some(&empty.to_json())), Load::Ok(empty));
        let state = one_term_state();
        assert_eq!(classify(Some(&state.to_json())), Load::Ok(state));
    }

    #[test]
    fn bad_json_is_invalid_and_keeps_the_raw_text() {
        assert_eq!(
            classify(Some("{not json")),
            Load::Invalid {
                raw: "{not json".into()
            }
        );
        assert_eq!(classify(Some("")), Load::Invalid { raw: String::new() });
    }

    #[test]
    fn version_zero_is_invalid() {
        let raw = r#"{"version":0,"progress":[]}"#;
        assert_eq!(classify(Some(raw)), Load::Invalid { raw: raw.into() });
    }

    #[test]
    fn a_duplicate_term_is_invalid() {
        let raw = duplicate();
        assert_eq!(classify(Some(&raw)), Load::Invalid { raw });
    }

    #[test]
    fn a_newer_version_is_read_only_whatever_its_shape() {
        assert_eq!(
            classify(Some(NEWER)),
            Load::NewerVersion { raw: NEWER.into() }
        );
    }

    #[test]
    fn an_import_of_a_valid_file_is_the_whole_state() {
        let state = one_term_state();
        assert_eq!(validate_import(&state.to_json()), Ok(state));
    }

    #[test]
    fn an_import_refuses_what_is_not_a_state() {
        assert_eq!(validate_import("{nope"), Err(ImportError::NotAState));
        assert_eq!(validate_import("[1, 2]"), Err(ImportError::NotAState));
        assert_eq!(
            validate_import(r#"{"version":0,"progress":[]}"#),
            Err(ImportError::NotAState)
        );
    }

    #[test]
    fn an_import_refuses_a_newer_version() {
        assert_eq!(validate_import(NEWER), Err(ImportError::Newer));
    }

    #[test]
    fn an_import_refuses_a_duplicate_term() {
        assert_eq!(
            validate_import(&duplicate()),
            Err(ImportError::DuplicateTerm)
        );
    }

    #[test]
    fn the_export_name_carries_the_date() {
        let date = Date::parse("2026-10-05").unwrap();
        assert_eq!(export_name(date), "pc_learn-2026-10-05.json");
        assert_eq!(export_name(Date::from_days(0)), "pc_learn-1970-01-01.json");
    }
}
