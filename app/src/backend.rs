//! What a host plugs into the Learn fragment: where the progress lives
//! ([`ProgressStore`]) and how a file moves in and out ([`FileTransfer`]).
//! Both are required props of [`LearnProvider`](crate::fragment::LearnProvider):
//! there is no default and no "missing store" state. A host may also plug
//! where the progress is copied between devices ([`SyncTransport`], task
//! 050): an optional prop, and without one there is no sync UI.
//!
//! The site plugs [`web_store`](crate::web_store) and
//! [`web_files`](crate::web_files) (`localStorage`, a Blob download, a file
//! input); the PaliCanon app plugs its own, backed by Tauri commands.
//!
//! The fragment reads them through a copyable handle it puts in the Leptos
//! context (the traits are not `Send`: a host's store may hold `Rc`s).

use std::rc::Rc;

use learn_core::progress::State;
use leptos::prelude::*;

use crate::store::Load;

/// Whether the progress is on disk, as a store reports it
/// ([`ProgressStore::status`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveStatus {
    /// Everything accepted by [`ProgressStore::save`] is written.
    Saved,
    /// A write is in flight or queued; nothing is wrong.
    Pending,
    /// The progress is not safely written. The fragment shows one strip,
    /// 'Your progress is not saved', that offers Export.
    Failed(SaveError),
}

/// Why a save failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// A write failed and may succeed later: the store keeps the progress
    /// in memory and retries. The strip offers Export of the current state.
    Transient,
    /// The file holds a newer version and wins: the store stops writing.
    /// `unsaved` is the LATEST memory at the moment the conflict was
    /// received, the saves coalesced behind the refused one included, so
    /// nothing the learner answered is lost silently; the strip offers
    /// Export of it. The store then answers [`Load::NewerVersion`] and
    /// calls [`ProgressStore::on_change`].
    NewerVersion {
        /// The progress the file did not take.
        unsaved: State,
    },
}

/// Where the learner's progress lives. The fragment calls it from the UI
/// thread only; it answers at once from the implementation's own memory.
pub trait ProgressStore {
    /// The progress, read now from the implementation's memory (never a
    /// wait). [`Load::Unreadable`] when its read failed with the stored
    /// progress untouched.
    fn load(&self) -> Load;

    /// Accepts `state` as the new progress, at once: it returns nothing and
    /// promises nothing about durability (see [`ProgressStore::status`]).
    /// The implementation owns its queue and its page-hide hook. It refuses
    /// the write, and the exit write too, while its [`load`](Self::load)
    /// answers [`Load::Unreadable`], and after a [`SaveError::NewerVersion`]
    /// conflict (its load then answers [`Load::NewerVersion`]): the guard is
    /// the store's, not the fragment's buttons'.
    fn save(&self, state: &State);

    /// Whether what [`save`](Self::save) accepted is written. The same
    /// signal every time it is called; the fragment reads it reactively.
    fn status(&self) -> Signal<SaveStatus>;

    /// Registers `callback`, to be called when the memory changed OUTSIDE a
    /// [`save`](Self::save): another tab wrote the progress, or a write
    /// conflict swapped the memory for the file's. It carries no payload:
    /// the fragment's handler calls [`load`](Self::load) and
    /// [`status`](Self::status) again. Called under the fragment's reactive
    /// owner: a listener should be removed with `on_cleanup`.
    fn on_change(&self, callback: Box<dyn Fn()>);
}

/// How a file moves between the learner and the host (the browser's
/// download and file input, a share sheet, a dialog). Both are
/// fire-and-forget: a failure is shown by the implementation.
pub trait FileTransfer {
    /// Offers `text` as a file named `name` (the export, or the raw copy of
    /// a progress this build cannot read).
    fn export(&self, name: &str, text: &str);

