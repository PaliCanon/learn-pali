//! What the screens' tests share: a fixed `today`, a progress fixture on
//! the embedded deck, and small readers of rendered HTML.

// The fakes serve the render tests, which need `ssr`; under the csr
// build's `--all-targets` clippy they are compiled and unused.
#![cfg_attr(
    not(feature = "ssr"),
    allow(dead_code, reason = "used by the ssr render tests only")
)]

use learn_core::date::Date;
use learn_core::deck::TermId;
use learn_core::facet::{Facet, FacetSet};
use learn_core::progress::{Progress, State};
use learn_core::session::Session;
use learn_core::stage::Stage;

use crate::deck::deck;

/// The tests' today, 5 October 2026.
pub(crate) fn today() -> Date {
    Date::parse("2026-10-05").expect("a date")
}

/// Three new terms (kho, ca 2.1, vā 1); kho moved to Week with two lapses
/// and two facets passed (due 12 October 2026); one orphan, `ghost 1`, in
/// Quarter.
pub(crate) fn progress_fixture() -> State {
    let mut session = Session::start(deck(), State::empty(), today(), 0).expect("a session");
    assert_eq!(session.request_new(3), 3);
    let mut state = session.into_state();
    let week = &mut state.progress[0];
    week.stage = Stage::Week;
    week.due = Date::from_days(today().days() + 7);
    week.lapses = 2;
    week.passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
    state.progress.push(Progress {
        term: TermId("ghost 1".into()),
        stage: Stage::Quarter,
        passed: FacetSet::empty(),
        due: today(),
        lapses: 0,
    });
    state
}

/// The byte where `needle` starts; a panic showing the HTML otherwise.
#[cfg(feature = "ssr")]
pub(crate) fn position(html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{needle}: {html}"))
}

/// The opening tag that contains `needle`.
#[cfg(feature = "ssr")]
pub(crate) fn tag_around<'a>(html: &'a str, needle: &str) -> &'a str {
    let at = position(html, needle);
    let start = html[..at].rfind('<').expect(html);
    &html[start..at + html[at..].find('>').expect(html)]
}

/// The `<button>` that ends with the label `label`, its class removed (the
/// variant's classes hold `disabled:` states of their own).
#[cfg(feature = "ssr")]
pub(crate) fn button_labelled(html: &str, label: &str) -> String {
    let end = position(html, &format!(">{label}<"));
    let tag = &html[html[..end].rfind("<button").expect(html)..end];
    let class = tag.find(r#"class=""#).expect(tag);
    let close = class + 7 + tag[class + 7..].find('"').expect(tag);
    format!("{}{}", &tag[..class], &tag[close + 1..])
}

// ---- The fake host: a store and a file transfer the tests drive ----

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use leptos::prelude::*;

use crate::backend::{
    Backend, FileTransfer, PickFailed, ProgressStore, SaveError, SaveStatus, SharedFiles,
    SharedStore,
};
use crate::store::Load;

/// What the fake store does with the next save it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The write goes through.
    Written,
    /// The write fails and may succeed later: the memory is kept, the
    /// status is `Failed(Transient)`.
    Transient,
    /// The file holds a newer version: the memory is swapped for the
    /// file's, the status is `Failed(NewerVersion { unsaved })`, then
    /// `on_change` is called, all in one synchronous run.
    Newer,
}

/// The state of a [`FakeStore`].
struct FakeInner {
    /// The memory: what `load` answers.
    load: RefCell<Load>,
    /// Every state `save` accepted, in order.
    saved: RefCell<Vec<State>>,
    /// Every state that reached the "file".
    written: RefCell<Vec<State>>,
    /// The outcomes of the next saves, then `Written`.
    script: RefCell<VecDeque<Outcome>>,
    /// The status the store reports.
    status: RwSignal<SaveStatus>,
    /// The fragment's `on_change` handlers.
    callbacks: RefCell<Vec<Box<dyn Fn()>>>,
}

/// A [`ProgressStore`] the tests drive: scripted save outcomes, the
/// store's own guard against writing over a progress it could not read.
/// Make it inside an `Owner` (its status is a signal).
#[derive(Clone)]
pub(crate) struct FakeStore(Rc<FakeInner>);

impl FakeStore {
    /// A store whose memory is `load`, status `Saved`.
    pub(crate) fn new(load: Load) -> Self {
        FakeStore(Rc::new(FakeInner {
            load: RefCell::new(load),
            saved: RefCell::default(),
            written: RefCell::default(),
            script: RefCell::default(),
            status: RwSignal::new(SaveStatus::Saved),
            callbacks: RefCell::default(),
        }))
    }

    /// The outcomes of the next saves, in order (`Written` afterwards).
    pub(crate) fn script(&self, outcomes: &[Outcome]) {
        self.0.script.borrow_mut().extend(outcomes.iter().copied());
    }

    /// Every state `save` was given.
    pub(crate) fn saved(&self) -> Vec<State> {
        self.0.saved.borrow().clone()
    }

    /// Every state that reached the "file".
    pub(crate) fn written(&self) -> Vec<State> {
        self.0.written.borrow().clone()
    }

    /// Sets the status the store reports.
    pub(crate) fn set_status(&self, status: SaveStatus) {
        self.0.status.set(status);
    }

