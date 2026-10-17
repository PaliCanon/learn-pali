//! What the screens' tests share: a fixed `today`, a progress fixture on
//! the embedded deck, and small readers of rendered HTML.

// The fakes serve the render tests, which need `ssr`; under the csr
// build's `--all-targets` clippy they are compiled and unused.
#![cfg_attr(
    not(feature = "ssr"),
    allow(dead_code, reason = "used by the ssr render tests only")
)]

use learn_core::date::Date;
use learn_core::deck::{Meaning, Pos, Term, TermId};
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

/// A word the learner added from a passage: `sānuvajja`, two facets
/// (Meaning, Recall), at the rank an added word has.
pub(crate) fn added_word() -> Term {
    Term {
        id: TermId("sānuvajja".into()),
        lemma: "sānuvajja".into(),
        pos: Pos("adj".into()),
        meaning: Meaning {
            en: "blameworthy".into(),
            fr: None,
            vi: None,
            literal: None,
        },
        construction: None,
        root: None,
        example: None,
        forms: Vec::new(),
        rank: u32::MAX,
    }
}

/// A state holding [`added_word`], started at `stage` and due on `due`.
pub(crate) fn added_started(stage: Stage, due: Date) -> State {
    let mut held = State::empty();
    held.added.push(added_word());
    held.progress.push(Progress {
        term: added_word().id,
        stage,
        passed: FacetSet::empty(),
        due,
        lapses: 0,
    });
    held
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
    /// The store can no longer read what it holds: the write is refused,
    /// the load answers `Unreadable` (the status stays as it was).
    Unreadable,
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
            Some(Outcome::Unreadable) => {
                *self.0.load.borrow_mut() = Load::Unreadable("denied".into());
            }
            Some(Outcome::Newer) => {
                *self.0.load.borrow_mut() = Load::NewerVersion {
                    raw: r#"{"version": 3}"#.into(),
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

// ---- The fake sync transport (task 050) ----

use std::cell::Cell;

use crate::backend::{SyncAccount, SyncError, SyncTransport};

/// A `pull` waiting for its answer.
type PullDone = Box<dyn FnOnce(Result<Option<String>, SyncError>)>;
/// A `push` or a `sign_in` waiting for its answer.
type Done = Box<dyn FnOnce(Result<(), SyncError>)>;

/// The state of a [`FakeTransport`].
struct TransportInner {
    /// The Drive file's text; none: no file.
    drive: RefCell<Option<String>>,
    /// Who is signed in.
    account: RwSignal<SyncAccount>,
    /// The pulls waiting for [`FakeTransport::answer_pull`].
    pulls: RefCell<VecDeque<PullDone>>,
    /// The pushes waiting for [`FakeTransport::answer_push`], with their text.
    pushes: RefCell<VecDeque<(String, Done)>>,
    /// The sign-ins waiting for [`FakeTransport::answer_sign_in`].
    sign_ins: RefCell<VecDeque<Done>>,
    /// Every text a push was given, answered or not.
    pushed: RefCell<Vec<String>>,
    /// How many pulls were asked.
    pull_calls: Cell<usize>,
    /// How many sign-outs were asked.
    sign_outs: Cell<usize>,
    /// The failure the next answered pull or push gives instead.
    failure: RefCell<Option<SyncError>>,
}

/// A [`SyncTransport`] the tests drive: every call waits until the test
/// answers it, so a test can act between a pull and its answer. Make it
/// inside an `Owner` (its account is a signal).
#[derive(Clone)]
pub(crate) struct FakeTransport(Rc<TransportInner>);

impl FakeTransport {
    /// A transport for `account`, with no file on the Drive.
    pub(crate) fn new(account: SyncAccount) -> Self {
        FakeTransport(Rc::new(TransportInner {
            drive: RefCell::new(None),
            account: RwSignal::new(account),
            pulls: RefCell::default(),
            pushes: RefCell::default(),
            sign_ins: RefCell::default(),
            pushed: RefCell::default(),
            pull_calls: Cell::new(0),
            sign_outs: Cell::new(0),
            failure: RefCell::new(None),
        }))
    }

    /// A signed-in transport (`learner@example.org`).
    pub(crate) fn signed_in() -> Self {
        FakeTransport::new(SyncAccount::SignedIn {
            email: Some("learner@example.org".into()),
        })
    }

    /// Puts `text` on the Drive (none: no file).
    pub(crate) fn set_drive(&self, text: Option<&str>) {
        *self.0.drive.borrow_mut() = text.map(str::to_owned);
    }

    /// The Drive file's text.
    pub(crate) fn drive(&self) -> Option<String> {
        self.0.drive.borrow().clone()
    }

    /// The next answered pull or push fails with `error`.
    pub(crate) fn fail_next(&self, error: SyncError) {
        *self.0.failure.borrow_mut() = Some(error);
    }

    /// Answers the oldest waiting pull with the Drive's text (or the
    /// scripted failure); false when none waits.
    pub(crate) fn answer_pull(&self) -> bool {
        let Some(done) = self.0.pulls.borrow_mut().pop_front() else {
            return false;
        };
        match self.0.failure.borrow_mut().take() {
            Some(error) => done(Err(error)),
            None => done(Ok(self.drive())),
        }
        true
    }

    /// Answers the oldest waiting push: its text reaches the Drive (or the
    /// scripted failure, the Drive unchanged); false when none waits.
    pub(crate) fn answer_push(&self) -> bool {
        let Some((text, done)) = self.0.pushes.borrow_mut().pop_front() else {
            return false;
        };
        let failure = self.0.failure.borrow_mut().take();
        if let Some(error) = failure {
            done(Err(error));
        } else {
            *self.0.drive.borrow_mut() = Some(text);
            done(Ok(()));
        }
        true
    }

    /// Answers every waiting pull and push, and those they start.
    pub(crate) fn settle(&self) {
        while self.answer_pull() || self.answer_push() {}
    }

    /// Answers the oldest waiting sign-in: signed in on success.
    pub(crate) fn answer_sign_in(&self, result: Result<(), SyncError>) {
        let done = self
            .0
            .sign_ins
            .borrow_mut()
            .pop_front()
            .expect("a sign-in waits");
        if result.is_ok() {
            self.0.account.set(SyncAccount::SignedIn {
                email: Some("learner@example.org".into()),
            });
        }
        done(result);
    }

    /// Every text a push was given.
    pub(crate) fn pushed(&self) -> Vec<String> {
        self.0.pushed.borrow().clone()
    }

    /// How many pulls were asked.
    pub(crate) fn pull_calls(&self) -> usize {
        self.0.pull_calls.get()
    }

    /// How many sign-ins wait.
    pub(crate) fn sign_ins_waiting(&self) -> usize {
        self.0.sign_ins.borrow().len()
    }

    /// How many sign-outs were asked.
    pub(crate) fn sign_outs(&self) -> usize {
        self.0.sign_outs.get()
    }
}

impl SyncTransport for FakeTransport {
    fn sign_in(&self, on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        self.0.sign_ins.borrow_mut().push_back(on_done);
    }

    fn sign_out(&self) {
        self.0.sign_outs.set(self.0.sign_outs.get() + 1);
        self.0.account.set(SyncAccount::SignedOut);
    }

    fn account(&self) -> Signal<SyncAccount> {
        self.0.account.into()
    }

    fn pull(&self, on_done: Box<dyn FnOnce(Result<Option<String>, SyncError>)>) {
        self.0.pull_calls.set(self.0.pull_calls.get() + 1);
        self.0.pulls.borrow_mut().push_back(on_done);
    }

    fn push(&self, text: &str, on_done: Box<dyn FnOnce(Result<(), SyncError>)>) {
        self.0.pushed.borrow_mut().push(text.to_owned());
        self.0
            .pushes
            .borrow_mut()
            .push_back((text.to_owned(), on_done));
    }
}
