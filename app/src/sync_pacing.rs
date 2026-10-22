//! The pacing of the automatic sync cycles (task 059): a pure state machine,
//! no Leptos, no clock of its own. [`SyncCycle`](crate::sync::SyncCycle)
//! calls it with the monotonic time (milliseconds) and does what the
//! returned [`Act`] says; this module only decides WHEN a cycle may start.
//!
//! - **Reasons.** Every automatic trigger is a [`Reason`]; [`Pacer::ask`]
//!   coalesces them: at most one trailing timer is armed, any ask while it
//!   is armed is absorbed, an ask during a cycle sets 'again'.
//! - **Spacing.** A cycle starts at the last cycle's end plus
//!   `max(`[`MIN_GAP`]`, the backoff step)`; the backoff steps are
//!   [`BACKOFF`] (10 s, 30 s, 1 min, 5 min cap) after real failures, reset
//!   by a success and by [`Reason::Online`]. A save waits at least
//!   [`SAVE_DEBOUNCE`].
//! - **Gestures** ([`Pacer::gesture`]: Sync now, Retry, Resume, sign-in)
//!   bypass the gap: they cancel the armed timer, reset the backoff and
//!   start at once.
//!
//! The gating on the account, the network, a session or a terminal state
//! belongs to the caller.

use std::time::Duration;

/// The shortest time between the end of a cycle and the start of the next
/// automatic one.
pub(crate) const MIN_GAP: Duration = Duration::from_secs(15);

/// The wait after the 1st, 2nd, 3rd and later real failures; the last is
/// the cap.
pub(crate) const BACKOFF: [Duration; 4] = [
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(300),
];

/// How long a saved change waits before its cycle.
pub(crate) const SAVE_DEBOUNCE: Duration = Duration::from_secs(10);

/// Why an automatic cycle is asked. Logged to the console by the caller,
/// for diagnosis only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The learner saved a change (not the cycle's own merge save).
    Saved,
    /// The page opened or became visible again.
    Visible,
    /// The 5-minute tick, while visible.
    Tick,
    /// The browser came back online: the backoff is reset.
    Online,
    /// Back home from a session during which something was asked.
    Home,
}

/// What a cycle ended with, for the backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Both sides equal, or a terminal state: the ladder is reset.
    Success,
    /// A real failure (network, Drive, storage): one step up the ladder.
    Failure,
    /// The cycle did no I/O (a session started during the pull and the
    /// cycle stood down): the ladder and the last end stay as they were.
    Deferred,
}

/// What the caller does after telling the pacer something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    /// Nothing: the ask was absorbed, ignored or is waiting.
    Nothing,
    /// Start a cycle now.
    Start,
    /// Arm one timer of `delay`, then call [`Pacer::fired`] with `id`.
    Arm {
        /// The timer's identity: a timer of another id is stale.
        id: u64,
        /// How long.
        delay: Duration,
    },
}

/// The state of the gate: when the last cycle ended, how many real
/// failures in a row, what is armed.
#[derive(Debug, Default)]
pub(crate) struct Pacer {
    /// Monotonic milliseconds at the last cycle's end.
    last_end: Option<f64>,
    /// Real failures in a row.
    failures: usize,
    /// A cycle is running.
    in_flight: bool,
    /// An ask came during the running cycle: one more after it.
    again: bool,
    /// An Online came during the running cycle: the ladder restarts at its
    /// end whatever it ends with.
    online_seen: bool,
    /// The armed timer's id.
    armed: Option<u64>,
    /// The next timer id.
    next_id: u64,
}

impl Pacer {
    /// A pacer that has seen nothing.
    pub(crate) fn new() -> Self {
        Pacer::default()
    }

    /// The wait between a cycle's end and the next automatic start:
    /// `max(`[`MIN_GAP`]`, the backoff step)`.
    fn wait(&self) -> Duration {
        let step = match self.failures {
            0 => Duration::ZERO,
            n => BACKOFF[n.min(BACKOFF.len()) - 1],
        };
        MIN_GAP.max(step)
    }

