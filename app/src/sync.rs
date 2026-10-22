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
//! The gate (task 059, [`gate`]): every automatic ask ([`SyncCycle::ask`],
//! with its [`Reason`]) passes one coalescing gate, and the
//! [`Pacer`] decides WHEN a cycle starts.
//!
//! - **Ignored**: when the account is not signed in (signed out, or
//!   remembered with no valid token: paused, the web token's expiry is
//!   checked at each ask), and after a terminal state (refused, a newer or
//!   an unreadable Drive copy) until a gesture or a reload.
//! - **Offline** (`navigator.onLine` false): the status is 'Offline', not a
//!   failure; no timer is armed, the backoff is not touched; the online
//!   event resets the backoff and asks.
//! - **Held** by a session: nothing is armed; one ask is remembered and
//!   asked again at [`SyncCycle::release`] (back home).
//! - **Else** the pacer: one trailing timer, however many asks; a cycle in
//!   flight sets 'again' (one more after it, after the gap, or the backoff
//!   after a failure, never at once); a saved change waits for the debounce.
//!   When the timer runs out the gates are applied again.
//! - **Gestures** ([`SyncCycle::run`]: Sync now, Retry, Resume, sign-in)
//!   bypass the gap and the terminal state: they cancel the armed timer,
//!   reset the backoff and start at once (offline: 'Offline' at once, no
//!   cycle).
//!
//! One cycle at a time. A pull or a push unanswered after [`STEP_TIMEOUT`]
//! fails with `Network` and frees the cycle. A sign-out cuts the cycle in
//! flight: its late answers are dropped, its timers are stale, its backoff
//! and flags are cleared. A review session holds the sync
//! ([`SyncCycle::hold`]): a copy that arrives during it waits for the
//! session's end too, so nothing is merged under the learner and no Reload
//! banner is shown.
//!
//! The age ('2 min ago') and the count ('N terms from another device') of
//! the home's status line live in memory only: the wall-clock stamp of the
//! last cycle that ended with both sides equal, and the number of terms the
//! last merge that changed the local progress took from the Drive.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use learn_core::book::{Book, DeckId, merge_books};
use learn_core::deck::{Term, TermId};
use learn_core::progress::{Progress, State, StateError};
use leptos::prelude::*;

use crate::backend::{Backend, SaveError, SaveStatus, SharedSync, SyncAccount, SyncError};
use crate::store::Load;
use crate::sync_pacing::{Outcome, Pacer, Reason};