    /// Lets the learner choose a file and calls `on_done` once: with its
    /// text, or with [`PickFailed`] when the chooser could not be made or
    /// the file could not be read (a failure of its own, shown with its own
    /// reason; not an empty text, which is a file that was read and is
    /// refused as not a progress). A cancelled choice calls nothing. A pick
    /// that a later `pick_import` supersedes calls nothing.
    fn pick_import(&self, on_done: Box<dyn FnOnce(Result<String, PickFailed>)>);
}

/// The chooser of a file could not be made, or the chosen file could not be
/// read (see [`FileTransfer::pick_import`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickFailed;

/// Who is signed in to the sync, as a [`SyncTransport`] reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAccount {
    /// Nobody: no sync, nothing changes from a device without one.
    SignedOut,
    /// Signed in before, with no token at hand (the site after a reload:
    /// the token lived in the tab's memory only). A tap asks Google for a
    /// new one in its gesture ([`SyncTransport::sign_in`]), then syncs.
    Remembered {
        /// The account's email, kept as a login hint; none when Google did
        /// not give it.
        email: Option<String>,
    },
    /// A token at hand: a sync can run now.
    SignedIn {
        /// The account's email; none when Google did not give it.
        email: Option<String>,
    },
}

/// Why a call of a [`SyncTransport`] failed. Nothing local changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// No token, or Google refused it (expired, revoked): sign in again.
    SignedOut,
    /// The sign-in was closed or refused by the learner.
    Cancelled,
    /// The sign-in window could not open (a blocked popup): tap again.
    Popup,
    /// Google could not be reached (offline, blocked).
    Network,
    /// Google answered with an error: what it said, for the reason line.
    Drive(String),
    /// The device's own secure storage failed (the app's Keychain denied
    /// or locked): worded as local storage, never as a Google Drive
    /// refusal. Retry may succeed once the storage is available.
    Storage,
}

/// Where the progress is copied between devices: the learner's Google
/// Drive on both hosts (the site's transport is `web_sync()`, the
/// app's over its Rust commands). Called from the UI thread only, like the
/// store; every call answers through its callback, once (a pull and a push
/// cross the network). The transport keeps the file's identity between a
/// pull and the push after it, and logs the Drive file id it resolved.
pub trait SyncTransport {
    /// Signs in: asks for a token (called in the learner's gesture: the
    /// site opens Google's window from it), then calls `on_done` once.
    /// Signed in, [`account`](Self::account) answers
    /// [`SyncAccount::SignedIn`].
    fn sign_in(&self, on_done: Box<dyn FnOnce(Result<(), SyncError>)>);

    /// Signs out: revokes the token at Google (best effort), then forgets
    /// it and the account. The local progress is kept.
    fn sign_out(&self);

    /// Who is signed in. The same signal every time it is called; it may
    /// change after the page is mounted (the site reads its remembered
    /// account after hydration).
    fn account(&self) -> Signal<SyncAccount>;

    /// Reads the progress file: `Ok(None)` when there is none yet, else its
    /// text, as is.
    fn pull(&self, on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>);

    /// Writes `text` as the progress file (creating it when the last pull
    /// found none).
    fn push(&self, text: &str, on_done: Box<dyn FnOnce(Result<(), SyncError>)>);

    /// Gets ready for a sign-in that may follow (the sync UI is shown): the
    /// site loads Google's script now, so the tap that follows opens
    /// Google's window inside its gesture. No Google call, no token asked.
    /// Nothing by default.
    fn prepare(&self) {}

    /// Re-reads the account against the clock (task 059): a transport
    /// whose access can run out on its own (the site's token, valid about
    /// an hour) turns [`SyncAccount::SignedIn`] into
    /// [`SyncAccount::Remembered`] here when the access is gone, with no
    /// request. Called at each automatic ask and at the home status line's
    /// minute refresh. Nothing by default (the app never pauses).
    fn refresh(&self) {}
}

/// A shared [`SyncTransport`].
pub type SharedSync = Rc<dyn SyncTransport>;

