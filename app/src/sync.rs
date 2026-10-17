//! The sync cycle (task 050): one implementation for both hosts, over the
//! host's [`SyncTransport`] and its [`ProgressStore`](crate::backend::ProgressStore).
//!
//! A cycle refuses to start unless the store's load is `Ok` or `Empty` and
//! its status is not `Failed(NewerVersion)`: a progress this device could
//! not read, or one a newer version owns, is never synced. Then it pulls
//! the Drive copy:
//!
//! - no file: the local progress is pushed;
//! - a copy saved by a newer version: the sync stops, nothing is written
//!   ('Update the app to sync');
//! - a copy that does not parse: the sync stops, nothing is written, its
//!   raw text is kept for Export;
//! - else the local progress is read NOW (the store's memory, the saves
//!   made during the pull included) and merged with the copy
//!   ([`learn_core::merge::merge`]). When the merge changed it (compared as
//!   a set of terms: another order is no change), it is saved FIRST, the
//!   local copy before the Drive's (then the provider's changed-elsewhere
//!   handler runs, as for the store's `on_change`); a save the store
//!   refuses (a newer file, an unreadable store, a refused write) stops
//!   the cycle there, nothing pushed. Then the merge is pushed when it
//!   differs from the copy. The load, the merge and the save run in that
//!   one callback, with no wait between: no other answer can come in the
//!   middle.
//!
//! A failed pull changes nothing local. A failed push leaves the merge
//! saved locally (it holds everything both sides had): the next cycle
//! pushes it. No compare-and-set: the merge is a join, so a raced write is
//! healed at the next sync of either device.
//!
//! One cycle at a time: a request during one runs once, after it. A pull
//! or a push unanswered after [`STEP_TIMEOUT`] fails with `Network` and
//! frees the cycle. A sign-out cuts the cycle in flight: its late answers
//! are dropped. A review session holds the sync ([`SyncCycle::hold`]): a
//! cycle asked during it, or a copy that arrives during it, waits for the
//! session's end ([`SyncCycle::release`], back home), so nothing is merged
//! under the learner and no Reload banner is shown.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use learn_core::deck::TermId;
use learn_core::merge::merge;
use learn_core::progress::{Progress, State, StateError};
use leptos::prelude::*;

use crate::backend::{Backend, SaveError, SaveStatus, SharedSync, SyncAccount, SyncError};
use crate::store::Load;

/// Why this device's progress is not synced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The store could not read it.
    Unreadable,
    /// A newer version saved it, or a newer file won a write.
    Newer,
    /// It is damaged or not a progress.
    Invalid,
    /// The store could not write the merge (a refused write).
    NotSaved,
}

/// Where the sync stands in this tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    /// No cycle ran yet.
    Idle,
    /// A cycle is running.
    Syncing,
    /// The last cycle ended with both sides equal, at `at` (the local time,
    /// `HH:MM`).
    Synced {
        /// When.
        at: String,
    },
    /// The last cycle failed: 'Not synced' with the reason, and Retry.
    Failed(SyncError),
    /// This device's progress is not synced.
    Refused(Refusal),
    /// The Drive copy was saved by a newer version: never overwritten.
    DriveNewer,
    /// The Drive copy does not parse: never overwritten, its text offered
    /// to Export.
    DriveUnreadable {
        /// The copy, as is.
        raw: String,
    },
}

/// The cycle's state, shared by the callbacks of the transport.
struct Cycle {
    /// Where the copy lives.
    transport: SharedSync,
    /// The host's store.
    backend: Backend,
    /// Where the sync stands.
    status: RwSignal<SyncStatus>,
    /// A cycle is running.
    busy: Cell<bool>,
    /// A cycle was asked while one ran: it runs once after it.
    again: Cell<bool>,
    /// The current step (a pull or a push waiting for its answer): an
    /// answer or a timeout counts only while its step is current. A
    /// sign-out moves it, so the answers of a cycle it cut are dropped.
    step: Cell<u64>,
    /// A session runs: a cycle asked now waits for its end.
    held: Cell<bool>,
    /// Mounted on the client: the sync UI shows.
    mounted: RwSignal<bool>,
    /// A cycle was asked during the session.
    pending: Cell<bool>,
    /// The provider's changed-elsewhere handler.
    changed: Box<dyn Fn()>,
    /// The local time, `HH:MM`.
    clock: Box<dyn Fn() -> String>,
    /// Runs a callback once after a delay (the browser's `setTimeout`).
    timer: Box<Timer>,
}

