//! The site's host side: the progress in the browser's
//! `localStorage[pc_learn]` ([`web_store`]) and the files through a Blob
//! download and a file input ([`web_files`]). Today's behaviour, behind the
//! two traits of [`crate::backend`].
//!
//! Compiled for the site only (`ssr`, where it is a no-op that the server
//! never calls: the progress is read in an Effect after hydration; and
//! `web-store`, which `hydrate` enables). The app builds with `csr` alone
//! and plugs its own: nothing of this module is in its build.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::prelude::*;

use crate::backend::{
    FileTransfer, PickFailed, ProgressStore, SaveError, SaveStatus, SharedFiles, SharedStore,
};
use crate::store::{Load, classify};
use learn_core::progress::State;

/// The progress store of the web site: `localStorage[pc_learn]`, read and
/// written synchronously. Its status is `Saved` unless the browser refuses
/// a write (a full quota, private mode): then `Failed(Transient)`, the state
/// kept in memory until a write goes through. A storage that is absent or
/// throws on read is [`Load::Unreadable`], and nothing is written over it,
/// nor over a file saved by a newer version. It reports a change when
/// another tab writes the key (the `storage` event).
#[must_use]
pub fn web_store() -> SharedStore {
    Rc::new(WebStore::new(BrowserKv))
}

/// The file transfer of the web site: the export is a Blob URL download,
/// the import a hidden file input.
#[must_use]
pub fn web_files() -> SharedFiles {
    Rc::new(WebFiles)
}

/// `localStorage` could not be reached: absent, or it threw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Unavailable;

/// The one key of the progress, as [`WebStore`] sees the browser's storage.
trait KeyValue {
    /// The stored text; `Ok(None)` when the key is not there.
    fn get(&self) -> Result<Option<String>, Unavailable>;

    /// Stores `text`.
    fn set(&self, text: &str) -> Result<(), Unavailable>;
}

/// `localStorage[pc_learn]` (nothing, on the server).
struct BrowserKv;

impl KeyValue for BrowserKv {
    fn get(&self) -> Result<Option<String>, Unavailable> {
        browser::get()
    }

    fn set(&self, text: &str) -> Result<(), Unavailable> {
        browser::set(text)
    }
}

/// Which file pick is current. Starting a pick supersedes the one before:
/// its input is removed, and a late change of its chooser (some browsers
/// keep the dialog open) is ignored, so a stale chooser can never import.
#[derive(Default)]
#[cfg_attr(
    not(any(feature = "web-store", test)),
    expect(
        dead_code,
        reason = "used by the browser's file pick (web-store) and the tests"
    )
)]
struct Picks {
    /// The number of the latest pick.
    latest: Cell<u64>,
}

#[cfg_attr(
    not(any(feature = "web-store", test)),
    expect(
        dead_code,
        reason = "used by the browser's file pick (web-store) and the tests"
    )
)]
impl Picks {
    /// Starts a pick and returns its number.
    fn begin(&self) -> u64 {
        let pick = self.latest.get() + 1;
        self.latest.set(pick);
        pick
    }

    /// Whether `pick` is still the latest.
    fn is_current(&self, pick: u64) -> bool {
        self.latest.get() == pick
    }
}

/// [`web_store`]'s implementation over a [`KeyValue`].
struct WebStore<K> {
    /// Where the text lives.
    kv: K,
    /// The latest state a write did not take: what `load` answers while it
    /// is set, so a refused write loses nothing.
    unsaved: RefCell<Option<State>>,
    /// `Saved`, or `Failed(Transient)` while `unsaved` is set.
    status: RwSignal<SaveStatus>,
}

impl<K: KeyValue> WebStore<K> {
    /// A store over `kv`, status `Saved`.
    fn new(kv: K) -> Self {
        WebStore {
            kv,
            unsaved: RefCell::new(None),
            status: RwSignal::new(SaveStatus::Saved),
        }
    }