    /// The time left at `now` before the gap since the last end has passed
    /// (zero when it has, or when no cycle ended yet).
    fn delay(&self, now: f64) -> Duration {
        let Some(end) = self.last_end else {
            return Duration::ZERO;
        };
        let left_ms = end + self.wait().as_secs_f64() * 1000.0 - now;
        Duration::from_secs_f64(left_ms.max(0.0) / 1000.0)
    }

    /// Arms a new timer of `delay`: the old id, if any, is stale from now.
    fn arm(&mut self, delay: Duration) -> Act {
        let id = self.next_id;
        self.next_id += 1;
        self.armed = Some(id);
        Act::Arm { id, delay }
    }

    /// An automatic ask at monotonic time `now` (ms).
    ///
    /// Never starts a cycle itself: when the gap has already passed it arms
    /// a zero-delay timer, so that a burst of asks is absorbed.
    pub(crate) fn ask(&mut self, reason: Reason, now: f64) -> Act {
        if reason == Reason::Online {
            // The network is back: the ladder starts over, and a timer
            // armed at the old backoff is replaced by one at the new delay.
            // A cycle already running was started before it: whatever it
            // ends with, the ladder restarts at its end too.
            self.failures = 0;
            self.armed = None;
            self.online_seen = self.in_flight;
        }
        if self.in_flight {
            self.again = true;
            return Act::Nothing;
        }
        if self.armed.is_some() {
            return Act::Nothing;
        }
        let mut delay = self.delay(now);
        if reason == Reason::Saved {
            delay = delay.max(SAVE_DEBOUNCE);
        }
        self.arm(delay)
    }

    /// A gesture: starts at once, cancels the armed timer, resets the
    /// backoff; during a cycle it sets 'again' instead.
    pub(crate) fn gesture(&mut self) -> Act {
        self.armed = None;
        self.failures = 0;
        self.start_or_again()
    }

    /// The timer `id` ran out at `now`.
    ///
    /// The start time is checked here too, not only when the timer was
    /// armed: a timer that runs out inside the gap (or inside a backoff
    /// step) arms a new one for what is left and starts nothing.
    pub(crate) fn fired(&mut self, id: u64, now: f64) -> Act {
        if self.armed != Some(id) {
            return Act::Nothing;
        }
        self.armed = None;
        let left = self.delay(now);
        if !left.is_zero() && !self.in_flight {
            return self.arm(left);
        }
        self.start_or_again()
    }

    /// Starts a cycle, or, when one runs, asks for one more after it.
    fn start_or_again(&mut self) -> Act {
        if self.in_flight {
            self.again = true;
            Act::Nothing
        } else {
            self.in_flight = true;
            Act::Start
        }
    }

    /// The running cycle ended at `now`.
    ///
    /// An ask made during the cycle arms the one trailing timer at the full
    /// wait (the gap after a success, the backoff after a failure): it is
    /// never dropped and never immediate.
    pub(crate) fn finished(&mut self, outcome: Outcome, now: f64) -> Act {
        if !self.in_flight {
            return Act::Nothing;
        }
        self.in_flight = false;
        if outcome == Outcome::Deferred {
            // No I/O happened: nothing to wait for, nothing learned. The
            // ask that came meanwhile is the gate's pending flag.
            self.again = false;
            self.online_seen = false;
            return Act::Nothing;
        }
        self.last_end = Some(now);
        self.settle(outcome == Outcome::Failure);
        if std::mem::take(&mut self.again) {
            let wait = self.wait();
            self.arm(wait)
        } else {
            Act::Nothing
        }
    }

    /// Moves the ladder for a cycle that did I/O: a real failure is one
    /// step up, anything else resets it, and so does an Online that came
    /// during the cycle, whatever it ended with.
    fn settle(&mut self, failed: bool) {
        if std::mem::take(&mut self.online_seen) || !failed {
            self.failures = 0;
        } else {
            self.failures += 1;
        }
    }