/// Runs a callback once after a delay.
type Timer = dyn Fn(Duration, Box<dyn FnOnce()>);

/// How long a pull or a push may stay unanswered: then the cycle fails
/// with [`SyncError::Network`] and is free again.
pub(crate) const STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// The sync of the page, a `Copy` handle in the Leptos context, provided by
/// [`LearnProvider`](crate::fragment::LearnProvider) when the host gave a
/// transport.
#[derive(Clone, Copy)]
pub(crate) struct SyncCycle(StoredValue<Rc<Cycle>, LocalStorage>);

impl SyncCycle {
    /// A cycle over `transport` and `backend`; `changed` runs after a
    /// cycle saved a merge locally, `clock` tells the time of a sync,
    /// `timer` runs a callback once after a delay (the step timeout).
    pub(crate) fn new(
        transport: SharedSync,
        backend: Backend,
        changed: impl Fn() + 'static,
        clock: impl Fn() -> String + 'static,
        timer: impl Fn(Duration, Box<dyn FnOnce()>) + 'static,
    ) -> Self {
        SyncCycle(StoredValue::new_local(Rc::new(Cycle {
            transport,
            backend,
            status: RwSignal::new(SyncStatus::Idle),
            busy: Cell::new(false),
            again: Cell::new(false),
            step: Cell::new(0),
            held: Cell::new(false),
            mounted: RwSignal::new(false),
            pending: Cell::new(false),
            changed: Box::new(changed),
            clock: Box::new(clock),
            timer: Box::new(timer),
        })))
    }

    /// The sync of the nearest provider; none when the host gave none.
    pub(crate) fn current() -> Option<Self> {
        use_context::<SyncCycle>()
    }

    /// Sets where the sync stands (the render tests, which are `ssr`).
    #[cfg(all(test, feature = "ssr"))]
    pub(crate) fn set_status(self, status: SyncStatus) {
        self.0.with_value(|cycle| cycle.status.set(status));
    }

    /// Where the sync stands.
    pub(crate) fn status(self) -> Signal<SyncStatus> {
        self.0.with_value(|cycle| cycle.status.into())
    }

    /// Who is signed in.
    pub(crate) fn account(self) -> Signal<SyncAccount> {
        self.0.with_value(|cycle| cycle.transport.account())
    }

    /// Mounted on the client (the provider's Effect): the sync UI shows,
    /// and a cycle runs when a token is at hand (at load).
    pub(crate) fn mount(self) {
        self.0.with_value(|cycle| cycle.mounted.set(true));
        self.when_ready();
    }

    /// Whether the sync UI shows: mounted on the client. Tracked.
    pub(crate) fn mounted(self) -> bool {
        self.0.with_value(|cycle| cycle.mounted.get())
    }

    /// Marks the cycle mounted without a cycle (the render tests, which are
    /// `ssr`).
    #[cfg(all(test, feature = "ssr"))]
    pub(crate) fn set_mounted(self) {
        self.0.with_value(|cycle| cycle.mounted.set(true));
    }

    /// The sync UI shows where a tap may follow: the transport gets ready
    /// (the site loads Google's script), no Google call, no token asked.
    pub(crate) fn prepare(self) {
        self.0.with_value(|cycle| cycle.transport.prepare());
    }

    /// Runs a cycle now (or once after the running one).
    pub(crate) fn run(self) {
        self.0.get_value().run();
    }

    /// Runs a cycle when a token is at hand: at load and back home.
    pub(crate) fn when_ready(self) {
        if matches!(self.account().get_untracked(), SyncAccount::SignedIn { .. }) {
            self.run();
        }
    }

    /// A session started: a cycle asked from now on waits for its end.
    pub(crate) fn hold(self) {
        self.0.with_value(|cycle| cycle.held.set(true));
    }