    /// What the storage holds, whatever the memory says.
    fn stored(&self) -> Load {
        match self.kv.get() {
            Ok(raw) => classify(raw.as_deref()),
            Err(Unavailable) => Load::Unreadable("localStorage is not available".to_owned()),
        }
    }
}

impl<K: KeyValue> ProgressStore for WebStore<K> {
    /// The storage is read first: what it cannot give (unreadable) or must
    /// keep (a newer file) is the answer, whatever the memory holds; else
    /// the memory of a refused write, else the storage.
    fn load(&self) -> Load {
        let stored = self.stored();
        if matches!(stored, Load::Unreadable(_) | Load::NewerVersion { .. }) {
            return stored;
        }
        match self.unsaved.borrow().as_ref() {
            Some(state) => Load::Ok(state.clone()),
            None => stored,
        }
    }

    /// Nothing is written over a storage that could not be read, or over a
    /// file saved by a newer version: that refusal says
    /// `Failed(NewerVersion { unsaved })`, so the strip offers the answer
    /// just given. A write the browser refuses keeps the
    /// state in memory and says `Failed(Transient)`; the next save tries
    /// again.
    fn save(&self, state: &State) {
        match self.stored() {
            Load::Unreadable(_) => return,
            Load::NewerVersion { .. } => {
                // The file wins; the answer just given is offered for Export.
                self.status.set(SaveStatus::Failed(SaveError::NewerVersion {
                    unsaved: state.clone(),
                }));
                return;
            }
            Load::Ok(_) | Load::Empty | Load::Invalid { .. } => {}
        }
        if self.kv.set(&state.to_json()).is_ok() {
            *self.unsaved.borrow_mut() = None;
            self.status.set(SaveStatus::Saved);
        } else {
            *self.unsaved.borrow_mut() = Some(state.clone());
            self.status.set(SaveStatus::Failed(SaveError::Transient));
        }
    }

    fn status(&self) -> Signal<SaveStatus> {
        self.status.into()
    }

    fn on_change(&self, callback: Box<dyn Fn()>) {
        browser::on_storage_change(callback);
    }
}

/// [`web_files`]'s implementation.
struct WebFiles;

impl FileTransfer for WebFiles {
    fn export(&self, name: &str, text: &str) {
        browser::download(name, text);
    }

    fn pick_import(&self, on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {
        browser::pick_file(on_done);
    }
}

/// The server's stubs: the web store is never called there.
#[cfg(not(feature = "web-store"))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the same signatures as the browser's, which can fail"
)]
mod browser {
    use super::{PickFailed, Unavailable};

    /// Nothing is stored on the server.
    pub(super) fn get() -> Result<Option<String>, Unavailable> {
        Ok(None)
    }

    /// Nothing is written on the server.
    pub(super) fn set(_text: &str) -> Result<(), Unavailable> {
        Ok(())
    }

    /// Nothing is downloaded on the server.
    pub(super) fn download(_name: &str, _text: &str) {}

    /// No file is chosen on the server.
    pub(super) fn pick_file(_on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {}

    /// No other tab on the server.
    pub(super) fn on_storage_change(_on_change: Box<dyn Fn()>) {}
}

/// The browser side, compiled with `web-store`; no native test (the
/// orchestrator's CDP probe and the user's check cover it).
#[cfg(feature = "web-store")]
mod browser {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use leptos::prelude::{on_cleanup, window_event_listener};
    use wasm_bindgen::JsCast;
    use web_sys::{
        Blob, BlobPropertyBag, FileReader, HtmlAnchorElement, HtmlInputElement, Storage, Url,
    };

    use super::{PickFailed, Picks, Unavailable};
    use crate::store::KEY;

    /// `window.localStorage`; an error when the browser has none or throws.
    fn storage() -> Result<Storage, Unavailable> {
        web_sys::window()
            .ok_or(Unavailable)?
            .local_storage()
            .ok()
            .flatten()
            .ok_or(Unavailable)
    }

