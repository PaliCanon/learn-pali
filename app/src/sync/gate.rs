//! The gate of the sync cycle (task 059): where an automatic ask becomes a
//! cycle, or nothing. The rules are in the module doc of
//! [`sync`](super); the spacing, the backoff and the coalescing are the
//! [`Pacer`](crate::sync_pacing::Pacer)'s, this file applies its [`Act`]s and the gates around it.

use std::rc::Rc;

use leptos::prelude::*;

use super::{Cycle, SyncStatus};
use crate::backend::{SyncAccount, SyncError};
use crate::sync_pacing::{Act, Outcome, Reason};

impl Cycle {
    /// The monotonic time, milliseconds.
    pub(super) fn now(&self) -> f64 {
        (self.monotonic.borrow())()
    }

    /// Whether the browser says it is online.
    fn is_online(&self) -> bool {
        (self.online.borrow())()
    }

    /// An automatic ask: the gates, then the pacer.
    pub(super) fn ask(self: &Rc<Self>, reason: Reason) {
        #[cfg(feature = "browser")]
        leptos::logging::log!("learn sync: ask {reason:?}");
        self.transport.refresh();
        if self.blocked() {
            return;
        }
        if self.status.get_untracked() == SyncStatus::Offline {
            self.status.set(SyncStatus::Idle);
        }
        let act = self.pacer.borrow_mut().ask(reason, self.now());
        self.apply(act);
    }

    /// Whether a token is at hand: the account is `SignedIn` (the web's
    /// expiry and the desktop's revoked token turn it, see `refresh`).
    fn signed_in(&self) -> bool {
        matches!(
            self.transport.account().get_untracked(),
            SyncAccount::SignedIn { .. }
        )
    }

    /// Whether an automatic start is stopped now: no token, a terminal
    /// state, a session (the ask is remembered), the network (the status
    /// says so). The pacer is not told: nothing is armed, no backoff step.
    fn blocked(&self) -> bool {
        if !self.signed_in() || self.terminal.get() {
            return true;
        }
        if self.held.get() {
            self.pending.set(true);
            return true;
        }
        if !self.is_online() {
            self.show_offline();
            return true;
        }
        false
    }

    /// 'Offline' (not a failure), unless a cycle runs: it will say how it ended.
    fn show_offline(&self) {
        if !self.pacer.borrow().in_flight() {
            self.status.set(SyncStatus::Offline);
        }
    }

    /// A gesture (Sync now, Retry, Resume, sign-in): the account is
    /// refreshed first and nothing is asked, nothing touched, unless a token
    /// is at hand (the paused or signed-out state is the Resume and sign-in
    /// buttons' to leave). Outside a session it lifts a terminal state and,
    /// online, starts at once; during one it only remembers the ask and
    /// leaves a terminal state set (the session's end asks again, and a
    /// stopped cycle stays stopped until a gesture outside a session).
    pub(super) fn run(self: &Rc<Self>) {
        self.transport.refresh();
        if !self.signed_in() {
            return;
        }
        if self.held.get() {
            // A session lifts nothing: a stopped cycle stays stopped.
            self.pending.set(true);
            return;
        }
        self.terminal.set(false);
        if self.is_online() {
            let act = self.pacer.borrow_mut().gesture();
            self.apply(act);
        } else {
            self.show_offline();
        }
    }

    /// The timer `id` ran out: a stale id (cancelled, replaced or already
    /// fired) has no effect at all; otherwise the gates are applied again
    /// before the pacer is told, and a gate that blocks drops the timer.
    fn fired(self: &Rc<Self>, id: u64) {
        if !self.pacer.borrow().is_armed(id) {
            return;
        }
        self.transport.refresh();
        if self.blocked() {
            self.pacer.borrow_mut().cancel();
            return;
        }
        let act = self.pacer.borrow_mut().fired(id, self.now());
        self.apply(act);
    }

    /// Does what the pacer says: start, arm one timer, or nothing.
    pub(super) fn apply(self: &Rc<Self>, act: Act) {
        match act {
            Act::Nothing => {}
            Act::Start => self.start(),
            Act::Arm { id, delay } => {
                let cycle = Rc::clone(self);
                (self.timer)(delay, Box::new(move || cycle.fired(id)));
            }
        }
    }

    /// Ends the cycle with `status`; the pacer is told how it ended and
    /// what it says (the one asked meanwhile) is done.
    pub(super) fn finish(self: &Rc<Self>, status: SyncStatus) {
        let (status, outcome) = self.outcome_of(status);
        self.status.set(status);
        let act = self.pacer.borrow_mut().finished(outcome, self.now());
        self.apply(act);
    }