    /// The session ended (back home): the cycle asked during it runs now;
    /// with none asked, a cycle runs when a token is at hand.
    pub(crate) fn release(self) {
        let asked = self.0.with_value(|cycle| {
            cycle.held.set(false);
            cycle.pending.replace(false)
        });
        if asked {
            self.run();
        } else {
            self.when_ready();
        }
    }

    /// Sign in, then a cycle; a failed sign-in is the status' reason.
    pub(crate) fn sign_in(self) {
        let cycle = self.0.get_value();
        let after = Rc::clone(&cycle);
        cycle
            .transport
            .sign_in(Box::new(move |signed| match signed {
                Ok(()) => after.run(),
                Err(error) => after.status.set(SyncStatus::Failed(error)),
            }));
    }

    /// 'Sync now', 'Retry' and the home's Sync chip: a cycle when a token
    /// is at hand, else a sign-in (in the gesture) then a cycle.
    pub(crate) fn sync_now(self) {
        if matches!(self.account().get_untracked(), SyncAccount::SignedIn { .. }) {
            self.run();
        } else {
            self.sign_in();
        }
    }

    /// Sign out: the transport revokes and forgets; the local progress is
    /// kept, the status back to `Idle`. A cycle in flight is cut: its step
    /// moves on (a late answer is dropped), and nothing waits after it.
    pub(crate) fn sign_out(self) {
        self.0.with_value(|cycle| {
            cycle.transport.sign_out();
            cycle.step.set(cycle.step.get() + 1);
            cycle.busy.set(false);
            cycle.again.set(false);
            cycle.pending.set(false);
            cycle.status.set(SyncStatus::Idle);
        });
    }
}

impl Cycle {
    /// Starts a cycle, unless one runs (then it runs once after) or this
    /// device's progress must not be synced.
    fn run(self: &Rc<Self>) {
        if self.held.get() {
            self.pending.set(true);
            return;
        }
        if self.busy.get() {
            self.again.set(true);
            return;
        }
        if let Err(refusal) = self.local() {
            self.status.set(SyncStatus::Refused(refusal));
            return;
        }
        self.busy.set(true);
        self.status.set(SyncStatus::Syncing);
        let step = self.begin();
        let cycle = Rc::clone(self);
        self.transport.pull(Box::new(move |pulled| {
            if cycle.take(step) {
                cycle.pulled(pulled);
            }
        }));
    }

    /// Starts a step: its number, and its timeout armed.
    fn begin(self: &Rc<Self>) -> u64 {
        let step = self.step.get() + 1;
        self.step.set(step);
        let cycle = Rc::clone(self);
        (self.timer)(
            STEP_TIMEOUT,
            Box::new(move || {
                if cycle.take(step) {
                    cycle.finish(SyncStatus::Failed(SyncError::Network));
                }
            }),
        );
        step
    }

    /// Whether `step` is still current; it is then answered (the next
    /// answer or timeout of it is dropped).
    fn take(&self, step: u64) -> bool {
        let current = self.step.get() == step;
        if current {
            self.step.set(step + 1);
        }
        current
    }

    /// This device's progress as the cycle may sync it, or why not.
    fn local(&self) -> Result<State, Refusal> {
        if matches!(
            self.backend.status().get_untracked(),
            SaveStatus::Failed(SaveError::NewerVersion { .. })
        ) {
            return Err(Refusal::Newer);
        }
        match self.backend.load() {
            Load::Ok(state) => Ok(state),
            Load::Empty => Ok(State::empty()),
            Load::Unreadable(_) => Err(Refusal::Unreadable),
            Load::NewerVersion { .. } => Err(Refusal::Newer),
            Load::Invalid { .. } => Err(Refusal::Invalid),
        }
    }

    /// Why the store did not take the save of `merged`, read from its
    /// status and its memory right after it; none when it took it.
    fn refused(&self, merged: &State) -> Option<Refusal> {
        match self.backend.status().get_untracked() {
            SaveStatus::Failed(SaveError::NewerVersion { .. }) => return Some(Refusal::Newer),
            SaveStatus::Failed(SaveError::Transient) => return Some(Refusal::NotSaved),
            SaveStatus::Saved | SaveStatus::Pending => {}
        }
        match self.backend.load() {
            Load::Ok(state) if same(&state, merged) => None,
            Load::Unreadable(_) => Some(Refusal::Unreadable),
            Load::NewerVersion { .. } => Some(Refusal::Newer),
            Load::Ok(_) | Load::Empty | Load::Invalid { .. } => Some(Refusal::NotSaved),
        }
    }