    /// `localStorage[pc_learn]`; an error when the storage cannot be read.
    pub(super) fn get() -> Result<Option<String>, Unavailable> {
        storage()?.get_item(KEY).map_err(|_| Unavailable)
    }

    /// Writes `text` to `localStorage[pc_learn]`, synchronously; an error
    /// when the browser refuses (a full quota, private mode).
    pub(super) fn set(text: &str) -> Result<(), Unavailable> {
        storage()?.set_item(KEY, text).map_err(|_| Unavailable)
    }

    /// The Blob URL of `text` (`application/json`).
    fn json_url(text: &str) -> Option<String> {
        let parts = js_sys::Array::of1(&text.into());
        let options = BlobPropertyBag::new();
        options.set_type("application/json");
        let blob = Blob::new_with_str_sequence_and_options(&parts, &options).ok()?;
        Url::create_object_url_with_blob(&blob).ok()
    }

    /// Offers `text` as a file download named `name` (a Blob URL, revoked
    /// once the click is made).
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

    thread_local! {
        /// The input of the pick in progress: a browser that never fires
        /// `cancel` would leave it in the page, so the next pick removes it.
        static PICKING: RefCell<Option<HtmlInputElement>> = const { RefCell::new(None) };
    }

    /// A hidden `<input type=file>` put in the page for one choice; the one
    /// left by an earlier pick is removed first.
    fn hidden_file_input() -> Option<HtmlInputElement> {
        if let Some(stale) = PICKING.with_borrow_mut(Option::take) {
            stale.remove();
        }
        let document = web_sys::window()?.document()?;
        let input = document
            .create_element("input")
            .ok()?
            .dyn_into::<HtmlInputElement>()
            .ok()?;
        input.set_type("file");
        input.set_accept(".json,application/json");
        input.set_hidden(true);
        document.body()?.append_child(&input).ok()?;
        PICKING.with_borrow_mut(|picking| *picking = Some(input.clone()));
        Some(input)
    }

    thread_local! {
        /// Which pick is current: a stale chooser's late change is ignored.
        static PICKS: Picks = const { Picks { latest: Cell::new(0) } };
    }