    /// The status a cycle's end shows and its outcome for the backoff. A
    /// terminal state is a success (nothing to retry: the ladder is reset),
    /// so is a network error while the browser is offline (shown as
    /// 'Offline'); every other failure climbs the ladder. A synced end is
    /// stamped with the wall clock.
    fn outcome_of(&self, status: SyncStatus) -> (SyncStatus, Outcome) {
        match status {
            SyncStatus::Refused(_)
            | SyncStatus::DriveNewer
            | SyncStatus::DriveUnreadable { .. } => {
                self.terminal.set(true);
                (status, Outcome::Success)
            }
            SyncStatus::Failed(SyncError::Network) if !self.is_online() => {
                (SyncStatus::Offline, Outcome::Success)
            }
            SyncStatus::Failed(_) => (status, Outcome::Failure),
            SyncStatus::Synced { .. } => {
                self.synced_at.set(Some((self.wall.borrow())()));
                (status, Outcome::Success)
            }
            SyncStatus::Idle | SyncStatus::Syncing | SyncStatus::Offline => {
                (status, Outcome::Success)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    use learn_core::progress::State;
    use leptos::prelude::*;

    use super::super::tests::{
        Timers, fire, lapsed, synced, test_now, timed_cycle, with_term, with_word,
    };
    use super::super::{Refusal, STEP_TIMEOUT, SyncCycle, SyncStatus, terms_changed};
    use crate::backend::{SyncAccount, SyncError};
    use crate::store::Load;
    use crate::sync_pacing::{MIN_GAP, Reason};
    use crate::test_support::{FakeStore, FakeTransport, progress_fixture};

    /// The wall clock the rig starts with.
    const WALL: f64 = 1.7e12;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// A cycle over fakes, with its clocks and its online probe in the
    /// test's hands. The Drive holds the local progress, so a cycle ends at
    /// the pull's answer (no push, no second step timer).
    struct Rig {
        sync: SyncCycle,
        transport: FakeTransport,
        store: FakeStore,
        timers: Timers,
        wall: Rc<Cell<f64>>,
        online: Rc<Cell<bool>>,
    }

    impl Rig {
        fn new(account: SyncAccount, load: Load) -> Rig {
            let store = FakeStore::new(load);
            let transport = FakeTransport::new(account);
            transport.set_drive(Some(&progress_fixture().to_json()));
            let (sync, _, timers) = timed_cycle(&store, &transport);
            let wall = Rc::new(Cell::new(WALL));
            let clock = Rc::clone(&wall);
            sync.set_clocks(test_now, move || clock.get());
            let online = Rc::new(Cell::new(true));
            let probe = Rc::clone(&online);
            sync.set_online(move || probe.get());
            Rig {
                sync,
                transport,
                store,
                timers,
                wall,
                online,
            }
        }

        fn signed_in() -> Rig {
            Rig::new(
                SyncAccount::SignedIn { email: None },
                Load::Ok(progress_fixture()),
            )
        }

        fn status(&self) -> SyncStatus {
            self.sync.status().get_untracked()
        }

        /// The delays of the timers waiting.
        fn armed(&self) -> Vec<Duration> {
            self.timers.borrow().iter().map(|(d, _)| *d).collect()
        }

        /// The timers armed so far, taken out (to fire them later).
        fn drain(&self) -> Timers {
            Rc::new(RefCell::new(self.timers.borrow_mut().drain(..).collect()))
        }

        /// Answers the running cycle's pull, with a network failure when
        /// `fail`; the timers it armed before (its step timeout) are
        /// dropped, so only what the cycle's end armed is left.
        fn end(&self, fail: bool) {
            if fail {
                self.transport.fail_next(SyncError::Network);
            }
            self.timers.borrow_mut().clear();
            assert!(self.transport.answer_pull(), "a pull waits");
        }

        /// One round: an ask, its timer fires, the cycle ends. The delay it
        /// waited.
        fn round(&self, fail: bool) -> Duration {
            self.sync.ask(Reason::Tick);
            let armed = self.armed();
            assert_eq!(armed.len(), 1, "one timer: {armed:?}");
            fire(&self.timers);
            self.end(fail);
            armed[0]
        }
    }

    /// The fixture with its first `n` entries lapsed once more.
    fn lapsed_n(n: usize) -> learn_core::progress::State {
        let mut state = progress_fixture();
        for entry in &mut state.progress[..n] {
            entry.lapses += 1;
            entry.stage = learn_core::stage::Stage::Day;
        }
        state
    }

    // ---- a: what the gate ignores or holds ----

    /// Design: "ignored when paused, signed out". The signed-in row is the
    /// control: an ask arms one timer, its firing runs one cycle. Does not
    /// catch: a gate that checks the account after arming (the timer is
    /// then re-checked by `fired`, see the expiry test).
    #[test]
    fn an_automatic_ask_is_ignored_unless_signed_in() {
        for (account, expected) in [
            (SyncAccount::SignedOut, 0),
            (SyncAccount::Remembered { email: None }, 0),
            (SyncAccount::SignedIn { email: None }, 1),
        ] {
            Owner::new().with(|| {
                let rig = Rig::new(account.clone(), Load::Ok(progress_fixture()));
                rig.sync.ask(Reason::Visible);
                assert_eq!(rig.timers.borrow().len(), expected, "{account:?}");
                fire(&rig.timers);
                assert_eq!(rig.transport.pull_calls(), expected, "{account:?}");
            });
        }
    }

    /// The three terminal states: how each is reached, what it shows.
    fn terminal_rig(kind: &str) -> (Rig, SyncStatus) {
        let load = match kind {
            "refused" => Load::Unreadable("denied".into()),
            _ => Load::Ok(progress_fixture()),
        };
        let rig = Rig::new(SyncAccount::SignedIn { email: None }, load);
        match kind {
            "newer" => rig
                .transport
                .set_drive(Some(r#"{"version": 3, "progress": []}"#)),
            "unreadable" => rig.transport.set_drive(Some("{not json")),
            _ => {}
        }
        rig.sync.run();
        rig.transport.settle();
        rig.timers.borrow_mut().clear();
        let status = match kind {
            "refused" => SyncStatus::Refused(Refusal::Unreadable),
            "newer" => SyncStatus::DriveNewer,
            _ => SyncStatus::DriveUnreadable {
                raw: "{not json".into(),
            },
        };
        (rig, status)
    }

    /// Design: "terminal states (`Refused`, `DriveNewer`, `DriveUnreadable`): no
    /// automatic cycle until Sync now or a reload". The gesture runs again
    /// and, once it ends well, the control: an ask arms a timer again.
    #[test]
    fn a_terminal_state_ignores_asks_until_a_gesture() {
        for kind in ["refused", "newer", "unreadable"] {
            Owner::new().with(|| {
                let (rig, status) = terminal_rig(kind);
                assert_eq!(rig.status(), status, "{kind}");
                let pulls = rig.transport.pull_calls();
                for reason in [Reason::Visible, Reason::Tick, Reason::Online, Reason::Home] {
                    rig.sync.ask(reason);
                }
                assert_eq!(rig.timers.borrow().len(), 0, "{kind}: nothing armed");
                assert_eq!(rig.transport.pull_calls(), pulls, "{kind}");
                // A gesture runs again, whatever the state.
                rig.store.changed_to(Load::Ok(progress_fixture()));
                rig.transport.set_drive(Some(&progress_fixture().to_json()));
                rig.sync.run();
                assert_eq!(rig.transport.pull_calls(), pulls + 1, "{kind}: a gesture");
                rig.transport.settle();
                assert_eq!(rig.status(), synced(), "{kind}");
                rig.timers.borrow_mut().clear();
                rig.sync.ask(Reason::Tick);
                assert_eq!(
                    rig.armed(),
                    vec![MIN_GAP],
                    "{kind}: the terminal state is over"
                );
            });
        }
    }

    /// A terminal state reached by an automatic cycle's timer, with an ask
    /// armed behind it: the timer, fired, does nothing.
    #[test]
    fn a_timer_armed_before_a_terminal_state_fires_into_nothing() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.transport
                .set_drive(Some(r#"{"version": 3, "progress": []}"#));
            rig.sync.run();
            rig.sync.ask(Reason::Tick);
            rig.end(false);
            assert_eq!(rig.status(), SyncStatus::DriveNewer);
            assert_eq!(rig.armed(), vec![MIN_GAP], "the ask made during the cycle");
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 1, "the terminal state wins");
        });
    }

    /// Design: "held by a session until release". Three asks during a
    /// session make one cycle at its end, not three.
    #[test]
    fn a_held_ask_waits_for_the_session_and_arms_one_cycle_at_its_end() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.hold();
            for reason in [Reason::Visible, Reason::Tick, Reason::Saved] {
                rig.sync.ask(reason);
            }
            assert_eq!(rig.timers.borrow().len(), 0, "nothing armed in a session");
            assert_eq!(rig.transport.pull_calls(), 0);
            rig.sync.release();
            assert_eq!(rig.armed(), vec![Duration::ZERO], "one, not three");
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 1);
        });
    }

    /// Back home with nothing asked during the session: nothing is asked.
    #[test]
    fn release_asks_nothing_when_nothing_was_asked() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.hold();
            rig.sync.release();
            assert_eq!(rig.timers.borrow().len(), 0);
            rig.sync.hold();
            rig.sync.ask(Reason::Tick);
            rig.sync.release();
            rig.sync.release();
            assert_eq!(rig.armed().len(), 1, "the one ask, once");
        });
    }

    /// A gesture during a session (held) lifts nothing: a stopped cycle
    /// stays stopped, nothing is armed or pulled, and the session's end asks
    /// again without reviving it. A gesture outside a session lifts it.
    /// Caught by: `terminal.set(false)` moved back before the held check
    /// (the release then arms a cycle into a state that is not over).
    #[test]
    fn a_gesture_under_a_hold_leaves_a_terminal_state_set() {
        for kind in ["refused", "newer", "unreadable"] {
            Owner::new().with(|| {
                let (rig, status) = terminal_rig(kind);
                let pulls = rig.transport.pull_calls();
                rig.sync.hold();
                rig.sync.run();
                assert_eq!(rig.transport.pull_calls(), pulls, "{kind}: held");
                assert_eq!(rig.timers.borrow().len(), 0, "{kind}: nothing armed");
                assert_eq!(rig.status(), status, "{kind}: still terminal");
                rig.sync.release();
                assert_eq!(rig.timers.borrow().len(), 0, "{kind}: still stopped");
                rig.sync.ask(Reason::Tick);
                assert_eq!(rig.timers.borrow().len(), 0, "{kind}: still terminal");
                assert_eq!(rig.transport.pull_calls(), pulls, "{kind}");
                rig.store.changed_to(Load::Ok(progress_fixture()));
                rig.transport.set_drive(Some(&progress_fixture().to_json()));
                rig.sync.run();
                assert_eq!(rig.transport.pull_calls(), pulls + 1, "{kind}: lifted");
            });
        }
    }

    /// A timer armed before the session that runs out during it: it
    /// starts nothing, and the session's end asks again.
    #[test]
    fn a_timer_that_fires_during_a_session_waits_for_its_end() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed().len(), 1);
            rig.sync.hold();
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 0, "held");
            rig.sync.release();
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 1, "at the session's end");
        });
    }

    // ---- b, c: the burst and the ask during a cycle ----

    /// Design: "one 'armed' flag and one trailing timer". A wake-up burst
    /// (visible, focus, tick) is one timer and one pull.
    #[test]
    fn a_wake_burst_arms_one_timer_and_runs_one_cycle() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Visible);
            rig.sync.ask(Reason::Visible);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.timers.borrow().len(), 1, "{:?}", rig.armed());
            assert_eq!(rig.transport.pull_calls(), 0, "never at once");
            assert_eq!(fire(&rig.timers), vec![Duration::ZERO]);
            assert_eq!(rig.transport.pull_calls(), 1);
        });
    }

    /// Design: "a cycle in flight sets 'again' (one more after it, so a
    /// change made during a push is not lost)": one more, after `MIN_GAP`
    /// when the first succeeded; never dropped.
    #[test]
    fn an_ask_during_a_cycle_runs_one_more_after_the_gap() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.run();
            rig.sync.ask(Reason::Saved);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.timers.borrow().len(), 1, "only the step's timeout");
            rig.end(false);
            assert_eq!(rig.armed(), vec![MIN_GAP], "not at once");
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 2, "one more");
            rig.end(false);
            assert_eq!(rig.armed(), Vec::<Duration>::new(), "and no more");
        });
    }

    /// After a failure the one more waits the backoff, never zero.
    #[test]
    fn an_ask_during_a_failing_cycle_waits_the_backoff() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            assert_eq!(rig.round(true), Duration::ZERO);
            assert_eq!(rig.round(true), secs(15));
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![secs(30)]);
            fire(&rig.timers);
            rig.sync.ask(Reason::Saved);
            rig.end(true);
            assert_eq!(rig.armed(), vec![secs(60)], "the third failure's step");
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 4, "run, not dropped");
        });
    }

    // ---- d: offline ----

    /// Design: "offline (navigator.onLine false: status 'Offline', not a
    /// failure ...)": the asks arm nothing and touch no ladder; back online
    /// the first ask waits the gap of the one earlier failure (15 s), and
    /// the status leaves Offline.
    #[test]
    fn an_ask_while_offline_shows_offline_and_arms_nothing() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.round(true);
            rig.online.set(false);
            for reason in [Reason::Visible, Reason::Tick, Reason::Home] {
                rig.sync.ask(reason);
            }
            assert_eq!(rig.status(), SyncStatus::Offline);
            assert_eq!(rig.timers.borrow().len(), 0, "{:?}", rig.armed());
            assert_eq!(rig.transport.pull_calls(), 1);
            rig.online.set(true);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.status(), SyncStatus::Idle, "no longer offline");
            assert_eq!(rig.armed(), vec![secs(15)], "the ladder was not touched");
        });
    }

    /// Design: "a gesture while offline shows 'Offline' at once and starts
    /// no cycle".
    #[test]
    fn a_gesture_while_offline_shows_offline_and_starts_nothing() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.online.set(false);
            rig.sync.run();
            assert_eq!(rig.status(), SyncStatus::Offline);
            assert_eq!(rig.transport.pull_calls(), 0);
            assert_eq!(rig.timers.borrow().len(), 0);
            rig.online.set(true);
            rig.sync.run();
            assert_eq!(rig.transport.pull_calls(), 1, "back online it runs");
        });
    }

    /// Design: "the online event resets [the backoff] and asks": after
    /// three failures an Online ask waits the gap, not 60 s, and the timer
    /// armed at the old delay does nothing when it runs out.
    #[test]
    fn the_online_reason_resets_a_standing_backoff() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            for _ in 0..3 {
                rig.round(true);
            }
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![secs(60)]);
            rig.sync.ask(Reason::Online);
            assert_eq!(rig.armed(), vec![secs(60), MIN_GAP]);
            assert_eq!(fire(&rig.timers), vec![secs(60), MIN_GAP]);
            assert_eq!(rig.transport.pull_calls(), 4, "one cycle, not two");
        });
    }

    /// Design: "not a failure": a network error while the probe says
    /// offline is 'Offline', and does not climb the ladder.
    #[test]
    fn a_network_failure_while_offline_is_offline_not_a_failure() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.round(true);
            rig.sync.ask(Reason::Tick);
            fire(&rig.timers);
            rig.online.set(false);
            rig.end(true);
            assert_eq!(rig.status(), SyncStatus::Offline);
            assert_eq!(rig.timers.borrow().len(), 0);
            rig.online.set(true);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![secs(15)], "not the second step (30 s)");
        });
    }

    /// A network error with the probe online stays a failure.
    #[test]
    fn a_network_failure_while_online_is_a_failure() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.round(true);
            assert_eq!(
                rig.status(),
                SyncStatus::Failed(SyncError::Network),
                "the status"
            );
        });
    }

    /// A timer that runs out offline starts nothing and is dropped: the
    /// next ask arms again.
    #[test]
    fn a_timer_that_fires_offline_starts_nothing() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Tick);
            rig.online.set(false);
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 0);
            assert_eq!(rig.status(), SyncStatus::Offline);
            rig.online.set(true);
            rig.sync.ask(Reason::Online);
            assert_eq!(rig.armed().len(), 1, "armed again");
        });
    }

    /// An ask during a cycle, offline: the running cycle's status stays.
    #[test]
    fn an_offline_ask_during_a_cycle_leaves_its_status() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.run();
            rig.online.set(false);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.status(), SyncStatus::Syncing);
        });
    }

    // ---- e, f: the ladder and the gestures ----

    /// Design: backoff 15 s (the gap), 30 s, 1 min, 5 min cap, after the
    /// end of the failed cycle; a success resets it.
    #[test]
    fn the_ladder_is_the_gap_then_30_60_300_300_and_a_success_resets_it() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            let delays: Vec<_> = [true, true, true, true, true, false, false]
                .into_iter()
                .map(|fail| rig.round(fail))
                .collect();
            let expected = [0, 15, 30, 60, 300, 300, 15].map(secs);
            assert_eq!(delays, expected);
        });
    }

    /// The gap runs from the cycle's end by the monotonic clock.
    #[test]
    fn the_gap_runs_from_the_end_of_the_last_cycle() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            let now = Rc::new(Cell::new(1_000_000.0));
            let clock = Rc::clone(&now);
            rig.sync.set_clocks(move || clock.get(), || WALL);
            rig.round(false);
            now.set(1_010_000.0);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![secs(5)], "15 s since the end, 10 s gone");
        });
    }

    /// Design: gestures "bypass the gap: they cancel the armed timer, clear
    /// 'armed', reset the backoff and start at once".
    #[test]
    fn a_gesture_starts_at_once_and_cancels_the_armed_timer() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.run();
            rig.end(false);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![MIN_GAP]);
            let stale = rig.drain();
            rig.sync.run();
            assert_eq!(rig.transport.pull_calls(), 2, "at once, inside the gap");
            fire(&stale);
            assert_eq!(rig.transport.pull_calls(), 2, "the old timer does nothing");
            rig.end(false);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed().len(), 1, "and nothing stays 'armed'");
        });
    }

    /// A gesture resets the backoff: one failure after it is the first
    /// step, not the fourth.
    #[test]
    fn a_gesture_resets_the_backoff() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            for _ in 0..3 {
                rig.round(true);
            }
            rig.sync.run();
            rig.end(true);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![MIN_GAP], "not 60 s");
        });
    }

    /// Sign-in is a gesture: the cycle starts in the answer, no timer.
    #[test]
    fn a_sign_in_starts_the_cycle_at_once() {
        Owner::new().with(|| {
            let rig = Rig::new(SyncAccount::SignedOut, Load::Ok(progress_fixture()));
            rig.sync.sign_in();
            rig.transport.answer_sign_in(Ok(()));
            assert_eq!(rig.transport.pull_calls(), 1);
            assert_eq!(rig.armed(), vec![STEP_TIMEOUT], "the step's timeout only");
        });
    }

    /// Design: "a closed window (Cancelled) returns to the state before the
    /// tap, no error"; a blocked window is Popup; the others are the cause.
    #[test]
    fn a_sign_in_that_fails_leaves_or_sets_the_status() {
        let failed = SyncStatus::Failed(SyncError::Network);
        for (before, error, after) in [
            (SyncStatus::Idle, SyncError::Cancelled, SyncStatus::Idle),
            (failed.clone(), SyncError::Cancelled, failed.clone()),
            (
                failed.clone(),
                SyncError::Popup,
                SyncStatus::Failed(SyncError::Popup),
            ),
            (
                SyncStatus::Idle,
                SyncError::Drive("403".into()),
                SyncStatus::Failed(SyncError::Drive("403".into())),
            ),
        ] {
            Owner::new().with(|| {
                let rig = Rig::new(
                    SyncAccount::Remembered { email: None },
                    Load::Ok(progress_fixture()),
                );
                rig.sync.set_status(before.clone());
                rig.sync.sign_in();
                rig.transport.answer_sign_in(Err(error.clone()));
                assert_eq!(rig.status(), after, "{before:?} then {error:?}");
                assert_eq!(rig.transport.pull_calls(), 0);
            });
        }
    }

    // ---- g: sign-out cuts everything ----

    /// Design: "sign-out clears pending timers, backoff and flags": the
    /// timer armed before it does nothing, even with a token back.
    #[test]
    fn a_sign_out_makes_the_armed_timer_stale() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Tick);
            rig.sync.sign_out();
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 0);
        });
    }

    /// The backoff, the terminal state, the pending flag, the stamp and the
    /// count are cleared by a sign-out.
    #[test]
    fn a_sign_out_clears_the_backoff_the_flags_and_the_stamp() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            for _ in 0..3 {
                rig.round(true);
            }
            rig.sync.set_synced(Some(WALL), 3);
            rig.sync.sign_out();
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            assert_eq!(rig.sync.synced_at().get_untracked(), None);
            assert_eq!(rig.sync.terms_from_other().get_untracked(), 0);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![Duration::ZERO], "no backoff, no last end");
            // A terminal state does not survive it either.
            let (rig, _) = terminal_rig("newer");
            rig.sync.sign_out();
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed().len(), 1, "terminal cleared");
            // Nor does an ask held for a session.
            let rig = Rig::signed_in();
            rig.sync.hold();
            rig.sync.ask(Reason::Tick);
            rig.sync.sign_out();
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            rig.sync.release();
            assert_eq!(rig.timers.borrow().len(), 0, "pending cleared");
        });
    }

    // ---- h: the web expiry ----

    /// Design: "the web transport turns `SignedIn` into `Remembered` ... checked
    /// at each ask": an expired token pauses the sync, no request. Every
    /// automatic reason meets it the same way (G3, review round 2): the
    /// account is refreshed first and found `Remembered` (the paused state),
    /// no timer is armed, no pull is made, the status is untouched and
    /// nothing is held for later (a release arms nothing). Assertions added
    /// to the first row of round 1: the five reasons instead of one, the
    /// account, the status, the held flag. Does not catch: a gate that
    /// refreshes but reads the old account for the rest of the ask (the
    /// account assertion does), or a status written and restored.
    #[test]
    fn an_ask_after_the_token_expired_is_ignored() {
        for reason in [
            Reason::Visible,
            Reason::Tick,
            Reason::Saved,
            Reason::Online,
            Reason::Home,
        ] {
            Owner::new().with(|| {
                let rig = Rig::signed_in();
                let status = rig.status();
                rig.transport.expire_on_refresh();
                rig.sync.ask(reason);
                assert_eq!(
                    rig.transport.refreshes(),
                    1,
                    "{reason:?}: checked at the ask"
                );
                assert_eq!(rig.timers.borrow().len(), 0, "{reason:?}: no timer");
                assert_eq!(rig.transport.pull_calls(), 0, "{reason:?}: no pull");
                assert_eq!(
                    rig.sync.account().get_untracked(),
                    SyncAccount::Remembered { email: None },
                    "{reason:?}: paused"
                );
                assert_eq!(rig.status(), status, "{reason:?}: status untouched");
                rig.sync.hold();
                rig.sync.release();
                assert_eq!(rig.timers.borrow().len(), 0, "{reason:?}: nothing held");
            });
        }
    }

    /// The same at the timer: a token that expired while it waited.
    #[test]
    fn a_timer_that_fires_after_the_token_expired_starts_nothing() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Visible);
            rig.transport.expire_on_refresh();
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), 0);
            assert_eq!(
                rig.sync.account().get_untracked(),
                SyncAccount::Remembered { email: None }
            );
        });
    }

    // ---- i: the stamp ----

    /// Design: "the age ... from the wall-clock stamp taken when the cycle
    /// ended in this tab": a Synced cycle stamps it; a failed or refused
    /// one leaves it.
    #[test]
    fn the_stamp_is_the_wall_clock_at_a_synced_end_only() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            assert_eq!(rig.sync.synced_at().get_untracked(), None);
            rig.sync.run();
            rig.end(false);
            assert_eq!(rig.sync.synced_at().get_untracked(), Some(WALL));
            rig.wall.set(2.0 * WALL);
            rig.sync.run();
            rig.end(true);
            assert_eq!(rig.sync.synced_at().get_untracked(), Some(WALL), "failed");
            rig.store.changed_to(Load::Unreadable("denied".into()));
            rig.sync.run();
            assert_eq!(rig.status(), SyncStatus::Refused(Refusal::Unreadable));
            assert_eq!(rig.sync.synced_at().get_untracked(), Some(WALL), "refused");
            rig.store.changed_to(Load::Ok(progress_fixture()));
            rig.sync.run();
            rig.end(false);
            assert_eq!(rig.sync.synced_at().get_untracked(), Some(2.0 * WALL));
        });
    }

    // ---- j: the count ----

    /// Design: "counted with the merge's own set comparison": the terms of
    /// the merge that are new or differ from the local.
    #[test]
    fn terms_changed_counts_the_terms_the_merge_changed() {
        let base = progress_fixture();
        let mut reordered = base.clone();
        reordered.progress.reverse();
        let mut two_words = with_word(base.clone());
        let mut other = crate::test_support::added_word();
        other.id = learn_core::deck::TermId("abhiññā 1".into());
        two_words.added.push(other);
        // The same word also started: one term, not two.
        let mut started = with_word(base.clone());
        started.progress.push(learn_core::progress::Progress::new(
            crate::test_support::added_word().id,
            crate::test_support::today(),
        ));
        for (merged, expected, what) in [
            (base.clone(), 0, "identical"),
            (lapsed(base.clone()), 1, "one term ahead"),
            (with_term(base.clone(), "new 1"), 1, "a new term"),
            (reordered, 0, "a reordered copy"),
            (with_word(base.clone()), 1, "an added word"),
            (two_words, 2, "two added words"),
            (started, 1, "a word and its progress are one term"),
            (lapsed_n(2), 2, "two terms ahead"),
        ] {
            assert_eq!(terms_changed(&base, &merged), expected, "{what}");
        }
    }

    /// Design: "a cycle that merged another device's progress adds N terms
    /// from another device, kept until the next such cycle".
    #[test]
    fn the_count_is_set_by_a_merge_kept_by_a_quiet_cycle_and_replaced() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            assert_eq!(rig.sync.terms_from_other().get_untracked(), 0);
            rig.transport.set_drive(Some(&lapsed_n(2).to_json()));
            rig.sync.run();
            rig.transport.settle();
            assert_eq!(rig.sync.terms_from_other().get_untracked(), 2);
            rig.sync.run();
            rig.transport.settle();
            assert_eq!(rig.sync.terms_from_other().get_untracked(), 2, "kept");
            rig.transport.set_drive(Some(&lapsed_n(3).to_json()));
            rig.sync.run();
            rig.transport.settle();
            assert_eq!(rig.sync.terms_from_other().get_untracked(), 1, "replaced");
        });
    }

    /// A merge this device only pushes (it is ahead) brings nothing from
    /// another device.
    #[test]
    fn a_cycle_that_only_pushes_counts_nothing() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.transport.set_drive(Some(&State::empty().to_json()));
            rig.sync.run();
            rig.transport.settle();
            assert_eq!(rig.sync.terms_from_other().get_untracked(), 0);
        });
    }

    // ---- m: the open ----

    /// Design: a cycle at open when a token is at hand (a Visible ask).
    #[test]
    fn mount_asks_at_open_when_signed_in() {
        for (account, armed) in [
            (SyncAccount::SignedIn { email: None }, vec![Duration::ZERO]),
            (SyncAccount::Remembered { email: None }, vec![]),
            (SyncAccount::SignedOut, vec![]),
        ] {
            Owner::new().with(|| {
                let rig = Rig::new(account.clone(), Load::Ok(progress_fixture()));
                rig.sync.mount();
                assert!(rig.sync.mounted());
                assert_eq!(rig.armed(), armed, "{account:?}: armed, not run");
                assert_eq!(rig.transport.pull_calls(), 0, "{account:?}");
                fire(&rig.timers);
                assert_eq!(rig.transport.pull_calls(), armed.len(), "{account:?}");
            });
        }
    }

    // ---- n: round 1 (F2 to F5, F12) ----

    /// The rig after a failed round and an ask that armed the 15 s retry:
    /// the status is `Failed(Network)` and one timer waits.
    fn retry_armed() -> Rig {
        let rig = Rig::signed_in();
        rig.round(true);
        rig.sync.ask(Reason::Tick);
        assert_eq!(rig.armed(), vec![MIN_GAP]);
        rig
    }

    /// F2, design: "ignored when paused, signed out" holds for a gesture
    /// too: it refreshes the account first and asks nothing when no token
    /// is at hand; nothing the pacer or the gate holds is touched. Does not
    /// catch: a check made after `terminal` is cleared but restored later.
    #[test]
    fn a_gesture_asks_nothing_when_no_token_is_at_hand() {
        for account in [
            SyncAccount::Remembered { email: None },
            SyncAccount::SignedOut,
        ] {
            Owner::new().with(|| {
                let rig = retry_armed();
                let pulls = rig.transport.pull_calls();
                rig.transport.set_account(account.clone());
                rig.sync.run();
                assert_eq!(rig.transport.pull_calls(), pulls, "{account:?}: no pull");
                assert_eq!(rig.status(), SyncStatus::Failed(SyncError::Network));
                assert_eq!(rig.armed(), vec![MIN_GAP], "{account:?}: timer kept");
                // The pacer was not told: the armed timer still runs its cycle.
                rig.transport
                    .set_account(SyncAccount::SignedIn { email: None });
                fire(&rig.timers);
                assert_eq!(rig.transport.pull_calls(), pulls + 1, "{account:?}");
            });
        }
    }

    /// F2: a terminal state stays terminal when the gesture finds no token
    /// (it is lifted only by a gesture that can run), and a gesture held
    /// for a session leaves no `pending`: the release arms nothing.
    #[test]
    fn a_gesture_without_a_token_lifts_no_terminal_state_and_holds_nothing() {
        Owner::new().with(|| {
            let (rig, status) = terminal_rig("newer");
            let pulls = rig.transport.pull_calls();
            rig.transport
                .set_account(SyncAccount::Remembered { email: None });
            rig.sync.run();
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            assert_eq!(rig.transport.pull_calls(), pulls);
            assert_eq!(rig.status(), status);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.timers.borrow().len(), 0, "still terminal");
        });
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.hold();
            rig.transport
                .set_account(SyncAccount::Remembered { email: None });
            rig.sync.run();
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            rig.sync.release();
            assert_eq!(rig.timers.borrow().len(), 0, "nothing was held");
        });
    }

    /// F2: the gesture refreshes the account first: a token that expired
    /// (web) or was refused (desktop) since the last ask is found, the
    /// account says so, nothing is asked. The signed-in row is the control:
    /// a gesture starts at once. Does not catch: a refresh made after the
    /// account was read.
    #[test]
    fn a_gesture_refreshes_the_account_before_it_starts() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.transport.expire_on_refresh();
            rig.sync.run();
            assert_eq!(rig.transport.pull_calls(), 0, "expired");
            assert_eq!(
                rig.sync.account().get_untracked(),
                SyncAccount::Remembered { email: None }
            );
        });
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.transport.refuse_on_refresh();
            rig.sync.run();
            assert_eq!(rig.transport.pull_calls(), 0, "refused");
            assert_eq!(rig.sync.account().get_untracked(), SyncAccount::SignedOut);
        });
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.run();
            assert_eq!(rig.transport.pull_calls(), 1, "control: at once");
            assert_eq!(rig.transport.refreshes(), 1);
        });
    }

    /// F3, design: "the timer `id` ran out ... a stale id is dropped": a
    /// timer the gesture cancelled does nothing when it runs out, even when a
    /// fresh one was armed since (no refresh, no pull); the fresh one pulls.
    #[test]
    fn a_stale_timer_has_no_effect_even_beside_a_fresh_one() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Tick);
            let old = rig.drain();
            rig.sync.run();
            rig.end(false);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![MIN_GAP], "a fresh timer");
            let (pulls, refreshes) = (rig.transport.pull_calls(), rig.transport.refreshes());
            fire(&old);
            assert_eq!(rig.transport.pull_calls(), pulls, "the old one: no pull");
            assert_eq!(rig.transport.refreshes(), refreshes, "and no refresh");
            assert_eq!(rig.armed(), vec![MIN_GAP], "the fresh one stays armed");
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), pulls + 1, "the fresh one pulls");
        });
    }

    /// F3: a stale timer fired during a session sets no `pending`: the
    /// release arms nothing. A stale timer fired offline shows no 'Offline'.
    #[test]
    fn a_stale_timer_sets_no_flag_and_no_status() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Tick);
            let old = rig.drain();
            rig.sync.run();
            rig.end(false);
            rig.sync.hold();
            fire(&old);
            rig.sync.release();
            assert_eq!(rig.timers.borrow().len(), 0, "no pending from a stale id");
        });
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            rig.sync.ask(Reason::Tick);
            let old = rig.drain();
            rig.sync.run();
            rig.end(false);
            rig.online.set(false);
            fire(&old);
            assert_eq!(rig.status(), synced(), "no Offline from a stale id");
        });
    }

    /// F4, design: "a closed window (Cancelled) returns to the state before
    /// the tap, no error". Green on arrival once F2 is in (the Cancelled arm
    /// already did nothing): the status stays, no pull, no sign-in waits,
    /// and the retry armed before the tap still fires after it. Does not
    /// catch: a Cancelled arm that touches state the test does not read.
    #[test]
    fn a_cancelled_sign_in_changes_nothing() {
        Owner::new().with(|| {
            let rig = retry_armed();
            rig.transport
                .set_account(SyncAccount::Remembered { email: None });
            rig.sync.sign_in();
            let pulls = rig.transport.pull_calls();
            rig.transport.answer_sign_in(Err(SyncError::Cancelled));
            assert_eq!(rig.status(), SyncStatus::Failed(SyncError::Network));
            assert_eq!(rig.transport.pull_calls(), pulls, "no pull");
            assert_eq!(rig.transport.sign_ins_waiting(), 0);
            assert_eq!(rig.armed(), vec![MIN_GAP], "the retry stays armed");
            rig.transport
                .set_account(SyncAccount::SignedIn { email: None });
            fire(&rig.timers);
            assert_eq!(rig.transport.pull_calls(), pulls + 1, "and still fires");
        });
    }

    /// F5, design: a cycle held by a session did no I/O: it leaves the
    /// ladder as it was. Three failures, a session starts during the next
    /// pull, the cycle is deferred; after the release the next failure is the
    /// fourth step (300 s), not the first (15 s).
    #[test]
    fn a_deferred_cycle_leaves_the_ladder_where_it_was() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            for _ in 0..3 {
                rig.round(true);
            }
            rig.sync.ask(Reason::Tick);
            fire(&rig.timers);
            rig.sync.hold();
            rig.end(false);
            assert_eq!(rig.status(), SyncStatus::Idle, "deferred");
            rig.sync.release();
            assert_eq!(rig.armed().len(), 1, "asked again at the release");
            fire(&rig.timers);
            rig.end(true);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![secs(300)], "the fourth step");
        });
    }

    /// F12, design: "the online event resets [the backoff] and asks": an
    /// Online that lands during a failing cycle resets the ladder when the
    /// cycle ends: the one more waits 15 s, and the next failure is the
    /// first step again (15 s, not 30 s). The pacer's own tests pin the
    /// rule; this row pins that the gate passes the reason through.
    #[test]
    fn an_online_during_a_failing_cycle_restarts_the_ladder() {
        Owner::new().with(|| {
            let rig = Rig::signed_in();
            for _ in 0..3 {
                rig.round(true);
            }
            rig.sync.ask(Reason::Tick);
            fire(&rig.timers);
            rig.sync.ask(Reason::Online);
            rig.end(true);
            assert_eq!(rig.armed(), vec![MIN_GAP], "15 s, not 300 s");
            fire(&rig.timers);
            rig.end(true);
            rig.sync.ask(Reason::Tick);
            assert_eq!(rig.armed(), vec![MIN_GAP], "the first step, not 30 s");
        });
    }

    // ---- o: round 2 (G3) ----

    /// G3, design: "an 'again' set during a cycle runs after `MIN_GAP`" and
    /// "held by a session until release": a cycle that stood down for a
    /// session ends `Deferred`, which clears the pacer's 'again' (the
    /// gate's `pending` carries the ask), so the release arms ONE timer and
    /// ONE more pull follows, not two cycles. The ask that set 'again' is a
    /// learner's save, and a second save lands during the session, before
    /// or after the pull's answer. Mutation: `Outcome::Deferred` leaving
    /// 'again' set arms a second timer after the first extra cycle. Does
    /// not catch: a release that asks twice but whose second timer the
    /// pacer absorbs (the pull count and the armed list still hold).
    #[test]
    fn a_deferred_end_with_again_and_pending_gives_exactly_one_cycle() {
        // None, before the pull's answer, after it (the session still on).
        for save_in_session in [None, Some(true), Some(false)] {
            Owner::new().with(|| {
                let rig = Rig::signed_in();
                rig.sync.run();
                rig.sync.ask(Reason::Saved);
                rig.sync.hold();
                if save_in_session == Some(true) {
                    rig.sync.ask(Reason::Saved);
                }
                rig.end(false);
                if save_in_session == Some(false) {
                    rig.sync.ask(Reason::Saved);
                }
                let row = format!("{save_in_session:?}");
                assert_eq!(rig.status(), SyncStatus::Idle, "{row}: deferred");
                assert_eq!(rig.armed().len(), 0, "{row}: a deferred end arms nothing");
                assert_eq!(rig.transport.pull_calls(), 1, "{row}");
                rig.sync.release();
                assert_eq!(rig.armed().len(), 1, "{row}: one timer, not two");
                fire(&rig.timers);
                assert_eq!(rig.transport.pull_calls(), 2, "{row}: one more pull");
                rig.end(false);
                assert_eq!(rig.armed().len(), 0, "{row}: and no more");
                assert_eq!(rig.transport.pull_calls(), 2, "{row}");
            });
        }
    }
}
