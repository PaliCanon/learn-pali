//! The site's host side: the progress in the browser's
//! `localStorage[pc_learn]` ([`web_store`]), the selected deck beside it in
//! `localStorage[pc_learn_deck]` (a device setting), and the files through
//! a Blob download and a file input ([`web_files`]). Today's behaviour,
//! behind the two traits of [`crate::backend`].
//!
//! Compiled for the site only (`ssr`, where it is a no-op that the server
//! never calls: the progress is read in an Effect after hydration; and
//! `web-store`, which `hydrate` enables). The app builds with `csr` alone
//! and plugs its own: nothing of this module is in its build.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::prelude::*;

use crate::backend::{
    FileTransfer, HintMemory, PickFailed, ProgressStore, SaveError, SaveStatus, SharedFiles,
    SharedHint, SharedStore,
};
use crate::store::{Load, classify};
use learn_core::book::{Book, DeckId};

/// The progress store of the web site: `localStorage[pc_learn]` holds the
/// Book, read and written synchronously; the selected deck is
/// `localStorage[pc_learn_deck]`. Its status is `Saved` unless the browser
/// refuses a write (a full quota, private mode): then `Failed(Transient)`,
/// the Book kept in memory until a write goes through. A storage that is absent or
/// throws on read is [`Load::Unreadable`], and nothing is written over it,
/// nor over a file saved by a newer version. It reports a change when
/// another tab writes the key (the `storage` event).
#[must_use]
pub fn web_store() -> SharedStore {
    Rc::new(WebStore::with_deck_storage(BrowserKv, BrowserStorage))
}

/// The file transfer of the web site: the export is a Blob URL download,
/// the import a hidden file input.
#[must_use]
pub fn web_files() -> SharedFiles {
    Rc::new(WebFiles)
}

/// Where the site remembers that the 'sync on several devices' hint was
/// answered (task 059): `localStorage[pc_learn_hint]` holds `1`. When the
/// browser has no storage or refuses the write, `remember` says so and the
/// hint keeps a flag in memory for the tab. On the server nothing is read
/// or written (it is never called there).
#[must_use]
pub fn web_hint() -> SharedHint {
    Rc::new(WebHint::new(BrowserStorage))
}

/// The key of the selected deck in `localStorage`: a device setting, not
/// part of the progress file and never synced.
pub(crate) const DECK_KEY: &str = "pc_learn_deck";

/// The key of the hint's answer in `localStorage`.
pub(crate) const HINT_KEY: &str = "pc_learn_hint";

/// What is stored under [`HINT_KEY`] once the hint was answered.
const HINT_ANSWERED: &str = "1";

/// The browser's storage as [`WebHint`] sees it: any key, so a test can
/// check which keys the hint touches (only its own).
trait Storage {
    /// The text under `key`; `Ok(None)` when the key is not there.
    fn get_item(&self, key: &str) -> Result<Option<String>, Unavailable>;

    /// Stores `text` under `key`.
    fn set_item(&self, key: &str, text: &str) -> Result<(), Unavailable>;
}

/// `window.localStorage` (nothing, on the server).
struct BrowserStorage;

impl Storage for BrowserStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, Unavailable> {
        browser::get_item(key)
    }

    fn set_item(&self, key: &str, text: &str) -> Result<(), Unavailable> {
        browser::set_item(key, text)
    }
}

/// [`web_hint`]'s implementation over a [`Storage`]: it reads and writes
/// [`HINT_KEY`] and no other key (`pc_learn`, the progress, is never
/// touched).
struct WebHint<S> {
    /// Where the answer lives.
    storage: S,
}

impl<S: Storage> WebHint<S> {
    /// A hint memory over `storage`.
    fn new(storage: S) -> Self {
        WebHint { storage }
    }
}

impl<S: Storage> HintMemory for WebHint<S> {
    /// The key holds `1`; anything else, an absent key or a storage that
    /// throws on read, is not an answer.
    fn seen(&self) -> bool {
        matches!(
            self.storage.get_item(HINT_KEY),
            Ok(Some(text)) if text == HINT_ANSWERED
        )
    }