    /// Opens the file chooser; the chosen file's text goes to `on_done`
    /// ([`read_file`]); the input is removed once the choice is made or
    /// cancelled (at the latest when the next pick starts, which also makes
    /// the earlier chooser inert: its change is ignored). A chooser that
    /// cannot be made, or a file that cannot be read, calls `on_done` with
    /// [`PickFailed`].
    pub(super) fn pick_file(on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {
        let pick = PICKS.with(Picks::begin);
        let Some(input) = hidden_file_input() else {
            on_done(Err(PickFailed));
            return;
        };
        let on_done = Rc::new(RefCell::new(Some(on_done)));
        let chosen = input.clone();
        let onchange = wasm_bindgen::closure::Closure::once_into_js(move || {
            let current = PICKS.with(|picks| picks.is_current(pick));
            if let Some(on_done) = on_done.borrow_mut().take().filter(|_| current) {
                read_file(&chosen, on_done);
            }
            chosen.remove();
        });
        let cancelled = input.clone();
        let oncancel = wasm_bindgen::closure::Closure::once_into_js(move || cancelled.remove());
        let _ = input.add_event_listener_with_callback("change", onchange.unchecked_ref());
        let _ = input.add_event_listener_with_callback("cancel", oncancel.unchecked_ref());
        input.click();
    }

    /// Reads the file chosen in `input` as text and calls `on_done` with it
    /// (`FileReader`'s onload); a read that fails calls it with
    /// [`PickFailed`] (onerror, or a reader that cannot be made); nothing
    /// when no file is chosen. The one-shot onload and onerror closures
    /// share the callback: whichever fires first takes it, so it runs once.
    fn read_file(
        input: &HtmlInputElement,
        on_done: impl FnOnce(Result<String, PickFailed>) + 'static,
    ) {
        let Some(file) = input.files().and_then(|files| files.item(0)) else {
            return;
        };
        let callback = Rc::new(RefCell::new(Some(on_done)));
        let on_failed = Rc::clone(&callback);
        let Ok(reader) = FileReader::new() else {
            fail(&callback);
            return;
        };
        let source = reader.clone();
        let on_loaded = Rc::clone(&callback);
        let onload = wasm_bindgen::closure::Closure::once_into_js(move || {
            let text = source.result().ok().and_then(|text| text.as_string());
            if let Some(on_done) = on_loaded.borrow_mut().take() {
                on_done(text.ok_or(PickFailed));
            }
        });
        let onerror = wasm_bindgen::closure::Closure::once_into_js(move || fail(&callback));
        reader.set_onload(Some(onload.unchecked_ref()));
        reader.set_onerror(Some(onerror.unchecked_ref()));
        if reader.read_as_text(&file).is_err() {
            fail(&on_failed);
        }
    }

    /// Calls the callback, if it is still there, with [`PickFailed`].
    fn fail<F: FnOnce(Result<String, PickFailed>)>(callback: &Rc<RefCell<Option<F>>>) {
        if let Some(on_done) = callback.borrow_mut().take() {
            on_done(Err(PickFailed));
        }
    }

    /// Calls `on_change` when another tab changes `pc_learn` (the `storage`
    /// event; other keys are ignored), for as long as the current reactive
    /// owner lives (the listener is removed in its cleanup).
    pub(super) fn on_storage_change(on_change: Box<dyn Fn()>) {
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
    use std::cell::{Cell, RefCell};

    use leptos::prelude::{GetUntracked, Owner};

    use super::*;
    use crate::backend::SaveError;
    use crate::test_support::progress_fixture;

    /// A `localStorage` the tests drive: its text, whether it reads, whether
    /// it takes a write.
    #[derive(Default)]
    struct FakeKv {
        text: RefCell<Option<String>>,
        readable: Cell<bool>,
        writable: Cell<bool>,
        /// Every write attempt, the refused ones included.
        attempts: Cell<usize>,
        /// The writes the storage took.
        writes: Cell<usize>,
    }

    impl FakeKv {
        fn working() -> Self {
            FakeKv {
                readable: Cell::new(true),
                writable: Cell::new(true),
                ..FakeKv::default()
            }
        }
    }

    impl KeyValue for &FakeKv {
        fn get(&self) -> Result<Option<String>, Unavailable> {
            if self.readable.get() {
                Ok(self.text.borrow().clone())
            } else {
                Err(Unavailable)
            }
        }

        fn set(&self, text: &str) -> Result<(), Unavailable> {
            self.attempts.set(self.attempts.get() + 1);
            if self.writable.get() {
                self.writes.set(self.writes.get() + 1);
                *self.text.borrow_mut() = Some(text.to_owned());
                Ok(())
            } else {
                Err(Unavailable)
            }
        }
    }

    /// On the server the web store reads nothing, reports `Saved` and
    /// writes nothing (it is never called there; this pins the stub).
    #[test]
    fn the_server_stub_is_an_empty_store_that_is_always_saved() {
        Owner::new().with(|| {
            let store = web_store();
            assert_eq!(store.load(), Load::Empty);
            store.save(&State::empty());
            assert_eq!(store.load(), Load::Empty);
            assert_eq!(store.status().get_untracked(), SaveStatus::Saved);
            store.on_change(Box::new(|| panic!("no other tab on the server")));
            let files = web_files();
            files.export("a.json", "{}");
            files.pick_import(Box::new(|_| panic!("no file on the server")));
        });
    }

    #[test]
    fn a_state_round_trips_and_an_empty_key_is_empty() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            let store = WebStore::new(&kv);
            assert_eq!(store.load(), Load::Empty);
            store.save(&progress_fixture());
            assert_eq!(store.load(), Load::Ok(progress_fixture()));
            assert_eq!(store.status().get_untracked(), SaveStatus::Saved);
        });
    }