    /// The pull's answer.
    fn pulled(self: &Rc<Self>, pulled: Result<Option<String>, SyncError>) {
        let drive = match pulled {
            Err(error) => return self.finish(SyncStatus::Failed(error)),
            Ok(None) => None,
            Ok(Some(text)) => match State::from_json(&text) {
                Ok(state) => Some(state),
                Err(StateError::UnknownVersion(version)) if version > State::VERSION => {
                    return self.finish(SyncStatus::DriveNewer);
                }
                Err(_) => return self.finish(SyncStatus::DriveUnreadable { raw: text }),
            },
        };
        self.join(drive);
    }

    /// Reads the local progress NOW, merges the copy in, saves the merge
    /// when it changed the local, pushes it when it differs from the copy:
    /// all in this one call, no wait between the load and the save.
    fn join(self: &Rc<Self>, drive: Option<State>) {
        if self.held.get() {
            // A session started during the pull: nothing is merged under
            // the learner; the cycle runs again at the session's end.
            self.pending.set(true);
            self.busy.set(false);
            self.again.set(false);
            self.status.set(SyncStatus::Idle);
            return;
        }
        let local = match self.local() {
            Ok(local) => local,
            Err(refusal) => return self.finish(SyncStatus::Refused(refusal)),
        };
        let merged = drive
            .as_ref()
            .map_or_else(|| local.clone(), |drive| merge(&local, drive));
        if !same(&merged, &local) {
            self.backend.save(&merged);
            (self.changed)();
            if let Some(refusal) = self.refused(&merged) {
                return self.finish(SyncStatus::Refused(refusal));
            }
        }
        if drive.as_ref().is_some_and(|drive| same(drive, &merged)) {
            return self.finish(SyncStatus::Synced { at: (self.clock)() });
        }
        let step = self.begin();
        let cycle = Rc::clone(self);
        self.transport.push(
            &merged.to_json(),
            Box::new(move |pushed| {
                if !cycle.take(step) {
                    return;
                }
                let status = match pushed {
                    Ok(()) => SyncStatus::Synced {
                        at: (cycle.clock)(),
                    },
                    Err(error) => SyncStatus::Failed(error),
                };
                cycle.finish(status);
            }),
        );
    }

    /// Ends the cycle with `status`; runs the one asked meanwhile.
    fn finish(self: &Rc<Self>, status: SyncStatus) {
        self.status.set(status);
        self.busy.set(false);
        if self.again.replace(false) {
            self.run();
        }
    }
}

/// Whether `a` and `b` hold the same progress, whatever the order of their
/// terms: the same version and `last_today`, and the same entry for every
/// term. A copy in another order is neither saved nor pushed.
fn same(a: &State, b: &State) -> bool {
    let entries = |state: &State| -> HashMap<TermId, Progress> {
        state
            .progress
            .iter()
            .map(|entry| (entry.term.clone(), entry.clone()))
            .collect()
    };
    a.version == b.version
        && a.last_today == b.last_today
        && a.progress.len() == b.progress.len()
        && entries(a) == entries(b)
}

/// Runs `callback` once after `delay` (the browser's `setTimeout`); never
/// on the server, which never syncs.
pub(crate) fn browser_timer(delay: Duration, callback: Box<dyn FnOnce()>) {
    #[cfg(feature = "browser")]
    leptos::prelude::set_timeout(callback, delay);
    #[cfg(not(feature = "browser"))]
    let _ = (delay, callback);
}