    /// The status, now.
    pub(crate) fn status_now(&self) -> SaveStatus {
        self.0.status.get_untracked()
    }

    /// The memory changed outside a save (another tab): `load` answers
    /// `load`, then the handlers run.
    pub(crate) fn changed_to(&self, load: Load) {
        *self.0.load.borrow_mut() = load;
        self.changed_outside();
    }

    /// Calls the fragment's `on_change` handlers.
    pub(crate) fn changed_outside(&self) {
        for callback in self.0.callbacks.borrow().iter() {
            callback();
        }
    }

    /// The write at exit or page hide: the latest memory goes to the
    /// "file", unless the load was `Unreadable` or a newer file won.
    pub(crate) fn exit_write(&self) {
        let memory = self.0.load.borrow().clone();
        if let Load::Ok(state) = memory {
            self.0.written.borrow_mut().push(state);
        }
    }
}

impl ProgressStore for FakeStore {
    fn load(&self) -> Load {
        self.0.load.borrow().clone()
    }

    fn save(&self, state: &State) {
        self.0.saved.borrow_mut().push(state.clone());
        // The store's own guard: nothing is written over a progress that
        // could not be read, or over a newer file.
        if matches!(
            *self.0.load.borrow(),
            Load::Unreadable(_) | Load::NewerVersion { .. }
        ) {
            return;
        }
        *self.0.load.borrow_mut() = Load::Ok(state.clone());
        match self.0.script.borrow_mut().pop_front() {
            None | Some(Outcome::Written) => {
                self.0.written.borrow_mut().push(state.clone());
                self.0.status.set(SaveStatus::Saved);
            }
            Some(Outcome::Transient) => {
                self.0.status.set(SaveStatus::Failed(SaveError::Transient));
            }
            Some(Outcome::Newer) => {
                *self.0.load.borrow_mut() = Load::NewerVersion {
                    raw: r#"{"version": 2}"#.into(),
                };
                self.0
                    .status
                    .set(SaveStatus::Failed(SaveError::NewerVersion {
                        unsaved: state.clone(),
                    }));
                self.changed_outside();
            }
        }
    }

    fn status(&self) -> Signal<SaveStatus> {
        self.0.status.into()
    }

    fn on_change(&self, callback: Box<dyn Fn()>) {
        self.0.callbacks.borrow_mut().push(callback);
    }
}

/// A `pick_import` callback waiting for the learner's file.
type Pick = Box<dyn FnOnce(Result<String, PickFailed>)>;

/// What the fake file transfer saw.
#[derive(Default)]
struct FilesInner {
    /// Every `(name, text)` exported.
    exported: RefCell<Vec<(String, String)>>,
    /// The `pick_import` callbacks waiting for a file.
    picks: RefCell<Vec<Pick>>,
    /// How many times a file was asked for.
    asked: RefCell<usize>,
}

/// A [`FileTransfer`] the tests drive: it records the exports, and
/// [`FakeFiles::choose`] plays the learner choosing a file.
#[derive(Clone, Default)]
pub(crate) struct FakeFiles(Rc<FilesInner>);

impl FakeFiles {
    /// Every `(name, text)` exported.
    pub(crate) fn exported(&self) -> Vec<(String, String)> {
        self.0.exported.borrow().clone()
    }

    /// How many times a file was asked for.
    pub(crate) fn picks(&self) -> usize {
        *self.0.asked.borrow()
    }

    /// The learner cancels the chooser: nothing is called back.
    pub(crate) fn cancel(&self) {
        self.0.picks.borrow_mut().clear();
    }

    /// The chooser cannot be made, or the chosen file cannot be read: the
    /// host calls back with [`PickFailed`].
    pub(crate) fn unreadable(&self) {
        self.deliver(Err(PickFailed));
    }

    /// Calls the waiting callbacks back with `result`.
    fn deliver(&self, result: Result<String, PickFailed>) {
        let pending: Vec<_> = self.0.picks.borrow_mut().drain(..).collect();
        for on_done in pending {
            on_done(result.clone());
        }
    }

    /// The learner chooses a file holding `text`.
    pub(crate) fn choose(&self, text: &str) {
        self.deliver(Ok(text.to_owned()));
    }
}

impl FileTransfer for FakeFiles {
    fn export(&self, name: &str, text: &str) {
        self.0
            .exported
            .borrow_mut()
            .push((name.to_owned(), text.to_owned()));
    }

    fn pick_import(&self, on_done: Box<dyn FnOnce(Result<String, PickFailed>)>) {
        *self.0.asked.borrow_mut() += 1;
        self.0.picks.borrow_mut().push(on_done);
    }
}

/// A [`Backend`] over `store` and `files`.
pub(crate) fn backend_of(store: &FakeStore, files: &FakeFiles) -> Backend {
    let (store, files): (SharedStore, SharedFiles) =
        (Rc::new(store.clone()), Rc::new(files.clone()));
    Backend::new(store, files)
}

/// Provides a [`Backend`] over fresh fakes whose memory is `load`, in the
/// current owner, as `LearnProvider` does; returns the fakes.
pub(crate) fn provide_fakes(load: Load) -> (FakeStore, FakeFiles) {
    let (store, files) = (FakeStore::new(load), FakeFiles::default());
    provide_context(backend_of(&store, &files));
    provide_context(crate::components::data_block::ProgressData::new());
    (store, files)
}