    /// A storage that is absent or throws on read is not an empty progress:
    /// it is Unreadable, and the store refuses to write over it.
    #[test]
    fn a_storage_that_cannot_be_read_is_unreadable_and_never_written() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            kv.readable.set(false);
            let store = WebStore::new(&kv);
            assert!(matches!(store.load(), Load::Unreadable(_)));
            store.save(&progress_fixture());
            assert_eq!(kv.attempts.get(), 0, "nothing is even tried over it");
            assert_eq!(store.status().get_untracked(), SaveStatus::Saved);
            assert!(
                matches!(store.load(), Load::Unreadable(_)),
                "no phantom memory"
            );
        });
    }

    /// The same guard as the app's store: a newer file is never overwritten.
    #[test]
    fn a_newer_file_is_never_overwritten() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            *kv.text.borrow_mut() = Some(r#"{"version": 99}"#.to_owned());
            let store = WebStore::new(&kv);
            assert!(matches!(store.load(), Load::NewerVersion { .. }));
            store.save(&progress_fixture());
            assert_eq!(kv.attempts.get(), 0, "nothing is even tried");
            // The answer just given is not lost silently: the strip offers
            // Export of it.
            assert_eq!(
                store.status().get_untracked(),
                SaveStatus::Failed(SaveError::NewerVersion {
                    unsaved: progress_fixture()
                })
            );
            assert!(
                matches!(store.load(), Load::NewerVersion { .. }),
                "the file wins"
            );
        });
    }

    /// A write the browser refuses (a full quota, private mode) is not
    /// silent: the status says Failed(Transient), the memory keeps the
    /// state, the next write that goes through says Saved again.
    #[test]
    fn a_refused_write_is_a_transient_failure_with_the_state_kept() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            let store = WebStore::new(&kv);
            kv.writable.set(false);
            store.save(&progress_fixture());
            assert_eq!(
                store.status().get_untracked(),
                SaveStatus::Failed(SaveError::Transient)
            );
            assert_eq!(store.load(), Load::Ok(progress_fixture()), "the memory");
            assert_eq!(kv.attempts.get(), 1, "the refused attempt is counted");
            assert_eq!(kv.writes.get(), 0);
            kv.writable.set(true);
            let later = State::empty();
            store.save(&later);
            assert_eq!(store.status().get_untracked(), SaveStatus::Saved);
            assert_eq!(kv.attempts.get(), 2);
            assert_eq!(kv.writes.get(), 1);
            assert_eq!(*kv.text.borrow(), Some(later.to_json()));
            assert_eq!(store.load(), Load::Ok(later), "the file again");
        });
    }

    /// The stored value is read first: a storage that turns unreadable (or
    /// holds a newer file) after a refused write answers that, not the
    /// memory.
    #[test]
    fn the_storage_is_read_before_the_memory() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            let store = WebStore::new(&kv);
            kv.writable.set(false);
            store.save(&progress_fixture());
            assert_eq!(store.load(), Load::Ok(progress_fixture()), "the memory");
            kv.readable.set(false);
            assert!(
                matches!(store.load(), Load::Unreadable(_)),
                "unreadable wins"
            );
            kv.readable.set(true);
            *kv.text.borrow_mut() = Some(r#"{"version": 99}"#.to_owned());
            assert!(
                matches!(store.load(), Load::NewerVersion { .. }),
                "the file wins"
            );
        });
    }

    /// A second pick supersedes the first: only the latest one is current,
    /// so a stale chooser's late change is ignored.
    #[test]
    fn only_the_latest_pick_is_current() {
        let picks = Picks::default();
        let first = picks.begin();
        assert!(picks.is_current(first));
        let second = picks.begin();
        assert!(!picks.is_current(first), "superseded");
        assert!(picks.is_current(second));
        assert_ne!(first, second);
    }
}