mod gate;

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
    /// The browser is offline: not a failure, no cycle starts, the backoff
    /// is not armed; the online event asks again.
    Offline,
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
    /// The pacing of the automatic cycles: the spacing, the backoff, the
    /// one trailing timer and the cycle in flight.
    pacer: RefCell<Pacer>,
    /// The last cycle ended in a terminal state (refused, a newer or an
    /// unreadable Drive copy): no automatic cycle until a gesture or a reload.
    terminal: Cell<bool>,
    /// The monotonic clock, milliseconds (the gap and the backoff).
    monotonic: RefCell<Box<dyn Fn() -> f64>>,
    /// The wall clock, milliseconds since the epoch (the status line's age).
    wall: RefCell<Box<dyn Fn() -> f64>>,
    /// Whether the browser says it is online.
    online: RefCell<Box<dyn Fn() -> bool>>,
    /// The current step (a pull or a push waiting for its answer): an
    /// answer or a timeout counts only while its step is current. A
    /// sign-out moves it, so the answers of a cycle it cut are dropped.
    step: Cell<u64>,
    /// A session runs: a cycle asked now waits for its end.
    held: Cell<bool>,
    /// Mounted on the client: the sync UI shows.
    mounted: RwSignal<bool>,
    /// The wall-clock time (milliseconds since the epoch) the last cycle
    /// ended in this tab with both sides equal; memory only, never written
    /// to the progress.
    synced_at: RwSignal<Option<f64>>,
    /// How many terms came from another device in the last cycle that
    /// merged any; memory only.
    from_other: RwSignal<usize>,
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
            pacer: RefCell::new(Pacer::new()),
            terminal: Cell::new(false),
            monotonic: RefCell::new(Box::new(monotonic_now_ms)),
            wall: RefCell::new(Box::new(wall_now_ms)),
            online: RefCell::new(Box::new(browser_online)),
            step: Cell::new(0),
            held: Cell::new(false),
            mounted: RwSignal::new(false),
            synced_at: RwSignal::new(None),
            from_other: RwSignal::new(0),
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

    /// Sets the clocks (the gate's tests): the monotonic one (the gap and
    /// the backoff) and the wall one (the status line's age).
    #[cfg(test)]
    pub(crate) fn set_clocks(
        self,
        monotonic: impl Fn() -> f64 + 'static,
        wall: impl Fn() -> f64 + 'static,
    ) {
        self.0.with_value(|cycle| {
            *cycle.monotonic.borrow_mut() = Box::new(monotonic);
            *cycle.wall.borrow_mut() = Box::new(wall);
        });
    }

    /// Sets the online probe (the gate's tests).
    #[cfg(test)]
    pub(crate) fn set_online(self, online: impl Fn() -> bool + 'static) {
        self.0
            .with_value(|cycle| *cycle.online.borrow_mut() = Box::new(online));
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

    /// An automatic ask (task 059): the one coalescing gate, see the module
    /// doc. Logs its reason to the console (diagnosis only).
    pub(crate) fn ask(self, reason: Reason) {
        self.0.get_value().ask(reason);
    }

    /// When the last cycle ended with both sides equal in this tab, as
    /// wall-clock milliseconds since the epoch; none before. Tracked.
    pub(crate) fn synced_at(self) -> Signal<Option<f64>> {
        self.0.with_value(|cycle| cycle.synced_at.into())
    }

    /// How many terms came from another device in the last cycle that
    /// merged any (kept until the next such cycle); 0 before. Tracked.
    pub(crate) fn terms_from_other(self) -> Signal<usize> {
        self.0.with_value(|cycle| cycle.from_other.into())
    }

    /// The home's 'Resume' (task 059): a gesture. With a token at hand a
    /// cycle, else a sign-in in the gesture then a cycle: the same as
    /// [`sync_now`](Self::sync_now).
    pub(crate) fn resume(self) {
        self.sync_now();
    }

    /// Re-reads the account against the clock (the web token's expiry):
    /// [`SyncTransport::refresh`](crate::backend::SyncTransport::refresh).
    /// The gate does the same at each ask and each timer. The status line's
    /// minute tick calls it (browser builds only).
    #[cfg(any(feature = "browser", test))]
    pub(crate) fn check_account(self) {
        self.0.with_value(|cycle| cycle.transport.refresh());
    }

    /// Sets when the last cycle ended and how many terms came from another
    /// device (the render tests, which are `ssr`).
    #[cfg(all(test, feature = "ssr"))]
    pub(crate) fn set_synced(self, at: Option<f64>, from_other: usize) {
        self.0.with_value(|cycle| {
            cycle.synced_at.set(at);
            cycle.from_other.set(from_other);
        });
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

    /// A gesture (Sync now, Retry, Resume, sign-in): a cycle now, bypassing
    /// the gap, the backoff and a terminal state (once after the running
    /// one), when a token is at hand after the account is refreshed; with
    /// none it asks nothing and touches nothing.
    pub(crate) fn run(self) {
        self.0.get_value().run();
    }

    /// Asks a cycle at open, when a token is at hand.
    pub(crate) fn when_ready(self) {
        self.ask(Reason::Visible);
    }

    /// A session started: a cycle asked from now on waits for its end.
    pub(crate) fn hold(self) {
        self.0.with_value(|cycle| cycle.held.set(true));
    }

    /// The session ended (back home): the cycle asked during it is asked
    /// again; with none asked, nothing is.
    pub(crate) fn release(self) {
        let asked = self.0.with_value(|cycle| {
            cycle.held.set(false);
            cycle.pending.replace(false)
        });
        if asked {
            self.ask(Reason::Home);
        }
    }

    /// Sign in, then a cycle; a failed sign-in is the status' reason, a
    /// closed window leaves the status as it was.
    pub(crate) fn sign_in(self) {
        let cycle = self.0.get_value();
        let after = Rc::clone(&cycle);
        cycle
            .transport
            .sign_in(Box::new(move |signed| match signed {
                Ok(()) => after.run(),
                Err(SyncError::Cancelled) => {}
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
    /// moves on (a late answer is dropped); the pacer forgets its timer, its
    /// backoff and its flags; the terminal state, the stamp and the count
    /// are cleared.
    pub(crate) fn sign_out(self) {
        self.0.with_value(|cycle| {
            cycle.transport.sign_out();
            cycle.step.set(cycle.step.get() + 1);
            cycle.pacer.borrow_mut().reset();
            cycle.terminal.set(false);
            cycle.pending.set(false);
            cycle.synced_at.set(None);
            cycle.from_other.set(0);
            cycle.status.set(SyncStatus::Idle);
        });
    }
}

impl Cycle {
    /// Starts a cycle: the pull. The pacer has said so (`Act::Start`).
    fn start(self: &Rc<Self>) {
        if let Err(refusal) = self.local() {
            return self.finish(SyncStatus::Refused(refusal));
        }
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
    fn local(&self) -> Result<Book, Refusal> {
        if matches!(
            self.backend.status().get_untracked(),
            SaveStatus::Failed(SaveError::NewerVersion { .. })
        ) {
            return Err(Refusal::Newer);
        }
        match self.backend.load_book() {
            Load::Ok(book) => Ok(book),
            Load::Empty => Ok(Book::empty()),
            Load::Unreadable(_) => Err(Refusal::Unreadable),
            Load::NewerVersion { .. } => Err(Refusal::Newer),
            Load::Invalid { .. } => Err(Refusal::Invalid),
        }
    }

    /// Why the store did not take the save of `merged`, read from its
    /// status and its memory right after it; none when it took it.
    fn refused(&self, merged: &Book) -> Option<Refusal> {
        match self.backend.status().get_untracked() {
            SaveStatus::Failed(SaveError::NewerVersion { .. }) => return Some(Refusal::Newer),
            SaveStatus::Failed(SaveError::Transient) => return Some(Refusal::NotSaved),
            SaveStatus::Saved | SaveStatus::Pending => {}
        }
        match self.backend.load_book() {
            Load::Ok(book) if same(&book, merged) => None,
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
            Ok(Some(text)) => match Book::from_json(&text) {
                Ok(mut book) => {
                    book.shadow_added(&DeckId::words(), crate::deck::deck());
                    Some(book)
                }
                Err(StateError::UnknownVersion(version)) if version > Book::VERSION => {
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
    fn join(self: &Rc<Self>, drive: Option<Book>) {
        // The one boundary of a session: the push below sends this call's
        // local snapshot, so a hold landing after the check is harmless (its
        // progress is merged at the next cycle, nothing is saved under it).
        if self.held.get() {
            return self.defer();
        }
        let local = match self.local() {
            Ok(local) => local,
            Err(refusal) => return self.finish(SyncStatus::Refused(refusal)),
        };
        let merged = drive
            .as_ref()
            .map_or_else(|| local.clone(), |drive| merge_books(&local, drive));
        if !same(&merged, &local)
            && let Some(refusal) = self.save_merge(&local, &merged)
        {
            return self.finish(SyncStatus::Refused(refusal));
        }
        if drive.as_ref().is_some_and(|drive| same(drive, &merged)) {
            return self.finish(SyncStatus::Synced { at: (self.clock)() });
        }
        self.push(&merged);
    }

    /// A session started during the pull: nothing is merged under the
    /// learner; the cycle ends (the pacer is told, or it would stay in
    /// flight for ever) as `Deferred`, which did no I/O and so leaves the
    /// ladder and the last end as they were, and is asked again at the
    /// session's end.
    fn defer(self: &Rc<Self>) {
        self.pending.set(true);
        self.status.set(SyncStatus::Idle);
        let act = self
            .pacer
            .borrow_mut()
            .finished(Outcome::Deferred, self.now());
        self.apply(act);
    }

    /// Saves the merge locally, tells the home, counts what came from the
    /// Drive; why the store did not take it, if it did not.
    fn save_merge(&self, local: &Book, merged: &Book) -> Option<Refusal> {
        self.backend.save_merge_book(merged);
        (self.changed)();
        if let Some(refusal) = self.refused(merged) {
            return Some(refusal);
        }
        let count = terms_changed(local, merged);
        if count > 0 {
            self.from_other.set(count);
        }
        None
    }

    /// Pushes the merge; the push's answer ends the cycle.
    fn push(self: &Rc<Self>, merged: &Book) {
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
}

/// Whether `a` and `b` hold the same progress: the same deck ids and the
/// same entry ([`same_state`]) for each. A deck one side lacks is no empty
/// entry (only `words`, which a Book always holds, is the invariant): the
/// side that has it is ahead.
fn same(a: &Book, b: &Book) -> bool {
    a.decks().count() == b.decks().count()
        && a.decks()
            .all(|(id, state)| b.get(id).is_some_and(|other| same_state(state, other)))
}

/// Whether `a` and `b` hold the same progress, whatever the order of their
/// terms and of their added words: the same `last_today`, the same entry
/// for every term and the same added words. The version stamp is how a
/// file was written (1 while no word was added), not progress: a copy in
/// another order or of another stamp is neither saved nor pushed.
fn same_state(a: &State, b: &State) -> bool {
    a.last_today == b.last_today
        && a.progress.len() == b.progress.len()
        && a.added.len() == b.added.len()
        && entries_of(a) == entries_of(b)
        && words_of(a) == words_of(b)
}

/// The entries of `state` by term.
fn entries_of(state: &State) -> HashMap<TermId, Progress> {
    state
        .progress
        .iter()
        .map(|entry| (entry.term.clone(), entry.clone()))
        .collect()
}

/// The added words of `state` by id.
fn words_of(state: &State) -> HashMap<TermId, Term> {
    state
        .added
        .iter()
        .map(|term| (term.id.clone(), term.clone()))
        .collect()
}

/// How many terms of `merged` are absent from `local` or differ from it,
/// summed over the decks: a deck `local` lacks counts all its terms.
fn terms_changed(local: &Book, merged: &Book) -> usize {
    merged
        .decks()
        .map(|(id, state)| deck_terms_changed(&local.state(id), state))
        .sum()
}

/// [`terms_changed`] of one deck: the entries by term and the added words
/// by id, compared as [`same_state`] compares them (another order is no
/// change). A word and its progress entry are one term.
fn deck_terms_changed(local: &State, merged: &State) -> usize {
    let (entries, words) = (entries_of(local), words_of(local));
    let mut changed: HashSet<&TermId> = HashSet::new();
    for entry in &merged.progress {
        if entries.get(&entry.term) != Some(entry) {
            changed.insert(&entry.term);
        }
    }
    for word in &merged.added {
        if words.get(&word.id) != Some(word) {
            changed.insert(&word.id);
        }
    }
    changed.len()
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

/// The monotonic time, milliseconds (the browser's `performance.now()`;
/// the time since the first call on the server, which never syncs, and in
/// the native tests). Never goes back: the gap and the backoff read it.
pub(crate) fn monotonic_now_ms() -> f64 {
    #[cfg(feature = "browser")]
    {
        web_sys::window()
            .and_then(|window| window.performance())
            .map_or(0.0, |performance| performance.now())
    }
    #[cfg(not(feature = "browser"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0
    }
}

/// Whether the browser says it is online (`navigator.onLine`); always
/// true off the browser.
pub(crate) fn browser_online() -> bool {
    #[cfg(feature = "browser")]
    {
        web_sys::window().is_none_or(|window| window.navigator().on_line())
    }
    #[cfg(not(feature = "browser"))]
    {
        true
    }
}

/// The wall-clock time, milliseconds since the epoch (the browser's
/// `Date.now()`; the system clock on the server, which never syncs). The
/// home status line's age is read from it: a night's sleep reads '8 h ago'.
pub(crate) fn wall_now_ms() -> f64 {
    #[cfg(feature = "browser")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(feature = "browser"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use learn_core::deck::TermId;
    use learn_core::merge::merge;
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
    pub(super) type Timers = Rc<RefCell<Vec<(Duration, Box<dyn FnOnce()>)>>>;

    /// A cycle over `store` and `transport`; the count of the changed
    /// handler's calls.
    fn cycle(store: &FakeStore, transport: &FakeTransport) -> (SyncCycle, Rc<Cell<usize>>) {
        let (sync, changed, _) = timed_cycle(store, transport);
        (sync, changed)
    }

    /// [`cycle`], and the timers it arms.
    pub(super) fn timed_cycle(
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
        sync.set_clocks(test_now, wall_now_ms);
        (sync, changed, timers)
    }

    thread_local! {
        /// The tests' monotonic clock, milliseconds: [`fire`] moves it by the
        /// longest delay it fires, so a timer that fires has really waited its
        /// delay (the pacer checks the gap again when a timer runs out).
        static TEST_NOW: Cell<f64> = const { Cell::new(1_000_000.0) };
    }

    /// The tests' monotonic clock (see [`fire`]).
    pub(super) fn test_now() -> f64 {
        TEST_NOW.with(Cell::get)
    }

    /// Fires every armed timer (they all ran out, after the longest delay
    /// on the tests' clock); their delays.
    pub(super) fn fire(timers: &Timers) -> Vec<Duration> {
        let due: Vec<_> = timers.borrow_mut().drain(..).collect();
        let longest = due
            .iter()
            .map(|(delay, _)| *delay)
            .max()
            .unwrap_or_default();
        TEST_NOW.with(|now| now.set(now.get() + longest.as_secs_f64() * 1000.0));
        due.into_iter()
            .map(|(delay, callback)| {
                callback();
                delay
            })
            .collect()
    }

    /// The fixture with one more term, `extra`, started today.
    pub(super) fn with_term(mut state: State, extra: &str) -> State {
        state
            .progress
            .push(Progress::new(TermId(extra.into()), today()));
        state
    }

    /// The fixture with its first term failed once more.
    pub(super) fn lapsed(mut state: State) -> State {
        state.progress[0].lapses += 1;
        state.progress[0].stage = Stage::Day;
        state
    }

    pub(super) fn synced() -> SyncStatus {
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

    /// The fixture holding the added word `sānuvajja`.
    pub(super) fn with_word(mut state: State) -> State {
        state.added.push(crate::test_support::added_word());
        state
    }

    #[test]
    fn an_added_word_on_this_device_alone_is_pushed() {
        Owner::new().with(|| {
            let local = with_word(progress_fixture());
            let store = FakeStore::new(Load::Ok(local.clone()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&progress_fixture().to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(store.saved(), vec![], "nothing new locally");
            assert_eq!(changed.get(), 0);
            assert_eq!(transport.pushed(), vec![local.to_json()]);
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    /// A word added while a pull is pending is in the merge the answer
    /// triggers: it is saved with the Drive's terms and pushed, never lost
    /// to a stale copy of the progress.
    #[test]
    fn an_add_while_a_pull_is_pending_keeps_the_word() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let drive = with_term(State::empty(), "ghost 2");
            transport.set_drive(Some(&drive.to_json()));
            let (sync, _) = cycle(&store, &transport);
            sync.run();
            let backend = backend_of(&store, &FakeFiles::default());
            crate::learn::adding::add_word(backend, crate::test_support::added_word()).unwrap();
            transport.settle();
            let Load::Ok(local) = backend.load() else {
                panic!("a progress");
            };
            assert_eq!(local.added, vec![crate::test_support::added_word()]);
            assert!(
                local.get(&TermId("kho".into())).is_some(),
                "and the device's"
            );
            assert!(
                local.get(&TermId("ghost 2".into())).is_some(),
                "and the Drive's"
            );
            let pushed = transport.pushed();
            assert_eq!(pushed.len(), 1);
            assert!(pushed[0].contains("sānuvajja"), "{}", pushed[0]);
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn an_added_word_on_the_drive_alone_is_saved_locally_and_not_pushed() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let ahead = with_word(progress_fixture());
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
    fn the_same_words_in_another_order_are_the_same_progress() {
        Owner::new().with(|| {
            let mut local = with_word(progress_fixture());
            let mut other = crate::test_support::added_word();
            other.id = TermId("abhiññā 1".into());
            local.added.push(other);
            let store = FakeStore::new(Load::Ok(local.clone()));
            let transport = FakeTransport::signed_in();
            let mut reordered = local;
            reordered.added.reverse();
            transport.set_drive(Some(&reordered.to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            assert_eq!(transport.pushed(), Vec::<String>::new());
        });
    }

    #[test]
    fn the_version_stamp_is_not_part_of_the_progress() {
        // A file of version 1 with no word reads as the empty state.
        let one: State = State::from_json(r#"{"version":1,"progress":[]}"#).unwrap();
        assert!(same_state(&one, &State::empty()));
        let stamped = State {
            version: 1,
            ..progress_fixture()
        };
        assert!(same_state(&stamped, &progress_fixture()));
        // The content still decides.
        assert!(!same_state(
            &with_word(progress_fixture()),
            &progress_fixture()
        ));
        assert!(!same_state(
            &lapsed(progress_fixture()),
            &progress_fixture()
        ));
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

    /// Origin rule (design): the cycle's own merge save carries origin
    /// sync and never asks for another cycle, or two devices would
    /// ping-pong; a save the learner makes does ask.
    #[test]
    fn the_merge_save_never_asks_for_another_cycle() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&lapsed(progress_fixture()).to_json()));
            let backend = backend_of(&store, &FakeFiles::default());
            let asked = Rc::new(Cell::new(0));
            let seen = Rc::clone(&asked);
            backend.on_saved(move || seen.set(seen.get() + 1));
            let shared: SharedSync = Rc::new(transport.clone());
            let sync = SyncCycle::new(shared, backend, || {}, || "10:42".to_owned(), |_, _| {});
            sync.run();
            transport.settle();
            assert_eq!(store.saved().len(), 1, "the merge was saved locally");
            assert_eq!(asked.get(), 0, "and it asked for nothing");
            backend.save(&State::empty());
            assert_eq!(asked.get(), 1, "a learner's save does ask");
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
            let newer = r#"{"version": 4, "decks": {}}"#;
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
    fn a_copy_whose_added_word_is_a_deck_term_is_read_with_the_word_shadowed() {
        Owner::new().with(|| {
            let mut copy = progress_fixture();
            let mut word = crate::test_support::added_word();
            word.id = TermId("kho".into());
            copy.added.push(word);
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            transport.set_drive(Some(&copy.to_json()));
            let (sync, changed) = cycle(&store, &transport);
            sync.run();
            transport.settle();
            // The copy is the local progress once the shadowed word is gone.
            assert_eq!(sync.status().get_untracked(), synced());
            assert_eq!(store.saved(), vec![]);
            assert_eq!(changed.get(), 0);
            assert_eq!(transport.pushed(), Vec::<String>::new());
        });
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
                unsaved: learn_core::book::Book::from_state(progress_fixture()),
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
            store.save(&learn_core::book::Book::from_state(answered.clone()));
            transport.settle();
            let drive = State::from_json(&transport.drive().unwrap()).unwrap();
            assert!(drive.get(&TermId("meanwhile 1".into())).is_some());
            assert_eq!(
                store.load(),
                Load::Ok(learn_core::book::Book::from_state(answered)),
                "the local is kept"
            );
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
            let (sync, _, timers) = timed_cycle(&store, &transport);
            sync.run();
            sync.run();
            sync.run();
            assert_eq!(transport.pull_calls(), 1, "one at a time");
            assert!(transport.answer_pull());
            assert_eq!(transport.pull_calls(), 1, "still running: the push");
            assert!(transport.answer_push());
            assert_eq!(transport.pull_calls(), 1, "the asked one waits the gap");
            fire(&timers);
            assert_eq!(transport.pull_calls(), 2, "the asked one, once");
            transport.settle();
            fire(&timers);
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
            // Signed in again (a gesture asks nothing without a token).
            transport.set_account(SyncAccount::SignedIn { email: None });
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

    /// A session that starts after the push left (the pull's join passed
    /// the held check): the cycle's own end is not held. A failed push
    /// shows Failed, arms nothing and leaves nothing for the session's end.
    /// Caught by: the failed push's end remembering an ask when held
    /// (`release` then arms a cycle).
    #[test]
    fn a_failed_push_answered_under_a_hold_ends_the_cycle_quietly() {
        Owner::new().with(|| {
            let (sync, transport, timers) = pushing_cycle();
            sync.hold();
            timers.borrow_mut().clear();
            transport.fail_next(SyncError::Network);
            assert!(transport.answer_push());
            assert_eq!(
                sync.status().get_untracked(),
                SyncStatus::Failed(SyncError::Network)
            );
            assert_eq!(timers.borrow().len(), 0, "nothing armed");
            sync.release();
            assert_eq!(timers.borrow().len(), 0, "release asks nothing");
            assert_eq!(transport.pull_calls(), 1);
        });
    }

    /// The same with a push that succeeds: Synced, and the release is silent.
    /// Caught by: the push's Ok end remembering an ask when held.
    #[test]
    fn an_ok_push_answered_under_a_hold_is_synced_and_release_is_silent() {
        Owner::new().with(|| {
            let (sync, transport, timers) = pushing_cycle();
            sync.hold();
            timers.borrow_mut().clear();
            assert!(transport.answer_push());
            assert!(matches!(
                sync.status().get_untracked(),
                SyncStatus::Synced { .. }
            ));
            assert_eq!(timers.borrow().len(), 0, "nothing armed");
            sync.release();
            assert_eq!(timers.borrow().len(), 0, "release asks nothing");
            assert_eq!(transport.pull_calls(), 1);
        });
    }

    /// The push's step timeout runs out under a hold: the cycle fails with
    /// the network error (a hold does not stretch the timeout), and the
    /// late answer of that push is dropped by `take(step)`.
    /// Caught by: `take` always answering true (the late Ok overwrites Failed).
    #[test]
    fn a_push_timeout_under_a_hold_fails_and_its_late_answer_is_dropped() {
        Owner::new().with(|| {
            let (sync, transport, timers) = pushing_cycle();
            sync.hold();
            assert_eq!(fire(&timers).len(), 2, "the pull's and the push's");
            let failed = SyncStatus::Failed(SyncError::Network);
            assert_eq!(sync.status().get_untracked(), failed);
            assert!(transport.answer_push(), "late");
            assert_eq!(sync.status().get_untracked(), failed, "dropped");
            sync.release();
            assert_eq!(timers.borrow().len(), 0);
        });
    }

    /// A cycle whose pull is answered (an empty Drive) and whose push waits.
    fn pushing_cycle() -> (SyncCycle, FakeTransport, Timers) {
        let store = FakeStore::new(Load::Ok(progress_fixture()));
        let transport = FakeTransport::signed_in();
        let (sync, _, timers) = timed_cycle(&store, &transport);
        sync.run();
        assert!(transport.answer_pull());
        (sync, transport, timers)
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
            let transport = FakeTransport::signed_in();
            let (sync, _, timers) = timed_cycle(&store, &transport);
            sync.hold();
            sync.run();
            sync.run();
            assert_eq!(transport.pull_calls(), 0, "deferred");
            sync.release();
            assert_eq!(transport.pull_calls(), 0, "armed, not run at once");
            fire(&timers);
            assert_eq!(transport.pull_calls(), 1, "once, at the session's end");
            transport.settle();
            sync.release();
            fire(&timers);
            assert_eq!(transport.pull_calls(), 1, "nothing asked: nothing more");
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
            let (sync, changed, timers) = timed_cycle(&store, &transport);
            sync.run();
            sync.hold();
            assert!(transport.answer_pull());
            assert_eq!(store.saved(), vec![], "nothing saved during the session");
            assert_eq!(changed.get(), 0, "no Reload banner");
            assert_eq!(transport.pushed(), Vec::<String>::new());
            sync.release();
            fire(&timers);
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
                let (sync, _, timers) = timed_cycle(&store, &transport);
                sync.when_ready();
                fire(&timers);
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
            assert_eq!(
                store.load(),
                Load::Ok(learn_core::book::Book::from_state(progress_fixture()))
            );
        });
    }

    /// The web token's expiry reaches the cycle through the transport:
    /// `check_account` is the one door (design: checked at each ask and at
    /// the status line's minute refresh, no request).
    #[test]
    fn check_account_lets_the_transport_turn_an_expired_token_remembered() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::signed_in();
            let (sync, _) = cycle(&store, &transport);
            sync.check_account();
            assert_eq!(transport.refreshes(), 1);
            assert!(matches!(
                sync.account().get_untracked(),
                SyncAccount::SignedIn { .. }
            ));
            transport.expire_on_refresh();
            sync.check_account();
            assert!(matches!(
                sync.account().get_untracked(),
                SyncAccount::Remembered { .. }
            ));
            assert_eq!(transport.pull_calls(), 0, "no request is wasted");
        });
    }

    /// Resume is a gesture: remembered with no token, it asks Google in
    /// the tap (a sign-in), never a window unasked.
    #[test]
    fn resume_asks_for_a_token_in_the_gesture_when_paused() {
        Owner::new().with(|| {
            let store = FakeStore::new(Load::Ok(progress_fixture()));
            let transport = FakeTransport::new(SyncAccount::Remembered { email: None });
            let (sync, _) = cycle(&store, &transport);
            sync.resume();
            assert_eq!(transport.sign_ins_waiting(), 1);
            assert_eq!(transport.pull_calls(), 0);
        });
    }

    // ---- the Book: one join per deck (task 063a) ----

    /// The deck the tests give progress to besides `words`.
    fn zz() -> DeckId {
        DeckId("zz".into())
    }

    /// A Book of `words` and, when given, the `zz` deck.
    fn book_of(words: State, zz_state: Option<State>) -> Book {
        let mut book = Book::from_state(words);
        if let Some(state) = zz_state {
            book.set(zz(), state);
        }
        book
    }

    /// A `zz` deck with one started term.
    fn zz_state() -> State {
        with_term(State::empty(), "zz 1")
    }

    /// A cycle over a store holding `local` and a Drive holding `drive`
    /// (none: no file), run to its end.
    fn run_cycle(
        local: &Book,
        drive: Option<&str>,
    ) -> (FakeStore, FakeTransport, SyncCycle, Rc<Cell<usize>>) {
        let store = FakeStore::with_book(Load::Ok(local.clone()));
        let transport = FakeTransport::signed_in();
        transport.set_drive(drive);
        let (sync, changed) = cycle(&store, &transport);
        sync.run();
        transport.settle();
        (store, transport, sync, changed)
    }

    /// The Book a pushed text holds.
    fn book_of_text(text: &str) -> Book {
        Book::from_json(text).expect("a pushed text reads")
    }

    #[test]
    fn a_deck_only_this_device_has_is_kept_whole_and_pushed_in_stamp_3() {
        Owner::new().with(|| {
            let local = book_of(progress_fixture(), Some(zz_state()));
            let drive = lapsed(progress_fixture());
            let (store, transport, sync, changed) = run_cycle(&local, Some(&drive.to_json()));
            let expected = book_of(drive, Some(zz_state()));
            let saved = store.saved_books();
            assert_eq!(saved.len(), 1, "the Drive's words are saved locally");
            assert!(same(&saved[0], &expected), "zz kept whole: {saved:?}");
            assert_eq!(changed.get(), 1);
            let pushed = transport.pushed();
            assert_eq!(pushed.len(), 1);
            assert!(pushed[0].contains("\"version\": 3"), "{}", pushed[0]);
            assert!(same(&book_of_text(&pushed[0]), &expected));
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn a_deck_only_the_drive_has_is_saved_locally_and_counted() {
        Owner::new().with(|| {
            let local = Book::from_state(progress_fixture());
            let drive = book_of(progress_fixture(), Some(zz_state()));
            let (store, transport, sync, changed) = run_cycle(&local, Some(&drive.to_json()));
            let saved = store.saved_books();
            assert_eq!(saved.len(), 1);
            assert!(same(&saved[0], &drive));
            assert_eq!(changed.get(), 1);
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(sync.terms_from_other().get_untracked(), 1, "the zz term");
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn two_devices_ahead_on_different_decks_join_each_deck() {
        Owner::new().with(|| {
            let local = book_of(lapsed(progress_fixture()), Some(zz_state()));
            let drive = book_of(progress_fixture(), Some(with_term(zz_state(), "zz 2")));
            let (store, transport, sync, _) = run_cycle(&local, Some(&drive.to_json()));
            let expected = book_of(
                lapsed(progress_fixture()),
                Some(with_term(zz_state(), "zz 2")),
            );
            let saved = store.saved_books();
            assert_eq!(saved.len(), 1);
            assert!(same(&saved[0], &expected), "{saved:?}");
            let pushed = transport.pushed();
            assert_eq!(pushed.len(), 1);
            assert!(same(&book_of_text(&pushed[0]), &expected));
            assert_eq!(
                sync.terms_from_other().get_untracked(),
                1,
                "only the zz term came from the Drive"
            );
        });
    }

    #[test]
    fn an_unknown_deck_on_the_drive_survives_the_cycle_and_is_pushed_back() {
        Owner::new().with(|| {
            let local = Book::from_state(lapsed(progress_fixture()));
            let future = DeckId("deck-from-the-future".into());
            let mut drive = Book::from_state(progress_fixture());
            drive.set(future.clone(), with_term(State::empty(), "x 1"));
            let (store, transport, _, _) = run_cycle(&local, Some(&drive.to_json()));
            let saved = store.saved_books();
            assert_eq!(saved.len(), 1);
            assert!(saved[0].get(&future).is_some(), "kept locally");
            let pushed = transport.pushed();
            assert_eq!(pushed.len(), 1);
            let back = book_of_text(&pushed[0]);
            assert!(same_state(
                back.get(&future).expect("pushed back"),
                &with_term(State::empty(), "x 1")
            ));
        });
    }

    #[test]
    fn a_stamp_3_copy_is_unknown_to_a_build_that_reads_only_2() {
        let book = book_of(progress_fixture(), Some(zz_state()));
        assert!(matches!(
            State::from_json_upto(&book.to_json(), 2),
            Err(StateError::UnknownVersion(3))
        ));
        // This build reads it; a stamp 4 is the newer one.
        assert!(Book::from_json(&book.to_json()).is_ok());
        Owner::new().with(|| {
            let local = Book::from_state(progress_fixture());
            let newer = r#"{"version": 4, "decks": {}}"#;
            let (store, transport, sync, changed) = run_cycle(&local, Some(newer));
            assert_eq!(sync.status().get_untracked(), SyncStatus::DriveNewer);
            assert_eq!(store.saved_books(), vec![]);
            assert_eq!(transport.pushed(), Vec::<String>::new());
            assert_eq!(transport.drive().as_deref(), Some(newer));
            assert_eq!(changed.get(), 0);
        });
    }

    #[test]
    fn a_stamp_3_local_is_pushed_stamp_3_over_a_flat_copy_of_the_same_words() {
        Owner::new().with(|| {
            let local = book_of(progress_fixture(), Some(zz_state()));
            let (store, transport, sync, _) =
                run_cycle(&local, Some(&progress_fixture().to_json()));
            assert_eq!(store.saved_books(), vec![], "nothing new locally");
            assert_eq!(transport.pushed(), vec![local.to_json()]);
            assert!(transport.pushed()[0].contains("\"version\": 3"));
            assert_eq!(sync.status().get_untracked(), synced());
        });
    }

    #[test]
    fn a_second_cycle_against_the_pushed_copy_changes_and_pushes_nothing() {
        Owner::new().with(|| {
            let local = book_of(lapsed(progress_fixture()), Some(zz_state()));
            let drive = book_of(progress_fixture(), Some(with_term(zz_state(), "zz 2")));
            let (store, transport, _, _) = run_cycle(&local, Some(&drive.to_json()));
            let (pushed, saved) = (transport.pushed().len(), store.saved_books().len());
            assert_eq!((pushed, saved), (1, 1));
            let (again, changed) = cycle(&store, &transport);
            again.run();
            transport.settle();
            assert_eq!(transport.pushed().len(), pushed, "nothing pushed");
            assert_eq!(store.saved_books().len(), saved, "nothing saved");
            assert_eq!(changed.get(), 0);
            assert_eq!(again.status().get_untracked(), synced());
        });
    }

    #[test]
    fn a_deck_one_side_lacks_is_no_empty_entry_except_words() {
        let words = Book::from_state(progress_fixture());
        let with_empty = book_of(progress_fixture(), Some(State::empty()));
        assert!(!same(&words, &with_empty));
        assert!(!same(&with_empty, &words));
        assert!(same(&with_empty, &with_empty.clone()));
        let missing = Book::empty();
        let empty_words = Book::from_state(State::empty());
        assert!(same(&missing, &empty_words), "words is always there");
    }

    #[test]
    fn the_count_sums_the_decks_and_a_deck_absent_locally_counts_whole() {
        let local = Book::from_state(progress_fixture());
        let merged = book_of(
            lapsed(progress_fixture()),
            Some(with_term(zz_state(), "zz 2")),
        );
        assert_eq!(terms_changed(&local, &merged), 3, "1 lapsed + 2 in zz");
        assert_eq!(terms_changed(&merged, &merged), 0);
    }

    #[test]
    fn the_wall_clock_is_the_epoch_in_milliseconds() {
        assert!(wall_now_ms() > 1.7e12, "after 2023: {}", wall_now_ms());
    }
}