/// Where the host remembers that the 'sync on several devices' hint was
/// answered (task 059): shown once after the first finished session, then
/// remembered when the learner dismissed it or signed in. An optional prop
/// of [`LearnProvider`](crate::fragment::LearnProvider): the site plugs
/// `localStorage[pc_learn_hint]`; a host with none keeps the flag in memory
/// for the tab (the hint then comes back after a reload).
pub trait HintMemory {
    /// Whether the hint was already answered.
    fn seen(&self) -> bool;

    /// Remembers it. False when the write failed: the caller then keeps a
    /// flag in memory for this tab.
    fn remember(&self) -> bool;
}

/// A shared [`HintMemory`].
pub type SharedHint = Rc<dyn HintMemory>;

/// The host's [`HintMemory`], as a `Copy` handle in the Leptos context
/// (provided only when the host gave one).
#[derive(Clone, Copy)]
pub(crate) struct Hint(StoredValue<SharedHint, LocalStorage>);

impl Hint {
    /// A handle over `memory`.
    pub(crate) fn new(memory: SharedHint) -> Self {
        Hint(StoredValue::new_local(memory))
    }

    /// The handle the nearest provider gave; none when the host gave no
    /// memory.
    pub(crate) fn current() -> Option<Self> {
        use_context::<Hint>()
    }

    /// [`HintMemory::seen`].
    pub(crate) fn seen(self) -> bool {
        self.0.get_value().seen()
    }

    /// [`HintMemory::remember`].
    pub(crate) fn remember(self) -> bool {
        self.0.get_value().remember()
    }
}

/// A shared [`ProgressStore`].
pub type SharedStore = Rc<dyn ProgressStore>;
/// A shared [`FileTransfer`].
pub type SharedFiles = Rc<dyn FileTransfer>;

/// The store and the file transfer the fragment was given, as a `Copy`
/// handle in the Leptos context (the values stay in the thread that made
/// them; the fragment runs there only).
#[derive(Clone, Copy)]
pub(crate) struct Backend {
    /// The progress store.
    store: StoredValue<SharedStore, LocalStorage>,
    /// The file transfer.
    files: StoredValue<SharedFiles, LocalStorage>,
    /// Called after each [`save`](Self::save) (not after
    /// [`save_merge`](Self::save_merge)): the sync's 'a change was saved'
    /// ask.
    saved: StoredValue<Option<Rc<dyn Fn()>>, LocalStorage>,
}

impl Backend {
    /// A handle over `store` and `files`.
    pub(crate) fn new(store: SharedStore, files: SharedFiles) -> Self {
        Backend {
            store: StoredValue::new_local(store),
            files: StoredValue::new_local(files),
            saved: StoredValue::new_local(None),
        }
    }

    /// The handle the nearest [`LearnProvider`](crate::fragment::LearnProvider)
    /// provided.
    ///
    /// # Panics
    /// When there is none: the fragment's screens need a provider above them.
    #[track_caller]
    pub(crate) fn current() -> Self {
        use_context::<Backend>().expect("Learn needs a LearnProvider above it")
    }

    /// [`ProgressStore::load`].
    pub(crate) fn load(self) -> Load {
        self.store.get_value().load()
    }

    /// [`ProgressStore::save`], a change the learner made: the listener
    /// of [`on_saved`](Self::on_saved) runs after it. Any new save path
    /// asks the sync by default.
    pub(crate) fn save(self, state: &State) {
        self.save_merge(state);
        if let Some(saved) = self.saved.get_value() {
            saved();
        }
    }

    /// [`ProgressStore::save`], the sync cycle's own merge: the cycle
    /// carries the origin 'sync' and never asks itself for another cycle
    /// (two devices cannot ping-pong).
    pub(crate) fn save_merge(self, state: &State) {
        self.store.get_value().save(state);
    }

    /// Registers the listener of [`save`](Self::save): one per backend,
    /// the latest wins.
    pub(crate) fn on_saved(self, callback: impl Fn() + 'static) {
        self.saved.set_value(Some(Rc::new(callback)));
    }

    /// [`ProgressStore::status`].
    pub(crate) fn status(self) -> Signal<SaveStatus> {
        self.store.get_value().status()
    }