    /// Writes `1` under the hint key; false when the browser has no
    /// storage or refuses the write (the hint then keeps a flag in memory
    /// for the tab).
    fn remember(&self) -> bool {
        self.storage.set_item(HINT_KEY, HINT_ANSWERED).is_ok()
    }
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

/// [`web_store`]'s implementation over a [`KeyValue`] (the Book) and a
/// [`Storage`] (the selected deck's key).
struct WebStore<K, D = BrowserStorage> {
    /// Where the Book lives.
    kv: K,
    /// Where the selected deck's id lives: [`DECK_KEY`] and no other key.
    device: D,
    /// The latest Book a write did not take: what `load` answers while it
    /// is set, so a refused write loses nothing.
    unsaved: RefCell<Option<Book>>,
    /// `Saved`, or `Failed(Transient)` while `unsaved` is set.
    status: RwSignal<SaveStatus>,
}

#[cfg(test)]
impl<K: KeyValue> WebStore<K> {
    /// A store over `kv`, status `Saved`, its device setting in the
    /// browser's storage.
    fn new(kv: K) -> Self {
        WebStore::with_deck_storage(kv, BrowserStorage)
    }
}

impl<K: KeyValue, D: Storage> WebStore<K, D> {
    /// A store over `kv`, status `Saved`, remembering the selected deck in
    /// `device`.
    fn with_deck_storage(kv: K, device: D) -> Self {
        WebStore {
            kv,
            device,
            unsaved: RefCell::new(None),
            status: RwSignal::new(SaveStatus::Saved),
        }
    }

    /// What the storage holds, whatever the memory says.
    fn stored(&self) -> Load<Book> {
        match self.kv.get() {
            Ok(raw) => classify(raw.as_deref()),
            Err(Unavailable) => Load::Unreadable("localStorage is not available".to_owned()),
        }
    }
}

impl<K: KeyValue, D: Storage> ProgressStore for WebStore<K, D> {
    /// The storage is read first: what it cannot give (unreadable) or must
    /// keep (a newer file) is the answer, whatever the memory holds; else
    /// the memory of a refused write, else the storage.
    fn load(&self) -> Load<Book> {
        let stored = self.stored();
        if matches!(stored, Load::Unreadable(_) | Load::NewerVersion { .. }) {
            return stored;
        }
        match self.unsaved.borrow().as_ref() {
            Some(book) => Load::Ok(book.clone()),
            None => stored,
        }
    }

    /// Nothing is written over a storage that could not be read, or over a
    /// file saved by a newer version: that refusal says
    /// `Failed(NewerVersion { unsaved })`, so the strip offers the answer
    /// just given. A write the browser refuses keeps the
    /// Book in memory and says `Failed(Transient)`; the next save tries
    /// again.
    fn save(&self, book: &Book) {
        match self.stored() {
            Load::Unreadable(_) => return,
            Load::NewerVersion { .. } => {
                // The file wins; the answer just given is offered for Export.
                self.status.set(SaveStatus::Failed(SaveError::NewerVersion {
                    unsaved: book.clone(),
                }));
                return;
            }
            Load::Ok(_) | Load::Empty | Load::Invalid { .. } => {}
        }
        if self.kv.set(&book.to_json()).is_ok() {
            *self.unsaved.borrow_mut() = None;
            self.status.set(SaveStatus::Saved);
        } else {
            *self.unsaved.borrow_mut() = Some(book.clone());
            self.status.set(SaveStatus::Failed(SaveError::Transient));
        }
    }

    fn status(&self) -> Signal<SaveStatus> {
        self.status.into()
    }

    /// Only `pc_learn` notifies: the selected deck is a device setting that
    /// other tabs do not hear about.
    fn on_change(&self, callback: Box<dyn Fn()>) {
        browser::on_storage_change(callback);
    }

    /// The id under [`DECK_KEY`]; none when the key is absent or the
    /// storage cannot be read.
    fn selected(&self) -> Option<DeckId> {
        match self.device.get_item(DECK_KEY) {
            Ok(Some(id)) if !id.is_empty() => Some(DeckId(id)),
            Ok(_) | Err(Unavailable) => None,
        }
    }