    /// Forgets everything (sign-out): no flag, no backoff, nothing armed,
    /// no cycle running. The ids stay increasing, so a timer armed before
    /// the reset can never fire after it.
    pub(crate) fn reset(&mut self) {
        *self = Pacer {
            next_id: self.next_id,
            ..Pacer::default()
        };
    }

    /// Drops the armed timer and the pending 'again' (a gate that blocks the
    /// timer's start). The ladder and the running cycle are left alone.
    pub(crate) fn cancel(&mut self) {
        self.armed = None;
        self.again = false;
    }

    /// Whether the timer `id` is the one armed (a stale or fired id is not).
    pub(crate) fn is_armed(&self, id: u64) -> bool {
        self.armed == Some(id)
    }

    /// Whether a cycle runs, as the pacer knows it.
    pub(crate) fn in_flight(&self) -> bool {
        self.in_flight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// `delay` in milliseconds: a timer fires when its delay has passed.
    fn ms(delay: Duration) -> f64 {
        delay.as_secs_f64() * 1000.0
    }

    /// The `(id, delay)` of an [`Act::Arm`]; panics with the act otherwise.
    fn arm(act: Act) -> (u64, Duration) {
        match act {
            Act::Arm { id, delay } => (id, delay),
            other => panic!("expected an Arm, got {other:?}"),
        }
    }

    /// The timer `id` ran out at `now`: named for what it asserts, that the
    /// timer really waited until `now` (a timer never runs out earlier than
    /// the delay it was armed with, unless a test means it to).
    fn fire_at(p: &mut Pacer, id: u64, now: f64) -> Act {
        p.fired(id, now)
    }

    /// One whole cycle asked at `at` and driven the way the gate does: ask,
    /// the timer fires after its delay, the cycle runs (it takes no time),
    /// it finishes with `outcome`. The pacer must be idle (nothing armed,
    /// nothing running). Returns the time the cycle ended, `at` plus the
    /// delay: the next ask is made from there, never before.
    fn run_cycle(p: &mut Pacer, outcome: Outcome, at: f64) -> f64 {
        let (id, delay) = arm(p.ask(Reason::Tick, at));
        let end = at + ms(delay);
        assert_eq!(fire_at(p, id, end), Act::Start);
        assert_eq!(p.finished(outcome, end), Act::Nothing);
        end
    }

    /// A pacer whose first cycle succeeded at `end` (nothing came before,
    /// so the timer had no delay and the cycle ended when it was asked).
    fn after_success(end: f64) -> Pacer {
        let mut p = Pacer::new();
        assert_eq!(run_cycle(&mut p, Outcome::Success, end), end);
        p
    }

    /// A pacer with `n` failed cycles in a row, the first asked at `at`;
    /// and the time the last one ended (each waited its step after the one
    /// before).
    fn after_failures(n: usize, at: f64) -> (Pacer, f64) {
        let mut p = Pacer::new();
        let mut end = at;
        for _ in 0..n {
            end = run_cycle(&mut p, Outcome::Failure, end);
        }
        (p, end)
    }

    /// A pacer with a cycle running (started by a timer), nothing armed.
    /// Returns the time the cycle started, `at` plus the delay: what the
    /// cycle does after that is timed from there.
    fn running(p: &mut Pacer, at: f64) -> f64 {
        let (id, delay) = arm(p.ask(Reason::Tick, at));
        let start = at + ms(delay);
        assert_eq!(fire_at(p, id, start), Act::Start);
        assert!(p.in_flight());
        start
    }

    #[test]
    fn first_ask_arms_a_zero_delay_timer_never_starts() {
        // "an ask when the gap has already passed arms a zero-delay timer"
        for reason in [Reason::Visible, Reason::Tick, Reason::Online, Reason::Home] {
            let (_, delay) = arm(Pacer::new().ask(reason, 1000.0));
            assert_eq!(delay, secs(0), "{reason:?}");
        }
        let (_, delay) = arm(Pacer::new().ask(Reason::Saved, 1000.0));
        assert_eq!(delay, SAVE_DEBOUNCE);
    }

    #[test]
    fn a_wake_burst_is_absorbed_by_the_armed_timer() {
        let mut p = Pacer::new();
        arm(p.ask(Reason::Visible, 1000.0));
        assert_eq!(p.ask(Reason::Tick, 1000.0), Act::Nothing);
        assert_eq!(p.ask(Reason::Home, 1001.0), Act::Nothing);
        assert_eq!(p.ask(Reason::Saved, 1002.0), Act::Nothing);
    }

    #[test]
    fn a_fired_timer_starts_once_and_a_stale_id_is_nothing() {
        let mut p = Pacer::new();
        let (id, _) = arm(p.ask(Reason::Visible, 1000.0));
        assert!(!p.in_flight());
        assert_eq!(p.fired(id + 1000, 1000.0), Act::Nothing, "unknown id");
        assert!(!p.in_flight());
        assert_eq!(p.fired(id, 1000.0), Act::Start);
        assert!(p.in_flight());
        assert_eq!(p.fired(id, 1000.0), Act::Nothing, "the same id twice");
    }

    #[test]
    fn an_ask_during_a_cycle_runs_one_more_after_the_gap() {
        let mut p = Pacer::new();
        running(&mut p, 1000.0);
        assert_eq!(p.ask(Reason::Saved, 2000.0), Act::Nothing);
        assert_eq!(p.ask(Reason::Tick, 2001.0), Act::Nothing, "one 'again'");
        let (_, delay) = arm(p.finished(Outcome::Success, 3000.0));
        assert_eq!(delay, MIN_GAP);
        assert!(!p.in_flight());
    }

    #[test]
    fn a_cycle_nobody_asked_during_ends_with_nothing() {
        let mut p = Pacer::new();
        running(&mut p, 1000.0);
        assert_eq!(p.finished(Outcome::Success, 3000.0), Act::Nothing);
        // 'again' was consumed: the next cycle's end does not re-arm.
        running(&mut p, 30000.0);
        assert_eq!(p.finished(Outcome::Success, 31000.0), Act::Nothing);
    }

    #[test]
    fn the_start_is_the_last_end_plus_the_gap() {
        let mut p = after_success(20_000.0);
        assert_eq!(arm(p.ask(Reason::Tick, 25_000.0)).1, secs(10));
        assert_eq!(
            arm(after_success(20_000.0).ask(Reason::Tick, 35_000.0)).1,
            secs(0)
        );
        assert_eq!(
            arm(after_success(20_000.0).ask(Reason::Tick, 99_999.0)).1,
            secs(0)
        );
        // A fractional remainder is kept.
        let d = arm(after_success(20_000.0).ask(Reason::Tick, 34_500.0)).1;
        assert_eq!(d, Duration::from_millis(500));
    }

    #[test]
    fn the_backoff_ladder_is_the_gap_then_30_60_300_300() {
        // max(MIN_GAP, step): the first step (10 s) is below the gap.
        let (mut p, mut end) = after_failures(1, 1000.0);
        // Every ask is made at the instant the last cycle ended.
        for (i, expected) in [15, 30, 60, 300, 300].into_iter().enumerate() {
            let (id, delay) = arm(p.ask(Reason::Tick, end));
            assert_eq!(delay, secs(expected), "after failure {}", i + 1);
            end += ms(delay);
            assert_eq!(fire_at(&mut p, id, end), Act::Start);
            assert_eq!(p.finished(Outcome::Failure, end), Act::Nothing);
        }
    }

    #[test]
    fn a_success_resets_the_ladder() {
        let (mut p, end) = after_failures(3, 1000.0);
        let end = run_cycle(&mut p, Outcome::Success, end);
        assert_eq!(arm(p.ask(Reason::Tick, end)).1, MIN_GAP);
    }

    #[test]
    fn an_again_after_a_failure_waits_the_backoff_not_the_gap() {
        // Two earlier failures; a third during which something was asked.
        let (mut p, end) = after_failures(2, 1000.0);
        let start = running(&mut p, end);
        assert_eq!(p.ask(Reason::Saved, start + 500.0), Act::Nothing);
        let act = p.finished(Outcome::Failure, start + 1000.0);
        assert_eq!(arm(act).1, secs(60), "never Start, never 15 s: {act:?}");
        // The same with a success earlier: the gap.
        let (mut p, end) = after_failures(2, 1000.0);
        let end = run_cycle(&mut p, Outcome::Success, end);
        let start = running(&mut p, end);
        assert_eq!(p.ask(Reason::Saved, start + 500.0), Act::Nothing);
        let act = p.finished(Outcome::Success, start + 1000.0);
        assert_eq!(arm(act).1, MIN_GAP);
    }

    #[test]
    fn the_again_timer_is_the_one_trailing_timer() {
        let mut p = Pacer::new();
        running(&mut p, 1000.0);
        assert_eq!(p.ask(Reason::Tick, 1500.0), Act::Nothing);
        let (id, _) = arm(p.finished(Outcome::Success, 2000.0));
        // It is armed: a further ask is absorbed, its id fires once.
        assert_eq!(p.ask(Reason::Visible, 2001.0), Act::Nothing);
        assert_eq!(p.fired(id, 17_000.0), Act::Start);
    }

    #[test]
    fn a_saved_change_waits_at_least_the_debounce() {
        // Gap passed: the debounce.
        assert_eq!(
            arm(after_success(20_000.0).ask(Reason::Saved, 99_000.0)).1,
            SAVE_DEBOUNCE
        );
        // 12 s of gap remaining: the larger.
        assert_eq!(
            arm(after_success(20_000.0).ask(Reason::Saved, 23_000.0)).1,
            secs(12)
        );
        // 3 s of gap remaining: the debounce.
        assert_eq!(
            arm(after_success(20_000.0).ask(Reason::Saved, 32_000.0)).1,
            SAVE_DEBOUNCE
        );
    }

    #[test]
    fn a_gesture_starts_at_once_and_resets_the_ladder() {
        let (mut p, end) = after_failures(3, 1000.0);
        assert_eq!(p.gesture(), Act::Start);
        assert!(p.in_flight());
        // One failure after the reset is step 1 (15 s), not the 5th (300 s).
        assert_eq!(p.finished(Outcome::Failure, end), Act::Nothing);
        assert_eq!(arm(p.ask(Reason::Tick, end)).1, MIN_GAP);
    }

    #[test]
    fn a_gesture_cancels_the_armed_timer() {
        let mut p = Pacer::new();
        let (id, _) = arm(p.ask(Reason::Visible, 1000.0));
        assert_eq!(p.gesture(), Act::Start);
        assert_eq!(p.fired(id, 1000.0), Act::Nothing, "cancelled");
        assert!(p.in_flight());
        // Armed was cleared: after the cycle an ask arms again.
        assert_eq!(p.finished(Outcome::Success, 2000.0), Act::Nothing);
        arm(p.ask(Reason::Tick, 2000.0));
    }

    #[test]
    fn a_gesture_during_a_cycle_runs_one_more_after_the_gap() {
        let mut p = Pacer::new();
        running(&mut p, 1000.0);
        assert_eq!(p.gesture(), Act::Nothing);
        assert_eq!(arm(p.finished(Outcome::Success, 2000.0)).1, MIN_GAP);
    }

    #[test]
    fn online_resets_the_ladder_and_asks_without_starting() {
        let (mut p, end) = after_failures(3, 1000.0);
        assert_eq!(arm(p.ask(Reason::Online, end)).1, MIN_GAP, "not 60 s");
        // With a 300 s timer armed, Online re-arms with the fresh delay.
        let (mut p, end) = after_failures(4, 1000.0);
        let (old, delay) = arm(p.ask(Reason::Tick, end));
        assert_eq!(delay, secs(300));
        let (new, delay) = arm(p.ask(Reason::Online, end));
        assert_eq!(delay, MIN_GAP);
        assert_ne!(new, old);
        assert_eq!(
            fire_at(&mut p, old, end),
            Act::Nothing,
            "the old timer is stale"
        );
        assert_eq!(fire_at(&mut p, new, end + ms(delay)), Act::Start);
    }

    #[test]
    fn online_during_a_cycle_resets_the_ladder_and_sets_again() {
        let (mut p, end) = after_failures(3, 1000.0);
        let start = running(&mut p, end);
        assert_eq!(p.ask(Reason::Online, start + 100.0), Act::Nothing);
        // Failures were reset to 0, then this one failure is step 1.
        let act = p.finished(Outcome::Failure, start + 200.0);
        assert_eq!(arm(act).1, MIN_GAP);
    }

    /// F12 (review round 1): an Online that lands during a cycle that then
    /// fails restarts the ladder, so the failure after it is step 1 again
    /// (15 s), not step 2 (30 s). Mutation not caught: resetting only at
    /// the ask (the old code) is caught here; resetting at the end but not
    /// at the ask is caught by the test above.
    #[test]
    fn online_during_a_failing_cycle_restarts_the_ladder_at_its_end() {
        let (mut p, end) = after_failures(3, 1000.0);
        let start = running(&mut p, end);
        assert_eq!(p.ask(Reason::Online, start + 100.0), Act::Nothing);
        let ended = start + 200.0;
        let (id, delay) = arm(p.finished(Outcome::Failure, ended));
        assert_eq!(delay, MIN_GAP, "the again after Online waits the gap");
        let second = ended + ms(delay);
        assert_eq!(fire_at(&mut p, id, second), Act::Start);
        // The next failure is the first of a new ladder.
        assert_eq!(p.finished(Outcome::Failure, second), Act::Nothing);
        assert_eq!(arm(p.ask(Reason::Tick, second)).1, MIN_GAP, "not 30 s");
    }

    /// F5: a cycle that stood down for a session did no I/O: it leaves the
    /// ladder as it was and its 'again' is the gate's pending flag, not the
    /// pacer's. Mutation: Deferred treated as Success resets the ladder
    /// (the last row).
    #[test]
    fn a_deferred_cycle_leaves_the_ladder_and_arms_nothing() {
        let (mut p, end) = after_failures(3, 1000.0);
        let (id, delay) = arm(p.ask(Reason::Tick, end));
        assert_eq!(delay, secs(60));
        let start = end + ms(delay);
        assert_eq!(fire_at(&mut p, id, start), Act::Start);
        assert_eq!(
            p.ask(Reason::Saved, start + 1000.0),
            Act::Nothing,
            "an again"
        );
        let deferred = start + 2000.0;
        assert_eq!(p.finished(Outcome::Deferred, deferred), Act::Nothing);
        assert!(!p.in_flight());
        // The ladder stands at 3: the next failure is step 4 (300 s). The
        // last end stood too, so the gap had passed and the ask has no wait.
        let (id, delay) = arm(p.ask(Reason::Tick, deferred));
        assert_eq!(delay, secs(0));
        assert_eq!(fire_at(&mut p, id, deferred), Act::Start);
        assert_eq!(p.finished(Outcome::Failure, deferred), Act::Nothing);
        assert_eq!(arm(p.ask(Reason::Tick, deferred)).1, secs(300));
    }

    /// F5: a deferred end keeps the last end too: a gap that had passed
    /// stays passed.
    #[test]
    fn a_deferred_cycle_keeps_the_last_end() {
        let mut p = after_success(1000.0);
        let (id, _) = arm(p.ask(Reason::Tick, 1000.0));
        assert_eq!(p.fired(id, 16_000.0), Act::Start);
        assert_eq!(p.finished(Outcome::Deferred, 90_000.0), Act::Nothing);
        assert_eq!(arm(p.ask(Reason::Tick, 90_000.0)).1, secs(0));
    }

    /// F8 (`Pacer::fired` uses `now`): "start time = last cycle's end plus the
    /// larger of the gap and the step" is checked when the timer fires, not only when it
    /// is armed: a timer that fires early re-arms for what is left of the
    /// gap and never starts inside it. Mutation: dropping the re-check
    /// (`let _ = now`) fails the first assertion.
    #[test]
    fn a_timer_that_fires_inside_the_gap_rearms_for_the_rest() {
        let mut p = after_success(1000.0);
        let (id, delay) = arm(p.ask(Reason::Tick, 1000.0));
        assert_eq!(delay, MIN_GAP);
        let (again, left) = arm(fire_at(&mut p, id, 5000.0));
        assert_eq!(left, secs(11), "what is left of the 15 s gap");
        assert_ne!(again, id);
        assert!(!p.in_flight(), "nothing started inside the gap");
        let stale = fire_at(&mut p, id, 16_000.0);
        assert_eq!(stale, Act::Nothing, "the early id is stale");
        assert_eq!(fire_at(&mut p, again, 16_000.0), Act::Start);
    }

    /// F8: the same inside a backoff step, and on time is a start.
    #[test]
    fn a_timer_on_time_starts_and_the_backoff_is_checked_too() {
        let (mut p, end) = after_failures(3, 1000.0);
        let (id, delay) = arm(p.ask(Reason::Tick, end));
        assert_eq!(delay, secs(60));
        let act = fire_at(&mut p, id, end + 30_000.0);
        assert_eq!(arm(act).1, secs(30), "inside the 60 s step");
        let (mut p, end) = after_failures(3, 1000.0);
        let (id, _) = arm(p.ask(Reason::Tick, end));
        assert_eq!(fire_at(&mut p, id, end + 60_000.0), Act::Start, "on time");
    }

    /// G2 (review round 2): a timer that runs out INSIDE a 60 s backoff step
    /// arms a new one for exactly what is left, which may itself run out
    /// early and re-arm for what is then left; the one that runs out on
    /// time starts the cycle, and every earlier id is stale. Design: "a
    /// timer that runs out inside the gap (or inside a backoff step) arms a
    /// new one for what is left and starts nothing". Mutation: `fired`
    /// without the gap re-check starts at the first early fire.
    #[test]
    fn a_timer_fired_inside_a_backoff_step_rearms_for_the_rest_then_starts() {
        let (mut p, end) = after_failures(3, 1000.0);
        let (id, delay) = arm(p.ask(Reason::Tick, end));
        assert_eq!(delay, secs(60));
        let (second, left) = arm(fire_at(&mut p, id, end + 45_000.0));
        assert_eq!(left, secs(15), "what is left of the 60 s step");
        assert!(!p.in_flight(), "nothing started inside the step");
        // The re-armed timer runs out early too: it re-arms for the rest.
        let (third, left) = arm(fire_at(&mut p, second, end + 50_000.0));
        assert_eq!(left, secs(10));
        assert!(!p.in_flight());
        // An ask meanwhile is absorbed by the one armed timer.
        assert_eq!(p.ask(Reason::Visible, end + 51_000.0), Act::Nothing);
        for stale in [id, second] {
            let act = fire_at(&mut p, stale, end + 60_000.0);
            assert_eq!(act, Act::Nothing, "stale {stale}");
        }
        assert_eq!(fire_at(&mut p, third, end + 60_000.0), Act::Start);
        assert!(p.in_flight());
    }

    /// F3 support: the gate asks whether a timer is the armed one before
    /// anything else.
    #[test]
    fn is_armed_is_true_for_the_armed_id_only() {
        let mut p = Pacer::new();
        assert!(!p.is_armed(0));
        let (id, _) = arm(p.ask(Reason::Visible, 1000.0));
        assert!(p.is_armed(id));
        assert!(!p.is_armed(id + 1));
        assert_eq!(p.fired(id, 1000.0), Act::Start);
        assert!(!p.is_armed(id), "a fired id is spent");
        assert_eq!(p.finished(Outcome::Success, 1000.0), Act::Nothing);
        let (next, _) = arm(p.ask(Reason::Saved, 1000.0));
        p.cancel();
        assert!(!p.is_armed(next), "a cancelled id is not armed");
    }

    #[test]
    fn reset_forgets_the_timer_the_cycle_and_the_ladder() {
        let (mut p, end) = after_failures(3, 1000.0);
        let (id, _) = arm(p.ask(Reason::Tick, end));
        p.reset();
        assert_eq!(fire_at(&mut p, id, end), Act::Nothing, "stale after reset");
        let (new, delay) = arm(p.ask(Reason::Tick, end));
        assert_eq!(delay, secs(0), "ladder and last end cleared");
        assert_ne!(new, id, "ids stay monotonic");
    }

    #[test]
    fn a_timer_armed_before_a_reset_never_fires_after_a_new_arm() {
        // The ids keep increasing across a reset, or the first timer of a
        // new session would share its id with the stale one.
        let mut p = Pacer::new();
        let (old, _) = arm(p.ask(Reason::Visible, 1000.0));
        p.reset();
        let (new, _) = arm(p.ask(Reason::Visible, 1000.0));
        assert_ne!(new, old);
        assert_eq!(p.fired(old, 1000.0), Act::Nothing);
        assert_eq!(p.fired(new, 1000.0), Act::Start);
    }

    #[test]
    fn reset_cuts_a_cycle_in_flight_and_its_late_answer_is_nothing() {
        let mut p = Pacer::new();
        running(&mut p, 1000.0);
        assert_eq!(p.ask(Reason::Saved, 1100.0), Act::Nothing);
        p.reset();
        assert!(!p.in_flight());
        assert_eq!(p.finished(Outcome::Success, 2000.0), Act::Nothing);
        // The late answer touched nothing: no last end, no 'again'.
        assert_eq!(arm(p.ask(Reason::Tick, 2001.0)).1, secs(0));
    }

    #[test]
    fn a_finish_nobody_started_is_nothing_and_changes_nothing() {
        let mut p = Pacer::new();
        assert_eq!(p.finished(Outcome::Failure, 5000.0), Act::Nothing);
        assert_eq!(p.finished(Outcome::Success, 5000.0), Act::Nothing);
        assert_eq!(arm(p.ask(Reason::Tick, 5001.0)).1, secs(0));
    }

    #[test]
    fn cancel_drops_the_armed_timer_and_the_again_and_keeps_the_ladder() {
        // Two failures on the ladder (30 s), a timer armed.
        let (mut p, end) = after_failures(2, 1000.0);
        let (id, _) = arm(p.ask(Reason::Tick, end));
        p.cancel();
        assert_eq!(fire_at(&mut p, id, end), Act::Nothing, "the stale id");
        let (new, delay) = arm(p.ask(Reason::Tick, end));
        assert_ne!(new, id);
        assert_eq!(delay, secs(30), "the ladder is left alone");
        // A running cycle stays running, but its 'again' is dropped.
        let mut p = Pacer::new();
        running(&mut p, 1000.0);
        assert_eq!(p.ask(Reason::Saved, 1100.0), Act::Nothing);
        p.cancel();
        assert!(p.in_flight());
        assert_eq!(p.finished(Outcome::Success, 2000.0), Act::Nothing);
    }

    /// Guards the designed values only; a wrong USE of them (the `max`, the
    /// step index) is caught by the gap, ladder and debounce tests, not here.
    #[test]
    fn the_constants_are_the_designed_ones() {
        assert_eq!(MIN_GAP, secs(15));
        assert_eq!(BACKOFF, [secs(10), secs(30), secs(60), secs(300)]);
        assert_eq!(SAVE_DEBOUNCE, secs(10));
    }
}