    /// [`ProgressStore::on_change`].
    pub(crate) fn on_change(self, callback: impl Fn() + 'static) {
        self.store.get_value().on_change(Box::new(callback));
    }

    /// [`FileTransfer::export`].
    pub(crate) fn export(self, name: &str, text: &str) {
        self.files.get_value().export(name, text);
    }

    /// [`FileTransfer::pick_import`].
    pub(crate) fn pick_import(self, on_done: impl FnOnce(Result<String, PickFailed>) + 'static) {
        self.files.get_value().pick_import(Box::new(on_done));
    }
}

#[cfg(test)]
mod tests {
    use learn_core::progress::State;
    use leptos::prelude::{GetUntracked, Owner};

    use super::*;
    use crate::test_support::{FakeFiles, FakeStore, backend_of};

    /// The handle forwards every call to the store and the transfer it was
    /// given, and returns the store's own answers.
    #[test]
    fn the_handle_forwards_to_the_store_and_the_transfer() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Empty);
            let files = FakeFiles::default();
            let backend = backend_of(&store, &files);
            assert_eq!(backend.load(), Load::Empty);
            let state = State::empty();
            backend.save(&state);
            assert_eq!(store.saved(), vec![state]);
            assert_eq!(backend.status().get_untracked(), SaveStatus::Saved);
            backend.export("a.json", "{}");
            assert_eq!(files.exported(), vec![("a.json".into(), "{}".into())]);
            backend.pick_import(|_| ());
            assert_eq!(files.picks(), 1);
        });
    }

    #[test]
    fn the_stores_change_callback_reaches_the_fragments_handler() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Empty);
            let backend = backend_of(&store, &FakeFiles::default());
            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
            let seen = std::rc::Rc::clone(&calls);
            backend.on_change(move || seen.set(seen.get() + 1));
            store.changed_outside();
            assert_eq!(calls.get(), 1);
        });
    }

    /// Origin rule (design): a learner's save asks the sync, the cycle's
    /// own merge save does not, and the store's changed-elsewhere event
    /// does not either.
    #[test]
    fn a_save_asks_and_a_merge_save_and_a_change_elsewhere_do_not() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Empty);
            let backend = backend_of(&store, &FakeFiles::default());
            let asked = std::rc::Rc::new(std::cell::Cell::new(0));
            let seen = std::rc::Rc::clone(&asked);
            backend.on_saved(move || seen.set(seen.get() + 1));
            backend.save(&State::empty());
            assert_eq!(asked.get(), 1, "a save asks");
            backend.save_merge(&State::empty());
            assert_eq!(asked.get(), 1, "the merge save never asks");
            assert_eq!(store.saved().len(), 2, "both reached the store");
            store.changed_outside();
            assert_eq!(asked.get(), 1, "a change elsewhere never asks");
        });
    }

    #[test]
    fn a_save_with_no_listener_just_saves() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Empty);
            let backend = backend_of(&store, &FakeFiles::default());
            backend.save(&State::empty());
            assert_eq!(store.saved().len(), 1);
        });
    }

    #[test]
    fn the_hint_handle_forwards_to_the_hosts_memory() {
        use std::cell::Cell;
        struct Memory(Cell<bool>);
        impl HintMemory for Memory {
            fn seen(&self) -> bool {
                self.0.get()
            }
            fn remember(&self) -> bool {
                self.0.set(true);
                true
            }
        }
        Owner::new().with(|| {
            assert!(Hint::current().is_none(), "none until a host gave one");
            let hint = Hint::new(std::rc::Rc::new(Memory(Cell::new(false))));
            assert!(!hint.seen());
            assert!(hint.remember());
            assert!(hint.seen());
        });
    }

    #[test]
    #[should_panic(expected = "Learn needs a LearnProvider above it")]
    fn a_screen_without_a_provider_says_what_is_missing() {
        Owner::new().with(|| {
            let _ = Backend::current();
        });
    }
}