/// The local time, `HH:MM` (the browser's clock); empty on the server,
/// which never syncs.
pub(crate) fn clock_label() -> String {
    #[cfg(feature = "browser")]
    {
        let now = js_sys::Date::new_0();
        format!("{:02}:{:02}", now.get_hours(), now.get_minutes())
    }
    #[cfg(not(feature = "browser"))]
    {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use learn_core::deck::TermId;
    use learn_core::progress::{Progress, State};
    use learn_core::stage::Stage;

    use super::*;
    use crate::backend::{
        ProgressStore, SaveError, SaveStatus, SharedSync, SyncAccount, SyncError,
    };
    use crate::test_support::{
        FakeFiles, FakeStore, FakeTransport, Outcome, backend_of, progress_fixture, today,
    };

    /// The timers a cycle armed, waiting to be fired by the test.
    type Timers = Rc<RefCell<Vec<(Duration, Box<dyn FnOnce()>)>>>;

    /// A cycle over `store` and `transport`; the count of the changed
    /// handler's calls.
    fn cycle(store: &FakeStore, transport: &FakeTransport) -> (SyncCycle, Rc<Cell<usize>>) {
        let (sync, changed, _) = timed_cycle(store, transport);
        (sync, changed)
    }

    /// [`cycle`], and the timers it arms.
    fn timed_cycle(
        store: &FakeStore,
        transport: &FakeTransport,
    ) -> (SyncCycle, Rc<Cell<usize>>, Timers) {
        let changed = Rc::new(Cell::new(0));
        let seen = Rc::clone(&changed);
        let timers: Timers = Rc::default();
        let armed = Rc::clone(&timers);
        let shared: SharedSync = Rc::new(transport.clone());
        let sync = SyncCycle::new(
            shared,
            backend_of(store, &FakeFiles::default()),
            move || seen.set(seen.get() + 1),
            || "10:42".to_owned(),
            move |delay, callback| armed.borrow_mut().push((delay, callback)),
        );
        (sync, changed, timers)
    }

    /// Fires every armed timer (they all ran out); their delays.
    fn fire(timers: &Timers) -> Vec<Duration> {
        let due: Vec<_> = timers.borrow_mut().drain(..).collect();
        due.into_iter()
            .map(|(delay, callback)| {
                callback();
                delay
            })
            .collect()
    }

    /// The fixture with one more term, `extra`, started today.
    fn with_term(mut state: State, extra: &str) -> State {
        state
            .progress
            .push(Progress::new(TermId(extra.into()), today()));
        state
    }

    /// The fixture with its first term failed once more.
    fn lapsed(mut state: State) -> State {
        state.progress[0].lapses += 1;
        state.progress[0].stage = Stage::Day;
        state
    }

    fn synced() -> SyncStatus {
        SyncStatus::Synced { at: "10:42".into() }
    }

    #[test]
    fn with_no_file_the_local_progress_is_pushed() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            assert_eq!(sync.status().get_untracked(), SyncStatus::Syncing);
            transport.settle();
            assert_eq!(transport.drive(), Some(progress_fixture().to_json()));
            assert_eq!(store.saved(), vec![], "nothing saved locally");
            assert_eq!(changed.get(), 0);
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn an_empty_device_with_no_file_pushes_an_empty_progress() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Empty);
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(transport.drive(), Some(State::empty().to_json()));
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn an_unchanged_copy_pushes_nothing_and_saves_nothing() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&progress_fixture().to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    /// The same progress in another order is the same progress: nothing
    /// is saved, the home is not told, nothing is pushed.
    /// The merge is saved before anything is pushed (the local copy
    /// first): a save the store refuses stops the cycle, nothing pushed.
    #[test]
    fn a_refused_save_stops_the_cycle_before_any_push() {
        for (outcome, refusal) in [
            (Outcome::Newer, Refusal::Newer),
            (Outcome::Unreadable, Refusal::Unreadable),
            (Outcome::Transient, Refusal::NotSaved),
        ] {
            Owner::new().with(|| {
                let local = with_term(progress_fixture(), "here 1");
                let store = FakeStore::new(Load::Ok(local));
                store.script(&[outcome]);
                let transport = FakeTransport::signed_in();
                let drive = lapsed(progress_fixture());
                transport.set_drive(Some(&drive.to_json()));
                let (sync, _) = cycle(&store, &transport);
                sync.run();
                transport.settle();
                assert_eq!(store.saved().len(), 1, "{outcome:?}: the save was tried");
                assert_eq!(transport.pushed(), Vec::<String>::new(), "{outcome:?}");
                assert_eq!(transport.drive(), Some(drive.to_json()), "{outcome:?}");
                assert_eq!(
                    sync.status().get_untracked(),
                    SyncStatus::Refused(refusal),
                    "{outcome:?}"
                );
            });
        }
    }

    #[test]
    fn a_reordered_copy_neither_saves_nor_fires_changed() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let mut reordered = progress_fixture();
            reordered.progress.reverse();
            transport.set_drive(Some(&reordered.to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn a_copy_ahead_is_saved_locally_and_not_pushed() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let ahead = lapsed(progress_fixture());
            transport.set_drive(Some(&ahead.to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(store.saved(), vec![ahead]);
            assert_eq!(changed.get(), 1, "the home reads the store again");
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn a_device_ahead_is_pushed_and_not_saved() {
        Owner::new().with(|| {
            let ahead = with_term(progress_fixture(), "new 1");
            let store = FakeStore::new(Load::Ok(ahead.clone()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&progress_fixture().to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            let drive = State::from_json(&transport.drive().unwrap()).unwrap();
            assert_eq!(drive, merge(&ahead, &progress_fixture()));
            assert!(drive.get(&TermId("new 1".into())).is_some());
        });
    }

    #[test]
    fn both_sides_changed_the_merge_is_saved_and_pushed() {
        Owner::new().with(|| {
            let local = with_term(progress_fixture(), "here 1");
            let drive = lapsed(with_term(progress_fixture(), "there 1"));
            let store = FakeStore::new(Load::Ok(local.clone()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&drive.to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            let merged = merge(&local, &drive);
            assert_eq!(store.saved(), vec![merged.clone()]);
            assert_eq!(changed.get(), 1);
            assert_eq!(transport.drive(), Some(merged.to_json()));
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn a_newer_copy_stops_the_sync_and_is_never_written() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let newer = r#"{"version": 2, "progress": []}"#;
            transport.set_drive(Some(newer));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(sync.status().get_untracked(), SyncStatus::DriveNewer);
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(transport.drive().as_deref(), Some(newer));
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
        });
    }

    #[test]
    fn a_copy_that_does_not_parse_stops_the_sync_with_its_raw_text() {
        for raw in ["{not json", r#"{"version": 0, "progress": []}"#, ""] {
            Owner::new().with(|| {
                let store = FakeStore::new(Load::Ok(progress_fixture()));
                let transport = FakeTransport::signed_in();
                transport.set_drive(Some(raw));
                let (sync, _) = cycle(&store, &transport);
                sync.run();
                transport.settle();
                assert_eq!(
                    sync.status().get_untracked(),
                    SyncStatus::DriveUnreadable { raw: raw.into() },
                    "{raw}"
                );
                assert_eq!(transport.pushed(), Vec::<String>::new(), "{raw}");
                assert_eq!(store.saved(), vec![], "{raw}");
            });
        }
    }

    #[test]
    fn a_progress_this_device_cannot_sync_is_refused_before_any_pull() {
        for (load, refusal) in [
            (Load::Unreadable("denied".into()), Refusal::Unreadable),
            (Load::NewerVersion { raw: "{}".into() }, Refusal::Newer),
            (Load::Invalid { raw: "{".into() }, Refusal::Invalid),
        ] {
            Owner::new().with(|| {
                let store = FakeStore::new(load.clone());
                let transport = FakeTransport::signed_in();
                let (sync, _) = cycle(&store, &transport);
                sync.run();
                assert_eq!(transport.pull_calls(), 0, "{load:?}");
                assert_eq!(
                    sync.status().get_untracked(),
                    SyncStatus::Refused(refusal),
                    "{load:?}"
                );
            });
        }
    }

    #[test]
    fn a_store_that_lost_a_write_to_a_newer_file_is_refused() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            store.set_status(SaveStatus::Failed(SaveError::NewerVersion {
                unsaved: progress_fixture(),
            }));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            assert_eq!(transport.pull_calls(), 0);
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Refused(Refusal::Newer)
            );
        });
    }

    /// The local progress is read again when the copy arrives: a store
    /// that turned unreadable meanwhile is refused, nothing written.
    #[test]
    fn a_store_that_turned_unreadable_during_the_pull_is_refused() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&lapsed(progress_fixture()).to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            store.changed_to(Load::Unreadable("denied".into()));
            transport.settle();
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Refused(Refusal::Unreadable)
            );
            assert_eq!(store.saved(), vec![]);
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(changed.get(), 0);
        });
    }

    /// The local progress is read when the copy arrives, never a copy
    /// taken before the pull: an answer saved meanwhile is kept.
    #[test]
    fn a_save_made_during_the_pull_is_kept() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&progress_fixture().to_json()));
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            let answered = with_term(progress_fixture(), "meanwhile 1");
            store.save(&answered);
            transport.settle();
            let drive = State::from_json(&transport.drive().unwrap()).unwrap();
            assert!(drive.get(&TermId("meanwhile 1".into())).is_some());
            assert_eq!(store.load(), Load::Ok(answered), "the local is kept");
        });
    }

    #[test]
    fn a_failed_pull_leaves_the_local_progress_untouched() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&lapsed(progress_fixture()).to_json()));
            transport.fail_next(SyncError::Network);
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Network)
            );
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            assert_eq!(transport.pushed(), Vec::<String>::new());
        });
    }

    #[test]
    fn a_failed_push_says_not_synced_and_the_next_cycle_pushes_again() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            assert!(transport.answer_pull());
            transport.fail_next(SyncError::Drive("403".into()));
            transport.settle();
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Drive("403".into()))
            );
            assert_eq!(transport.drive(), None);
            sync.run();
            transport.settle();
            assert_eq!(transport.drive(), Some(progress_fixture().to_json()));
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn one_cycle_at_a_time_and_the_requests_during_one_run_once_after() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            sync.run();
            sync.run();
            assert_eq!(transport.pull_calls(), 1, "one at a time");
            assert!(transport.answer_pull());
            assert_eq!(transport.pull_calls(), 1, "still running: the push");
            assert!(transport.answer_push());
            assert_eq!(transport.pull_calls(), 2, "the asked one, once");
            transport.settle();
            assert_eq!(transport.pull_calls(), 2, "and no more");
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    /// A sign-out during a pull: its answer, when it comes, is dropped
    /// (nothing saved, nothing pushed, the home not told) and the cycle is
    /// free at once.
    #[test]
    fn a_pull_answered_after_a_sign_out_is_dropped() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(with_term(progress_fixture(), "here 1")));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&lapsed(progress_fixture()).to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            sync.sign_out();
            assert!(transport.answer_pull());
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(sync.status().get_untracked(), SyncStatus::Idle);
            sync.run();
            assert_eq!(transport.pull_calls(), 2, "free again");
        });
    }

    /// A sign-out during a push: its answer is dropped too (no 'Synced').
    #[test]
    fn a_push_answered_after_a_sign_out_is_dropped() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            sync.run();
            assert!(transport.answer_pull());
            sync.sign_out();
            assert!(transport.answer_push());
            assert_eq!(sync.status().get_untracked(), SyncStatus::Idle);
            assert_eq!(transport.pull_calls(), 1, "the asked one is forgotten too");
        });
    }

    /// A pull that never answers fails after 30 s, frees the cycle, and
    /// its late answer is dropped.
    #[test]
    fn a_pull_that_never_answers_times_out() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _, timers) = timed_cycle(&store, &transport);
            sync.run();
            assert_eq!(fire(&timers), vec![STEP_TIMEOUT]);
            assert_eq!(STEP_TIMEOUT, Duration::from_secs(30));
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Network)
            );
            sync.run();
            assert_eq!(transport.pull_calls(), 2, "free again");
            assert!(transport.answer_pull(), "the late answer of the first");
            assert_eq!(transport.pushed(), Vec::<String>::new(), "dropped");
            assert_eq!(sync.status().get_untracked(), SyncStatus::Syncing);
        });
    }

    /// A push that never answers fails the same way; an answered step's
    /// timer, when it runs out, changes nothing.
    #[test]
    fn a_push_that_never_answers_times_out() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _, timers) = timed_cycle(&store, &transport);
            sync.run();
            assert!(transport.answer_pull());
            assert_eq!(fire(&timers).len(), 2, "the pull's and the push's");
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Network)
            );
            assert!(transport.answer_push(), "late");
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Network)
            );
        });
    }

    /// During a session a cycle waits for its end: nothing is pulled,
    /// nothing saved under the learner (no Reload banner); back home it
    /// runs once.
    #[test]
    fn a_cycle_asked_during_a_session_runs_at_its_end() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::new(SyncAccount::Remembered { email: None });
            let (sync, _) = cycle(&store, &transport);
            sync.hold();
            sync.run();
            sync.run();
            assert_eq!(transport.pull_calls(), 0, "deferred");
            sync.release();
            assert_eq!(transport.pull_calls(), 1, "once, at the session's end");
            transport.settle();
            sync.release();
            assert_eq!(transport.pull_calls(), 1, "no token at hand: nothing more");
        });
    }

    /// A session that starts while a pull is on its way: the copy is not
    /// merged into the progress under the learner; it is synced again at
    /// the session's end.
    #[test]
    fn a_pull_answered_during_a_session_is_deferred() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&lapsed(progress_fixture()).to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            sync.hold();
            assert!(transport.answer_pull());
            assert_eq!(store.saved(), vec![], "nothing saved during the session");
            assert_eq!(changed.get(), 0, "no Reload banner");
            assert_eq!(transport.pushed(), Vec::<String>::new());
            sync.release();
            assert_eq!(transport.pull_calls(), 2);
            transport.settle();
            assert_eq!(store.saved(), vec![lapsed(progress_fixture())]);
        });
    }

    /// A cycle asked during a session and a sign-out before its end: the
    /// end runs nothing.
    #[test]
    fn a_sign_out_forgets_the_deferred_cycle() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.hold();
            sync.run();
            sync.sign_out();
            sync.release();
            assert_eq!(transport.pull_calls(), 0);
        });
    }

    #[test]
    fn sync_now_signs_in_first_when_no_token_is_at_hand() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::new(SyncAccount::Remembered {
                email: Some("learner@example.org".into()),
            });
            let (sync, _) = cycle(&store, &transport);
            sync.sync_now();
            assert_eq!(transport.pull_calls(), 0);
            assert_eq!(transport.sign_ins_waiting(), 1);
            transport.answer_sign_in(Ok(()));
            assert_eq!(transport.pull_calls(), 1, "the cycle after the sign-in");
            transport.settle();
            sync.sync_now();
            assert_eq!(transport.sign_ins_waiting(), 0, "signed in: no new sign-in");
            assert_eq!(transport.pull_calls(), 2);
        });
    }

    #[test]
    fn a_failed_sign_in_is_the_reason_and_nothing_runs() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::new(SyncAccount::SignedOut);
            let (sync, _) = cycle(&store, &transport);
            sync.sign_in();
            transport.answer_sign_in(Err(SyncError::Popup));
            assert_eq!(transport.pull_calls(), 0);
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Popup)
            );
        });
    }

    #[test]
    fn a_cycle_at_load_or_back_home_runs_only_with_a_token_at_hand() {
        for (account, pulls) in [
            (SyncAccount::SignedOut, 0),
            (SyncAccount::Remembered { email: None }, 0),
            (SyncAccount::SignedIn { email: None }, 1),
        ] {
            Owner::new().with(|| {
                let store = FakeStore::new(Load::Ok(progress_fixture()));
                let transport = FakeTransport::new(account.clone());
                let (sync, _) = cycle(&store, &transport);
                sync.when_ready();
                assert_eq!(transport.pull_calls(), pulls, "{account:?}");
                assert_eq!(transport.sign_ins_waiting(), 0, "never a window unasked");
            });
        }
    }

    #[test]
    fn sign_out_tells_the_transport_and_keeps_the_local_progress() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            sync.sign_out();
            assert_eq!(transport.sign_outs(), 1);
            assert_eq!(sync.account().get_untracked(), SyncAccount::SignedOut);
            assert_eq!(sync.status().get_untracked(), SyncStatus::Idle);
            assert_eq!(store.load(), Load::Ok(progress_fixture()));
        });
    }
}