    /// Writes the id under [`DECK_KEY`]; a refused write is kept in the
    /// page's memory by the backend, so the selection holds until reload.
    fn select(&self, deck: &DeckId) {
        let _ = self.device.set_item(DECK_KEY, &deck.0);
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

    /// Nothing is stored on the server.
    pub(super) fn get_item(_key: &str) -> Result<Option<String>, Unavailable> {
        Ok(None)
    }

    /// Nothing can be written on the server: the write failed.
    pub(super) fn set_item(_key: &str, _text: &str) -> Result<(), Unavailable> {
        Err(Unavailable)
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

    /// `localStorage[key]`; an error when the storage cannot be read.
    pub(super) fn get_item(key: &str) -> Result<Option<String>, Unavailable> {
        storage()?.get_item(key).map_err(|_| Unavailable)
    }

    /// Writes `text` to `localStorage[key]`; an error when the browser
    /// refuses (a full quota, private mode).
    pub(super) fn set_item(key: &str, text: &str) -> Result<(), Unavailable> {
        storage()?.set_item(key, text).map_err(|_| Unavailable)
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

    /// The fixture progress as the Book of a learner who follows the words
    /// deck alone.
    fn book() -> Book {
        Book::from_state(progress_fixture())
    }

    /// The fixture's Book with a started term in the deck `zz` too.
    fn two_decks() -> Book {
        let mut other = progress_fixture();
        other.progress[0].term = learn_core::deck::TermId("zz-1".into());
        let mut book = book();
        book.set(DeckId("zz".into()), other);
        book
    }

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
            store.save(&Book::empty());
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
            store.save(&book());
            assert_eq!(store.load(), Load::Ok(book()));
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
            store.save(&book());
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
            store.save(&book());
            assert_eq!(kv.attempts.get(), 0, "nothing is even tried");
            // The answer just given is not lost silently: the strip offers
            // Export of it.
            assert_eq!(
                store.status().get_untracked(),
                SaveStatus::Failed(SaveError::NewerVersion { unsaved: book() })
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
            store.save(&book());
            assert_eq!(
                store.status().get_untracked(),
                SaveStatus::Failed(SaveError::Transient)
            );
            assert_eq!(store.load(), Load::Ok(book()), "the memory");
            assert_eq!(kv.attempts.get(), 1, "the refused attempt is counted");
            assert_eq!(kv.writes.get(), 0);
            kv.writable.set(true);
            let later = Book::empty();
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
            store.save(&book());
            assert_eq!(store.load(), Load::Ok(book()), "the memory");
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

    /// A Book of two decks round-trips whole, and a flat learner's file is
    /// exactly today's `State` text.
    #[test]
    fn a_book_with_two_decks_round_trips_whole() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            let store = WebStore::new(&kv);
            store.save(&two_decks());
            assert_eq!(store.load(), Load::Ok(two_decks()));
            let text = kv.text.borrow().clone().unwrap();
            assert!(text.contains("\"version\": 3"), "{text}");
            store.save(&book());
            assert_eq!(*kv.text.borrow(), Some(progress_fixture().to_json()));
        });
    }

    /// A refused write keeps the whole Book in memory, the other decks
    /// included.
    #[test]
    fn a_refused_write_keeps_the_whole_book() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            let store = WebStore::new(&kv);
            kv.writable.set(false);
            store.save(&two_decks());
            assert_eq!(store.load(), Load::Ok(two_decks()));
            assert_eq!(kv.writes.get(), 0);
        });
    }

    /// The selection is a device setting: `pc_learn_deck`, never the Book's
    /// key, and nothing of the Book is written by selecting.
    #[test]
    fn the_selection_lives_under_its_own_key() {
        Owner::new().with(|| {
            assert_eq!(DECK_KEY, "pc_learn_deck");
            assert_ne!(DECK_KEY, crate::store::KEY, "not the Book's key");
            let kv = FakeKv::working();
            let device = FakeStorage::working();
            let store = WebStore::with_deck_storage(&kv, &device);
            assert_eq!(store.selected(), None, "nothing chosen yet");
            store.select(&DeckId("zz".into()));
            assert_eq!(store.selected(), Some(DeckId("zz".into())));
            assert_eq!(
                device
                    .items
                    .borrow()
                    .get("pc_learn_deck")
                    .map(String::as_str),
                Some("zz")
            );
            assert_eq!(kv.attempts.get(), 0, "the Book is not written");
            let touched = device.touched.borrow();
            assert!(
                touched.iter().all(|key| key == "pc_learn_deck"),
                "{touched:?}"
            );
        });
    }

    /// A storage that throws on read or refuses the write is no selection,
    /// and no panic.
    #[test]
    fn an_unreadable_or_refusing_device_storage_is_no_selection() {
        Owner::new().with(|| {
            let kv = FakeKv::working();
            let device = FakeStorage::working();
            device.put("pc_learn_deck", "zz");
            let store = WebStore::with_deck_storage(&kv, &device);
            device.readable.set(false);
            assert_eq!(store.selected(), None);
            device.readable.set(true);
            device.put("pc_learn_deck", "");
            assert_eq!(store.selected(), None, "an empty id is none");
            device.writable.set(false);
            store.select(&DeckId::words());
            assert_eq!(store.selected(), None);
        });
    }

    /// A `localStorage` that holds any key, the tests drive: it records
    /// every key it is asked about.
    #[derive(Default)]
    struct FakeStorage {
        items: RefCell<std::collections::HashMap<String, String>>,
        readable: Cell<bool>,
        writable: Cell<bool>,
        /// Every key a read or a write named, in order.
        touched: RefCell<Vec<String>>,
    }

    impl FakeStorage {
        fn working() -> Self {
            FakeStorage {
                readable: Cell::new(true),
                writable: Cell::new(true),
                ..FakeStorage::default()
            }
        }

        fn put(&self, key: &str, text: &str) {
            self.items.borrow_mut().insert(key.into(), text.into());
        }
    }

    impl Storage for &FakeStorage {
        fn get_item(&self, key: &str) -> Result<Option<String>, Unavailable> {
            self.touched.borrow_mut().push(key.to_owned());
            if self.readable.get() {
                Ok(self.items.borrow().get(key).cloned())
            } else {
                Err(Unavailable)
            }
        }

        fn set_item(&self, key: &str, text: &str) -> Result<(), Unavailable> {
            self.touched.borrow_mut().push(key.to_owned());
            if self.writable.get() {
                self.put(key, text);
                Ok(())
            } else {
                Err(Unavailable)
            }
        }
    }

    /// '`pc_learn_hint` in localStorage': the key is that name, spelled out
    /// here so a typo in the constant fails (it also fails the key rows
    /// below, which compare with this literal).
    #[test]
    fn the_hint_key_is_pc_learn_hint() {
        assert_eq!(HINT_KEY, "pc_learn_hint");
    }

    /// An unanswered hint is not seen; remembering writes `1` under the
    /// hint key and reads back as seen.
    #[test]
    fn the_hint_remembers_one_and_reads_it_back() {
        let storage = FakeStorage::working();
        let hint = WebHint::new(&storage);
        assert!(!hint.seen(), "nothing stored yet");
        assert!(hint.remember(), "the write went through");
        assert_eq!(
            storage
                .items
                .borrow()
                .get("pc_learn_hint")
                .map(String::as_str),
            Some("1")
        );
        assert!(hint.seen());
    }

    /// Only the value `1` means answered.
    #[test]
    fn only_one_means_the_hint_was_answered() {
        let storage = FakeStorage::working();
        let hint = WebHint::new(&storage);
        for other in ["0", "", "true", "11"] {
            storage.put("pc_learn_hint", other);
            assert!(!hint.seen(), "{other:?} is not an answer");
        }
        storage.put("pc_learn_hint", "1");
        assert!(hint.seen());
    }

    /// A storage that refuses the write: `remember` says false (the caller
    /// keeps a flag in memory for the tab), nothing is stored, and the hint
    /// is not seen. No panic.
    #[test]
    fn a_refused_write_is_remembered_as_false() {
        let storage = FakeStorage::working();
        storage.writable.set(false);
        let hint = WebHint::new(&storage);
        assert!(!hint.remember());
        assert!(!hint.seen());
        assert_eq!(storage.items.borrow().len(), 0, "nothing stored");
    }

    /// A storage that throws on read: not seen, no panic, and `remember`
    /// still tries its write.
    #[test]
    fn an_unreadable_storage_is_not_seen() {
        let storage = FakeStorage::working();
        storage.put("pc_learn_hint", "1");
        storage.readable.set(false);
        let hint = WebHint::new(&storage);
        assert!(!hint.seen());
    }

    /// The hint never reads or writes another key: `pc_learn`, the
    /// progress, keeps its text, and the only key named is the hint's.
    #[test]
    fn the_hint_touches_no_other_key() {
        let storage = FakeStorage::working();
        storage.put("pc_learn", "{\"progress\": true}");
        let hint = WebHint::new(&storage);
        assert!(!hint.seen());
        assert!(hint.remember());
        assert!(hint.seen());
        let touched = storage.touched.borrow();
        assert!(touched.len() >= 3, "it did read and write: {touched:?}");
        assert!(
            touched.iter().all(|key| key == "pc_learn_hint"),
            "{touched:?}"
        );
        assert_eq!(
            storage.items.borrow().get("pc_learn").map(String::as_str),
            Some("{\"progress\": true}")
        );
    }

    /// On the server the hint memory reads nothing and cannot write: seen
    /// false, remember false (it is never called there; this pins the stub).
    #[test]
    fn the_server_stub_has_not_seen_the_hint_and_cannot_remember_it() {
        let hint = web_hint();
        assert!(!hint.seen());
        assert!(!hint.remember());
        assert!(!hint.seen());
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
