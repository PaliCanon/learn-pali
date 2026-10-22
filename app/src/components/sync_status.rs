//! The home's sync status line (task 059): where the sync stands, in one
//! line at the bottom of the home (user, 2026-10-22), replacing the Sync
//! chip. [`SyncStatusLine`] renders
//! nothing without a transport (the host gave no sync) and nothing before
//! the cycle is mounted on the client, so the server's paint, which has no
//! transport, and the first hydration pass agree.
//!
//! What it says is decided by the pure [`line`] from the account, the
//! status, the stamp of the last finished cycle and the count (no clock:
//! the age is a signal of its own):
//!
//! | Line | Text | Button |
//! | --- | --- | --- |
//! | synced | 'Synced · 2 min ago' (+ ' · 3 terms from another device') | none |
//! | syncing | 'Syncing…' | none |
//! | offline | 'Offline. Sync resumes when you are back online.' | none |
//! | retrying | 'Not synced. Retrying automatically.' and the cause | Retry |
//! | terminal | 'Not synced' ('Update the app' for a newer Drive copy) | See Settings |
//! | paused | 'Sync paused' ('Allow pop-ups, then tap Resume' after a blocked window; also a refused token on a signed-in account) | Resume |
//! | signed out | 'Signed out. Sign in again in Settings' | none |
//!
//! The age comes from the wall-clock stamp taken when the cycle ended in
//! this tab ([`SyncCycle::synced_at`]), clamped at 0, and is refreshed each
//! minute (no stamp, no age); it and the count live in memory only. The
//! refresh moves the age's text alone: the label and its button are never
//! rebuilt by it. The state label is the
//! announced region (`role="status"`, or `role="alert"` for a failure); the
//! age sits outside it, so the minute refresh is never announced. The
//! count is a region of its own, so its change is. On a phone the line is
//! one line: the labels truncate and the button never does.
//!
//! Every minute the line also re-reads the account against the clock
//! ([`SyncCycle::check_account`]: the web token's expiry turns the line to
//! 'Sync paused'). Native builds never tick.

use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, ShellState, Variant};

use crate::backend::{SyncAccount, SyncError};
use crate::catalogue::{Locale, strings};
use crate::components::sync_panel::error_reason;
use crate::sync::{SyncCycle, SyncStatus, wall_now_ms};

/// A minute, in milliseconds.
const MINUTE_MS: f64 = 60_000.0;

/// An hour, in milliseconds.
const HOUR_MS: f64 = 60.0 * MINUTE_MS;

/// A day, in milliseconds.
const DAY_MS: f64 = 24.0 * HOUR_MS;

/// Which terminal state: no automatic cycle until Sync now or a reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Terminal {
    /// This device's progress or the Drive copy is not usable: Settings
    /// says which.
    Settings,
    /// The Drive copy was saved by a newer version of the app.
    UpdateApp,
}

/// What the home's line says (see the module doc for the texts).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Line {
    /// Nothing to say.
    Hidden,
    /// The account is remembered with no valid token; `popup` when the
    /// window of the last tap was blocked.
    Paused {
        /// The browser blocked Google's window.
        popup: bool,
    },
    /// The web token was refused and no sign-in is at hand.
    SignedOutAgain,
    /// A cycle runs.
    Syncing,
    /// The browser is offline.
    Offline,
    /// The last cycle ended with both sides equal.
    Synced {
        /// Terms the last merging cycle took from another device.
        from_other: usize,
    },
    /// The last cycle failed; it is retried by itself.
    Retrying(SyncError),
    /// A terminal state: no automatic cycle.
    NotSynced(Terminal),
}

/// How long ago `synced_at` was, at `now` (wall-clock milliseconds):
/// clamped at 0, so a clock that stepped back never reads negative; none
/// without a stamp (the line then shows no age at all).
fn age_since(now: f64, synced_at: Option<f64>) -> Option<f64> {
    synced_at.map(|at| (now - at).max(0.0))
}

/// What the line says for this `account` and `status`, `synced_at` the
/// stamp of the last finished cycle (wall-clock milliseconds), `from_other`
/// the count of terms taken from another device. The clock is not an input:
/// the age is its own signal, so the minute refresh never rebuilds the
/// line.
pub(crate) fn line(
    account: &SyncAccount,
    status: &SyncStatus,
    synced_at: Option<f64>,
    from_other: usize,
) -> Line {
    match account {
        SyncAccount::SignedOut => {
            if *status == SyncStatus::Failed(SyncError::SignedOut) {
                Line::SignedOutAgain
            } else {
                Line::Hidden
            }
        }
        SyncAccount::Remembered { .. } => Line::Paused {
            popup: *status == SyncStatus::Failed(SyncError::Popup),
        },
        SyncAccount::SignedIn { .. } => signed_in_line(status, synced_at, from_other),
    }
}

/// The line of a signed-in account.
fn signed_in_line(status: &SyncStatus, synced_at: Option<f64>, from_other: usize) -> Line {
    let synced = || Line::Synced { from_other };
    match status {
        SyncStatus::Idle if synced_at.is_none() => Line::Hidden,
        SyncStatus::Idle | SyncStatus::Synced { .. } => synced(),
        SyncStatus::Syncing => Line::Syncing,
        SyncStatus::Offline => Line::Offline,
        // Safety net, normally unreachable: the web transport turns the
        // account Remembered on a 401 (so `line` takes the Remembered arm
        // and never gets here). This arm covers a transport that reports
        // the 401 without moving the account: a signed-in account still
        // carrying the refusal reads as the same paused state, never as
        // 'Retrying automatically'. The desktop's refused token reaches
        // the line as `SignedOut` (tested in the app repo).
        SyncStatus::Failed(SyncError::SignedOut) => Line::Paused { popup: false },
        SyncStatus::Failed(error) => Line::Retrying(error.clone()),
        SyncStatus::Refused(_) | SyncStatus::DriveUnreadable { .. } => {
            Line::NotSynced(Terminal::Settings)
        }
        SyncStatus::DriveNewer => Line::NotSynced(Terminal::UpdateApp),
    }
}

/// An age in words: 'just now' under a minute (and for a negative, NaN or
/// infinite age), then whole minutes, hours and days, rounded down.
pub(crate) fn age_label(locale: Locale, age_ms: f64) -> String {
    if !age_ms.is_finite() || age_ms < MINUTE_MS {
        return td_string!(locale, sync_age_now).to_string();
    }
    if age_ms < HOUR_MS {
        let count = (age_ms / MINUTE_MS).floor();
        td_string!(locale, sync_age_minutes, count = count).to_string()
    } else if age_ms < DAY_MS {
        let count = (age_ms / HOUR_MS).floor();
        td_string!(locale, sync_age_hours, count = count).to_string()
    } else {
        let count = (age_ms / DAY_MS).floor();
        td_string!(locale, sync_age_days, count = count).to_string()
    }
}

/// Whether `count` takes the singular string in `locale`: French reads 0
/// and 1 as singular, English and Vietnamese only 1 (the line passes only
/// counts above 0, but the rule holds for 0 too).
fn singular(locale: Locale, count: usize) -> bool {
    match locale {
        Locale::fr => count < 2,
        Locale::en | Locale::vi => count == 1,
    }
}

/// 'N term(s) from another device'.
fn from_other_label(locale: Locale, count: usize) -> String {
    if singular(locale, count) {
        td_string!(locale, sync_from_other_1, count = count).to_string()
    } else {
        td_string!(locale, sync_from_other_n, count = count).to_string()
    }
}

/// 'See Settings': opens Settings (`settings_open` is the shell's signal;
/// none: a host without the shell, where there is nothing to open).
pub(crate) fn open_settings(settings_open: Option<RwSignal<bool>>) {
    if let Some(open) = settings_open {
        open.set(true);
    }
}

/// The status line; nothing without a transport or before the cycle is
/// mounted.
#[component]
pub(crate) fn SyncStatusLine() -> impl IntoView {
    let sync = SyncCycle::current();
    move || {
        let sync = sync.filter(|sync| sync.mounted())?;
        Some(view! { <StatusLine sync=sync /> })
    }
}

/// The line of a mounted cycle.
#[component]
fn StatusLine(
    /// The page's sync.
    sync: SyncCycle,
) -> impl IntoView {
    let now = RwSignal::new(wall_now_ms());
    start_minute_tick(sync, now);
    let settings_open = use_context::<ShellState>().map(|shell| shell.settings_open);
    let (account, status) = (sync.account(), sync.status());
    let (synced_at, from_other) = (sync.synced_at(), sync.terms_from_other());
    let shape = shape_memo(account, status, synced_at, from_other);
    let age = age_signal(now, synced_at);
    // Paused: Google's script is readied for the Resume tap.
    Effect::new(move |_| {
        if matches!(shape.get(), Line::Paused { .. }) {
            sync.prepare();
        }
    });
    move || line_view(sync, settings_open, shape.get(), age)
}

/// What the line says, rebuilt only when the account, the status, the stamp
/// or the count change: it reads no clock.
fn shape_memo(
    account: Signal<SyncAccount>,
    status: Signal<SyncStatus>,
    synced_at: Signal<Option<f64>>,
    from_other: Signal<usize>,
) -> Memo<Line> {
    Memo::new(move |_| {
        line(
            &account.get(),
            &status.get(),
            synced_at.get(),
            from_other.get(),
        )
    })
}

/// The age of the last cycle, read against `now`: the only reader of the
/// minute clock besides the account check.
fn age_signal(now: RwSignal<f64>, synced_at: Signal<Option<f64>>) -> Signal<Option<f64>> {
    Signal::derive(move || age_since(now.get(), synced_at.get()))
}

/// Every minute: the age is read again and the account is re-read against
/// the clock. Never on native builds.
#[cfg(feature = "browser")]
fn start_minute_tick(sync: SyncCycle, now: RwSignal<f64>) {
    let refresh = move || {
        now.set(wall_now_ms());
        sync.check_account();
    };
    if let Ok(handle) = set_interval_with_handle(refresh, std::time::Duration::from_secs(60)) {
        on_cleanup(move || handle.clear());
    }
}

/// Native builds never tick.
#[cfg(not(feature = "browser"))]
fn start_minute_tick(sync: SyncCycle, now: RwSignal<f64>) {
    let _ = (sync, now);
}

/// The markup of a line; none for a hidden one.
fn line_view(
    sync: SyncCycle,
    settings_open: Option<RwSignal<bool>>,
    line: Line,
    age: Signal<Option<f64>>,
) -> Option<AnyView> {
    Some(match line {
        Line::Hidden => return None,
        Line::Synced { from_other } => synced_view(from_other, age),
        Line::Syncing => frame(label("status", |l| td_string!(l, sync_syncing).to_string())),
        Line::Offline => frame(label("status", |l| td_string!(l, sync_offline).to_string())),
        Line::SignedOutAgain => frame(label("alert", |l| {
            td_string!(l, sync_signed_out_again).to_string()
        })),
        Line::Retrying(error) => frame(view! {
            {label(
                "alert",
                move |l| { format!("{} {}", td_string!(l, sync_retrying), error_reason(l, &error)) },
            )}
            {action(|l| td_string!(l, sync_retry), move || sync.sync_now())}
        }),
        Line::Paused { popup } => frame(view! {
            {label("status", move |l| paused_text(l, popup))}
            {action(|l| td_string!(l, sync_resume), move || sync.resume())}
        }),
        Line::NotSynced(terminal) => frame(view! {
            {label("alert", move |l| terminal_text(l, terminal))}
            {action(|l| td_string!(l, sync_see_settings), move || open_settings(settings_open))}
        }),
    })
}

/// The text of 'Sync paused', with the way out after a blocked window.
fn paused_text(locale: Locale, popup: bool) -> String {
    if popup {
        format!(
            "{} · {}",
            td_string!(locale, sync_paused),
            td_string!(locale, sync_paused_popup)
        )
    } else {
        td_string!(locale, sync_paused).to_string()
    }
}

/// The text of a terminal state: a newer Drive copy says to update.
fn terminal_text(locale: Locale, terminal: Terminal) -> String {
    match terminal {
        Terminal::Settings => td_string!(locale, sync_line_not_synced).to_string(),
        Terminal::UpdateApp => format!(
            "{} · {}",
            td_string!(locale, sync_line_not_synced),
            td_string!(locale, sync_update_app)
        ),
    }
}

/// The one line: it may shrink (`min-w-0`), centred like the chip was, with
/// room above it (`mt-6`): it closes the home's column.
fn frame(children: impl IntoView + 'static) -> AnyView {
    view! {
        <div
            data-sync-line=""
            class="mt-6 flex min-w-0 items-center justify-center gap-2 text-sm text-muted"
        >
            {children}
        </div>
    }
    .into_any()
}

/// The state's label, a live region of the given `role`; it truncates.
fn label(
    role: &'static str,
    text: impl Fn(Locale) -> String + Send + Sync + 'static,
) -> impl IntoView {
    let t = strings();
    view! {
        <span role=role class="min-w-0 truncate">
            {move || text(t.locale())}
        </span>
    }
}

/// The line's button: a chip that never shrinks.
fn action(
    text: fn(Locale) -> &'static str,
    on_click: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    let t = strings();
    view! {
        <Button variant=Variant::Chip layout="shrink-0" on_click=move |_| on_click()>
            {move || text(t.locale())}
        </Button>
    }
}

/// 'Synced', the age outside any live region (none without a stamp), and
/// the count in a region of its own.
fn synced_view(from_other: usize, age: Signal<Option<f64>>) -> AnyView {
    frame(view! {
        {label("status", |l| td_string!(l, sync_line_synced).to_string())}
        {age_view(age)}
        {(from_other > 0)
            .then(|| {
                view! {
                    " · "
                    {label("status", move |l| from_other_label(l, from_other))}
                }
            })}
    })
}

/// ' · 2 min ago', its own reactive part of the line: present only with a
/// stamp, and only its text moves with the minute tick.
fn age_view(age: Signal<Option<f64>>) -> impl IntoView {
    let t = strings();
    let stamped = Memo::new(move |_| age.get().is_some());
    move || {
        stamped.get().then(|| {
            view! {
                " · "
                <span data-sync-age="" class="shrink-0 whitespace-nowrap">
                    {move || age_label(t.locale(), age.get().unwrap_or(0.0))}
                </span>
            }
        })
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use palicanon_ui_lib::Language;

    use super::*;
    use crate::backend::{Backend, SharedSync};
    use crate::catalogue::Locale;
    use crate::store::Load;
    use crate::sync::{Refusal, SyncCycle, wall_now_ms};
    use crate::test_support::{FakeTransport, provide_fakes, tag_around};

    const MINUTE: f64 = 60_000.0;
    const HOUR: f64 = 60.0 * MINUTE;

    fn signed_in() -> SyncAccount {
        SyncAccount::SignedIn {
            email: Some("learner@example.org".into()),
        }
    }

    fn remembered() -> SyncAccount {
        SyncAccount::Remembered { email: None }
    }

    fn synced_status() -> SyncStatus {
        SyncStatus::Synced { at: "10:42".into() }
    }

    // ---- The pure core ----

    /// Every status, for the accounts that ignore most of them.
    fn every_status() -> [SyncStatus; 7] {
        [
            SyncStatus::Idle,
            SyncStatus::Syncing,
            synced_status(),
            SyncStatus::Offline,
            SyncStatus::Failed(SyncError::Network),
            SyncStatus::Refused(Refusal::Newer),
            SyncStatus::DriveNewer,
        ]
    }

    /// Signed out: only a refused refresh turns into a line (design: 'a
    /// refused refresh token turns signed out, the status says Signed out.
    /// Sign in again in Settings'). Mutation not caught: the render of the
    /// line (see the render tests).
    #[test]
    fn signed_out_says_something_only_after_a_refused_refresh() {
        let now = 10.0 * HOUR;
        let refused = SyncStatus::Failed(SyncError::SignedOut);
        assert_eq!(
            line(&SyncAccount::SignedOut, &refused, None, 0),
            Line::SignedOutAgain
        );
        for status in every_status() {
            assert_eq!(
                line(&SyncAccount::SignedOut, &status, Some(now), 3),
                Line::Hidden,
                "signed out: {status:?}"
            );
        }
    }

    /// Remembered: paused, whatever the status; a blocked window says why
    /// (design: 'A blocked window (Popup) keeps it paused').
    #[test]
    fn a_remembered_account_is_paused_whatever_the_status() {
        let now = 10.0 * HOUR;
        for status in every_status() {
            assert_eq!(
                line(&remembered(), &status, Some(now), 0),
                Line::Paused { popup: false },
                "remembered: {status:?}"
            );
        }
        let popup = SyncStatus::Failed(SyncError::Popup);
        assert_eq!(
            line(&remembered(), &popup, None, 0),
            Line::Paused { popup: true }
        );
    }

    /// Signed in: the design's table. Mutations this does not catch: the
    /// render of a row (see the render tests).
    #[test]
    fn signed_in_follows_the_status() {
        let now = 10.0 * HOUR;
        let stamp = Some(now - 2.0 * MINUTE);
        let at =
            |status: SyncStatus, stamp, from_other| line(&signed_in(), &status, stamp, from_other);
        let synced = |from_other| Line::Synced { from_other };
        assert_eq!(at(SyncStatus::Idle, None, 0), Line::Hidden);
        assert_eq!(at(SyncStatus::Idle, stamp, 0), synced(0));
        assert_eq!(at(SyncStatus::Syncing, stamp, 0), Line::Syncing);
        assert_eq!(at(synced_status(), stamp, 0), synced(0));
        assert_eq!(at(synced_status(), stamp, 4), synced(4));
        assert_eq!(at(SyncStatus::Offline, stamp, 0), Line::Offline);
        for error in [
            SyncError::Network,
            SyncError::Storage,
            SyncError::Drive("403".into()),
            SyncError::Popup,
        ] {
            let status = SyncStatus::Failed(error.clone());
            assert_eq!(at(status, stamp, 0), Line::Retrying(error));
        }
    }

    /// F9: the transport turns the account Remembered on a 401, so a
    /// signed-in account that still carries `Failed(SignedOut)` is the same
    /// state as a remembered one: paused, with Resume, never 'Retrying
    /// automatically'. Mutation not caught: the render of the paused row
    /// (see the render tests).
    #[test]
    fn a_refused_token_on_a_signed_in_account_reads_paused() {
        let refused = SyncStatus::Failed(SyncError::SignedOut);
        for stamp in [None, Some(1_000.0)] {
            assert_eq!(
                line(&signed_in(), &refused, stamp, 2),
                Line::Paused { popup: false },
                "stamp {stamp:?}"
            );
        }
        let network = SyncStatus::Failed(SyncError::Network);
        assert_eq!(
            line(&signed_in(), &network, None, 0),
            Line::Retrying(SyncError::Network)
        );
    }

    /// Design: 'Synced' with no stamp is still 'Synced' (the stamp is in
    /// memory only); the age is the render's business, and Idle with no
    /// stamp stays hidden.
    #[test]
    fn a_synced_status_without_a_stamp_is_still_synced() {
        assert_eq!(
            line(&signed_in(), &synced_status(), None, 0),
            Line::Synced { from_other: 0 }
        );
        assert_eq!(line(&signed_in(), &SyncStatus::Idle, None, 0), Line::Hidden);
    }

    /// The terminal states: no automatic cycle, 'See Settings'; a newer
    /// Drive copy says 'Update the app'.
    #[test]
    fn the_terminal_states_point_to_settings() {
        let now = 10.0 * HOUR;
        for status in [
            SyncStatus::Refused(Refusal::Unreadable),
            SyncStatus::Refused(Refusal::NotSaved),
            SyncStatus::DriveUnreadable { raw: "{".into() },
        ] {
            assert_eq!(
                line(&signed_in(), &status, Some(now), 0),
                Line::NotSynced(Terminal::Settings),
                "{status:?}"
            );
        }
        assert_eq!(
            line(&signed_in(), &SyncStatus::DriveNewer, None, 0),
            Line::NotSynced(Terminal::UpdateApp)
        );
    }

    /// The age is clamped at 0: a clock that went back, or a stamp from a
    /// later tick, never reads negative; no stamp, no age. Mutation not
    /// caught: a clamp at another floor than 0 (the age label turns any
    /// negative into 'just now' too, so the label row would not tell).
    #[test]
    fn the_age_never_goes_below_zero() {
        assert_eq!(age_since(1_000.0, Some(5_000.0)), Some(0.0));
        assert_eq!(age_since(5_000.0, Some(1_000.0)), Some(4_000.0));
        assert_eq!(age_since(5_000.0, None), None);
    }

    /// A night's sleep: the age is measured on the wall clock, so after
    /// eight hours the line reads '8 h ago'.
    #[test]
    fn a_night_of_sleep_reads_hours() {
        let stamp = 1_000_000.0;
        let age = age_since(stamp + 8.0 * HOUR, Some(stamp)).expect("an age");
        assert_eq!(age_label(Locale::en, age), "8 h ago");
    }

    /// The age's words: floors, thresholds, junk, and the other languages.
    /// Mutation not caught: a threshold off by a millisecond inside a
    /// bucket.
    #[test]
    fn the_age_is_worded_by_its_largest_unit() {
        for (age, text) in [
            (0.0, "just now"),
            (59_999.0, "just now"),
            (MINUTE, "1 min ago"),
            (125_000.0, "2 min ago"),
            (59.0 * MINUTE + 59_000.0, "59 min ago"),
            (HOUR, "1 h ago"),
            (8.0 * HOUR, "8 h ago"),
            (23.0 * HOUR + 59.0 * MINUTE, "23 h ago"),
            (24.0 * HOUR, "1 d ago"),
            (50.0 * HOUR, "2 d ago"),
            (-5.0 * MINUTE, "just now"),
            (f64::NAN, "just now"),
            (f64::INFINITY, "just now"),
        ] {
            assert_eq!(age_label(Locale::en, age), text, "{age}");
        }
        assert_eq!(age_label(Locale::fr, 8.0 * HOUR), "il y a 8 h");
        assert_eq!(age_label(Locale::vi, 2.0 * MINUTE), "2 phút trước");
    }

    /// 'See Settings' opens Settings; with no shell it does nothing.
    #[test]
    fn see_settings_opens_settings() {
        Owner::new().with(|| {
            let open = RwSignal::new(false);
            open_settings(None);
            assert!(!open.get());
            open_settings(Some(open));
            assert!(open.get());
            open_settings(Some(open));
            assert!(open.get(), "opening twice leaves it open");
        });
    }

    /// The plural selector of the count: French reads 0 and 1 as singular,
    /// English and Vietnamese only 1 (Vietnamese has one form: its two
    /// strings are the same). Mutation not caught: a locale added to the
    /// catalogue is a compile error here, not a red row.
    #[test]
    fn the_count_is_singular_by_the_rules_of_each_language() {
        for (locale, count, singular_form) in [
            (Locale::fr, 0, true),
            (Locale::fr, 1, true),
            (Locale::fr, 2, false),
            (Locale::en, 0, false),
            (Locale::en, 1, true),
            (Locale::en, 2, false),
            (Locale::vi, 0, false),
            (Locale::vi, 1, true),
            (Locale::vi, 2, false),
        ] {
            assert_eq!(singular(locale, count), singular_form, "{locale:?} {count}");
        }
        assert_eq!(
            from_other_label(Locale::fr, 0),
            "0 terme venu d’un autre appareil"
        );
        assert_eq!(
            from_other_label(Locale::fr, 1),
            "1 terme venu d’un autre appareil"
        );
        assert_eq!(
            from_other_label(Locale::fr, 2),
            "2 termes venus d’un autre appareil"
        );
        assert_eq!(
            from_other_label(Locale::en, 0),
            "0 terms from another device"
        );
        assert_eq!(
            from_other_label(Locale::en, 1),
            "1 term from another device"
        );
        assert_eq!(
            from_other_label(Locale::vi, 0),
            "0 thuật ngữ từ thiết bị khác"
        );
        assert_eq!(
            from_other_label(Locale::vi, 1),
            "1 thuật ngữ từ thiết bị khác"
        );
        assert_eq!(
            from_other_label(Locale::vi, 2),
            "2 thuật ngữ từ thiết bị khác"
        );
    }

    /// Design: 'the minute refresh is never announced'. The clock moves ten
    /// times: the age follows every time, the shape (the label and its
    /// button) is never notified. Green on arrival for the shape: the line
    /// carries no age since F9 (`steady` held it before). Mutation not
    /// caught: the memo reading `now` again, since an equal line does not
    /// notify; the arrangement is held by `line` and `shape_memo` taking no
    /// clock.
    #[test]
    fn the_minute_tick_never_notifies_the_shape() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let _ = any_spawner::Executor::init_tokio();
            let (shape_runs, age_runs) =
                (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let owner = Owner::new();
            let now = owner.with(|| {
                let now = RwSignal::new(10.0 * HOUR);
                let synced_at: Signal<Option<f64>> = RwSignal::new(Some(10.0 * HOUR)).into();
                let shape = shape_memo(
                    RwSignal::new(signed_in()).into(),
                    RwSignal::new(synced_status()).into(),
                    synced_at,
                    RwSignal::new(0).into(),
                );
                count_runs(shape, &shape_runs);
                count_runs(age_signal(now, synced_at), &age_runs);
                now
            });
            leptos::task::tick().await;
            assert_eq!((runs(&shape_runs), runs(&age_runs)), (1, 1));
            for minute in 1..=10 {
                owner.with(|| now.set(10.0 * HOUR + f64::from(minute) * MINUTE));
                leptos::task::tick().await;
            }
            assert_eq!(runs(&shape_runs), 1, "the shape was notified by the tick");
            assert_eq!(runs(&age_runs), 11, "the age follows each tick");
        });
    }

    /// An effect that counts how often `source` notifies it (once at start).
    fn count_runs(source: impl Track + Send + Sync + 'static, runs: &Arc<AtomicUsize>) {
        let runs = runs.clone();
        Effect::new_isomorphic(move |_| {
            source.track();
            runs.fetch_add(1, Ordering::SeqCst);
        });
    }

    fn runs(count: &AtomicUsize) -> usize {
        count.load(Ordering::SeqCst)
    }

    /// The age signal: the stamp against the clock, clamped, none without a
    /// stamp.
    #[test]
    fn the_age_signal_follows_the_clock_and_the_stamp() {
        Owner::new().with(|| {
            let now = RwSignal::new(10.0 * HOUR);
            let stamp = RwSignal::new(None);
            let age = age_signal(now, stamp.into());
            assert_eq!(age.get(), None);
            stamp.set(Some(10.0 * HOUR - 2.0 * MINUTE));
            assert_eq!(age.get(), Some(2.0 * MINUTE));
            now.set(10.0 * HOUR + 3.0 * MINUTE);
            assert_eq!(age.get(), Some(5.0 * MINUTE));
            now.set(0.0);
            assert_eq!(age.get(), Some(0.0), "a clock that stepped back");
        });
    }

    // ---- The render ----

    /// The line rendered in `language` under fakes, with a cycle over a
    /// fake transport for `account`, at `status`, stamped `ago` ms ago
    /// (none: no stamp) with `from_other` terms; `mounted` as given.
    fn render_with(
        language: Language,
        cycle: Option<(SyncAccount, SyncStatus, Option<f64>, usize, bool)>,
    ) -> String {
        Owner::new()
            .with(|| {
                palicanon_ui_lib::provide_language().set(language);
                provide_fakes(Load::Ok(crate::test_support::progress_fixture()));
                if let Some((account, status, ago, from_other, mounted)) = cycle {
                    let transport: SharedSync = Rc::new(FakeTransport::new(account));
                    let cycle = SyncCycle::new(
                        transport,
                        Backend::current(),
                        || {},
                        String::new,
                        |_, _| {},
                    );
                    cycle.set_status(status);
                    cycle.set_synced(ago.map(|ago| wall_now_ms() - ago), from_other);
                    if mounted {
                        cycle.set_mounted();
                    }
                    provide_context(cycle);
                }
                view! { <SyncStatusLine /> }.to_html()
            })
            .replace("<!>", "")
    }

    fn html(
        account: SyncAccount,
        status: SyncStatus,
        ago: Option<f64>,
        from_other: usize,
    ) -> String {
        render_with(Language::En, Some((account, status, ago, from_other, true)))
    }

    fn buttons(html: &str) -> usize {
        html.matches("<button").count()
    }

    #[test]
    fn synced_says_synced_and_how_long_ago() {
        let html = html(signed_in(), synced_status(), Some(125_000.0), 0);
        assert!(html.contains(">Synced<"), "{html}");
        assert!(html.contains("2 min ago"), "{html}");
        assert!(html.contains(" · "), "{html}");
        assert_eq!(buttons(&html), 0, "{html}");
        assert!(!html.contains("another device"), "{html}");
    }

    /// The label is the announced region; the age sits outside it, so the
    /// minute refresh is never announced. Mutation this catches: the age
    /// moved inside the status element; not caught: the age put in another
    /// live region (`aria-live`, `role=alert`) of its own.
    #[test]
    fn the_age_is_outside_the_announced_region() {
        let html = html(signed_in(), synced_status(), Some(125_000.0), 0);
        let status = tag_around(&html, ">Synced<");
        assert!(status.contains(r#"role="status""#), "{status}");
        let open = html.find(status).expect("the tag");
        let close = open + html[open..].find("</span>").expect("closing");
        let region = &html[open..close];
        assert!(!region.contains("min ago"), "{region}");
        let age = tag_around(&html, "2 min ago");
        assert!(age.contains("data-sync-age"), "{age}");
        assert!(!age.contains("role="), "{age}");
        assert!(
            open < html.find("2 min ago").unwrap(),
            "label first: {html}"
        );
    }

    /// Design: the stamp is in memory only; a 'Synced' status with none
    /// says 'Synced' and no age (no 'just now' invented), and the count
    /// follows with its one separator.
    #[test]
    fn synced_without_a_stamp_shows_no_age() {
        let bare = html(signed_in(), synced_status(), None, 0);
        assert!(bare.contains(">Synced<"), "{bare}");
        assert!(!bare.contains("data-sync-age"), "{bare}");
        assert!(!bare.contains("just now"), "{bare}");
        assert!(!bare.contains(" · "), "{bare}");
        assert_eq!(buttons(&bare), 0, "{bare}");
        let counted = html(signed_in(), synced_status(), None, 3);
        assert!(!counted.contains("data-sync-age"), "{counted}");
        assert!(
            counted.contains(">3 terms from another device<"),
            "{counted}"
        );
        assert_eq!(counted.matches(" · ").count(), 1, "{counted}");
    }

    /// F9: a signed-in account with a refused token renders Paused with
    /// Resume, not the retry line. Mutation not caught: the pure mapping
    /// alone (see `a_refused_token_on_a_signed_in_account_reads_paused`).
    #[test]
    fn a_refused_token_on_a_signed_in_account_renders_paused() {
        let refused = SyncStatus::Failed(SyncError::SignedOut);
        let html = html(signed_in(), refused, None, 0);
        assert!(html.contains("Sync paused"), "{html}");
        assert!(html.contains(">Resume<"), "{html}");
        assert!(!html.contains("Retrying"), "{html}");
        assert!(!html.contains(">Retry<"), "{html}");
    }

    /// A zero count has no region of its own, whatever the language (the
    /// line passes only counts above 0).
    #[test]
    fn a_zero_count_renders_no_count_region() {
        for (language, word) in [
            (Language::En, "another device"),
            (Language::Fr, "autre appareil"),
            (Language::Vi, "thiết bị khác"),
        ] {
            let cycle = Some((signed_in(), synced_status(), Some(125_000.0), 0, true));
            let html = render_with(language, cycle);
            assert!(!html.contains(word), "{language:?}: {html}");
            assert_eq!(html.matches(" · ").count(), 1, "{html}");
        }
    }

    #[test]
    fn a_stale_stamp_reads_hours_in_the_page() {
        let html = html(signed_in(), SyncStatus::Idle, Some(8.0 * HOUR), 0);
        assert!(html.contains("8 h ago"), "{html}");
        assert!(!html.contains("min ago"), "{html}");
    }

    /// The count is its own announced region, after the age; 1 and many
    /// have their own words (kept until the next cycle that merges).
    #[test]
    fn terms_from_another_device_are_counted_in_their_own_region() {
        let one = html(signed_in(), synced_status(), Some(125_000.0), 1);
        assert!(one.contains(">1 term from another device<"), "{one}");
        let three = html(signed_in(), synced_status(), Some(125_000.0), 3);
        assert!(three.contains(">3 terms from another device<"), "{three}");
        let region = tag_around(&three, ">3 terms from another device<");
        assert!(region.contains(r#"role="status""#), "{region}");
        let (age, count) = (
            three.find("2 min ago").unwrap(),
            three.find("3 terms").unwrap(),
        );
        assert!(age < count, "label, age, count: {three}");
        assert_eq!(three.matches(" · ").count(), 2, "{three}");
        for (language, one_text, many_text) in [
            (
                Language::Fr,
                ">1 terme venu d’un autre appareil<",
                ">3 termes venus d’un autre appareil<",
            ),
            (
                Language::Vi,
                ">1 thuật ngữ từ thiết bị khác<",
                ">3 thuật ngữ từ thiết bị khác<",
            ),
        ] {
            for (n, text) in [(1, one_text), (3, many_text)] {
                let cycle = Some((signed_in(), synced_status(), Some(125_000.0), n, true));
                let html = render_with(language, cycle);
                assert!(html.contains(text), "{n}: {html}");
            }
        }
    }

    /// G4 (i), design: 'the age sits outside it, so the minute refresh is
    /// never announced'. A line with BOTH a stamp and a count: the label
    /// and the count are two `role="status"` regions (exactly two), the age
    /// is in neither of them and carries no role. Green on arrival.
    /// Mutations this catches: the age moved inside the label's element or
    /// the count's; not caught: an `aria-live` on the age.
    #[test]
    fn a_stamp_and_a_count_keep_the_age_outside_both_regions() {
        let html = html(signed_in(), synced_status(), Some(125_000.0), 3);
        assert!(html.contains(">Synced<"), "{html}");
        let age = tag_around(&html, "2 min ago");
        assert!(age.contains("data-sync-age"), "{age}");
        assert!(!age.contains("role="), "{age}");
        assert_eq!(html.matches(r#"role="status""#).count(), 2, "{html}");
        for text in [">Synced<", ">3 terms from another device<"] {
            let tag = tag_around(&html, text);
            assert!(tag.contains(r#"role="status""#), "{text}: {tag}");
            let open = html.find(tag).expect("the tag");
            let close = open + html[open..].find("</span>").expect("closing");
            assert!(!html[open..close].contains("min ago"), "{text}: {html}");
        }
        let (label, age_at, count) = (
            html.find(">Synced<").unwrap(),
            html.find("2 min ago").unwrap(),
            html.find("3 terms").unwrap(),
        );
        assert!(
            label < age_at && age_at < count,
            "label, age, count: {html}"
        );
    }

    /// G4 (ii). The ruling reads 'a native build offers no Resume on the
    /// Paused line'; the line does not depend on the build: Resume is the
    /// Paused line's one button whatever the host, and a host without a
    /// pause (the desktop, always signed in) never reaches the Paused line
    /// except through a refused token. What is pinned: for a signed-in
    /// account every status but `Failed(SignedOut)` renders no Resume; the
    /// Paused line has exactly one button, Resume, and no Retry or See
    /// Settings. Green on arrival. Mutation this catches: Resume added to
    /// another line; not caught: the desktop transport's own behaviour
    /// (the app repo).
    #[test]
    fn only_the_paused_line_offers_resume_and_it_offers_nothing_else() {
        for status in every_status() {
            let html = html(signed_in(), status.clone(), Some(125_000.0), 0);
            assert!(!html.contains(">Resume<"), "{status:?}: {html}");
        }
        let popup = html(signed_in(), SyncStatus::Failed(SyncError::Popup), None, 0);
        assert!(!popup.contains(">Resume<"), "{popup}");
        let refused = SyncStatus::Failed(SyncError::SignedOut);
        for (account, status) in [(signed_in(), refused), (remembered(), SyncStatus::Idle)] {
            let paused = html(account, status, None, 0);
            assert!(paused.contains("Sync paused"), "{paused}");
            assert_eq!(buttons(&paused), 1, "{paused}");
            assert!(paused.contains(">Resume<"), "{paused}");
            assert!(!paused.contains(">Retry<"), "{paused}");
            assert!(!paused.contains(">See Settings<"), "{paused}");
        }
    }

    /// G4 (iii): the age part of the line when the stamp appears and then
    /// disappears (sign-out clears it). With a stamp the span shows the
    /// age; once it is gone the part renders nothing (no 'just now'
    /// invented, no panic on the missing age); the stamp back brings the
    /// span back. Green on arrival (design: 'a Synced status with no stamp
    /// says Synced and no age'). Mutation this catches: the stamp guard
    /// dropped (the span shows with no stamp); not caught: the live DOM
    /// swap (the browser suites), this renders the part once per state.
    #[test]
    fn the_age_part_follows_the_stamp_appearing_and_disappearing() {
        Owner::new().with(|| {
            palicanon_ui_lib::provide_language().set(Language::En);
            provide_fakes(Load::Ok(crate::test_support::progress_fixture()));
            let now = RwSignal::new(10.0 * HOUR);
            let stamp = RwSignal::new(None);
            let age = age_signal(now, stamp.into());
            let part = |age| view! { {age_view(age)} }.to_html().replace("<!>", "");
            assert!(!part(age).contains("data-sync-age"), "{}", part(age));
            stamp.set(Some(10.0 * HOUR - 2.0 * MINUTE));
            let shown = part(age);
            assert!(shown.contains("data-sync-age"), "{shown}");
            assert!(shown.contains("2 min ago"), "{shown}");
            stamp.set(None);
            let gone = part(age);
            assert!(!gone.contains("data-sync-age"), "{gone}");
            assert!(!gone.contains("just now"), "{gone}");
            assert!(!gone.contains(" · "), "{gone}");
            stamp.set(Some(10.0 * HOUR));
            assert!(part(age).contains("just now"), "{}", part(age));
        });
    }

    #[test]
    fn syncing_and_offline_are_states_with_no_button() {
        let syncing = html(signed_in(), SyncStatus::Syncing, None, 0);
        assert!(syncing.contains(">Syncing…<"), "{syncing}");
        assert_eq!(buttons(&syncing), 0, "{syncing}");
        assert!(tag_around(&syncing, ">Syncing…<").contains(r#"role="status""#));
        let offline = html(signed_in(), SyncStatus::Offline, None, 0);
        assert!(
            offline.contains(">Offline. Sync resumes when you are back online.<"),
            "{offline}"
        );
        assert!(!offline.contains("Not synced"), "{offline}");
        assert_eq!(buttons(&offline), 0, "{offline}");
    }

    #[test]
    fn a_failure_retries_by_itself_says_why_and_offers_retry() {
        for (error, cause) in [
            (SyncError::Network, "Google Drive could not be reached."),
            (
                SyncError::Drive("403 rate".into()),
                "Google Drive refused: 403 rate",
            ),
            (
                SyncError::Storage,
                "This device could not use its secure storage.",
            ),
        ] {
            let html = html(signed_in(), SyncStatus::Failed(error.clone()), None, 0);
            let label = format!("Not synced. Retrying automatically. {cause}");
            assert!(html.contains(&label), "{error:?}: {html}");
            assert!(
                tag_around(&html, &label).contains(r#"role="alert""#),
                "{html}"
            );
            assert_eq!(buttons(&html), 1, "{html}");
            assert!(html.contains(">Retry<"), "{html}");
            assert!(!html.contains("Settings"), "{html}");
            if error == SyncError::Storage {
                assert!(!html.contains("Google Drive refused"), "{html}");
            }
        }
    }

    /// No countdown: the line names no time to the next try.
    #[test]
    fn a_failure_shows_no_countdown() {
        let html = html(signed_in(), SyncStatus::Failed(SyncError::Network), None, 0);
        assert!(!html.contains("in 15") && !html.contains(" s<"), "{html}");
    }

    #[test]
    fn terminal_states_point_to_settings_and_the_newer_copy_says_update() {
        for status in [
            SyncStatus::Refused(Refusal::Invalid),
            SyncStatus::DriveUnreadable { raw: "{".into() },
        ] {
            let html = html(signed_in(), status, None, 0);
            assert!(html.contains("Not synced"), "{html}");
            assert!(!html.contains("Update the app"), "{html}");
            assert!(!html.contains("Retrying"), "{html}");
            assert_eq!(buttons(&html), 1, "{html}");
            assert!(html.contains(">See Settings<"), "{html}");
            assert!(!html.contains(">Retry<"), "{html}");
        }
        let newer = html(signed_in(), SyncStatus::DriveNewer, None, 0);
        assert!(newer.contains("Update the app"), "{newer}");
        assert!(newer.contains(">See Settings<"), "{newer}");
        assert!(
            tag_around(&newer, "Update the app").contains(r#"role="alert""#),
            "{newer}"
        );
    }

    #[test]
    fn a_remembered_account_is_paused_with_resume() {
        let paused = html(remembered(), SyncStatus::Idle, None, 0);
        assert!(paused.contains("Sync paused"), "{paused}");
        assert!(!paused.contains("Allow pop-ups"), "{paused}");
        assert_eq!(buttons(&paused), 1, "{paused}");
        assert!(paused.contains(">Resume<"), "{paused}");
        assert!(tag_around(&paused, "Sync paused").contains(r#"role="status""#));
        let popup = html(remembered(), SyncStatus::Failed(SyncError::Popup), None, 0);
        assert!(
            popup.contains("Sync paused · Allow pop-ups, then tap Resume"),
            "{popup}"
        );
        assert!(popup.contains(">Resume<"), "{popup}");
    }

    #[test]
    fn a_refused_refresh_says_signed_out_with_no_button() {
        let html = html(
            SyncAccount::SignedOut,
            SyncStatus::Failed(SyncError::SignedOut),
            None,
            0,
        );
        assert!(
            html.contains("Signed out. Sign in again in Settings"),
            "{html}"
        );
        assert_eq!(buttons(&html), 0, "{html}");
    }

    /// One line on a phone: the line may shrink, the label truncates, the
    /// button never shrinks. Mutations this catches: `truncate` or `min-w-0`
    /// dropped from the label or the line. Not caught: `layout="shrink-0"`
    /// dropped from the button alone (equivalent today: ui-lib 0.3.1's
    /// button base class already carries `shrink-0`; the test pins the
    /// rendered result and fails if that base class ever loses it), and the
    /// classes being right but a parent clipping the line.
    #[test]
    fn on_a_phone_the_label_truncates_and_the_button_never_does() {
        let retry = html(signed_in(), SyncStatus::Failed(SyncError::Network), None, 0);
        let line = tag_around(&retry, "data-sync-line");
        for class in [
            "flex",
            "min-w-0",
            "items-center",
            "gap-2",
            "text-sm",
            "justify-center",
        ] {
            assert!(line.contains(class), "{class}: {line}");
        }
        let label = tag_around(&retry, "Not synced. Retrying");
        assert!(
            label.contains("truncate") && label.contains("min-w-0"),
            "{label}"
        );
        let rows = [
            retry,
            html(remembered(), SyncStatus::Idle, None, 0),
            html(signed_in(), SyncStatus::DriveNewer, None, 0),
        ];
        for html in rows {
            let start = html.find("<button").expect(&html);
            let button = &html[start..html[start..].find('>').unwrap() + start];
            assert!(button.contains("shrink-0"), "{button}");
        }
        let synced = html(signed_in(), synced_status(), Some(125_000.0), 3);
        assert!(
            tag_around(&synced, ">Synced<").contains("truncate"),
            "{synced}"
        );
    }

    #[test]
    fn nothing_is_rendered_when_there_is_nothing_to_say() {
        let hidden = [
            ("no transport", render_with(Language::En, None)),
            (
                "not mounted",
                render_with(
                    Language::En,
                    Some((signed_in(), synced_status(), Some(1.0), 0, false)),
                ),
            ),
            (
                "signed out, idle",
                html(SyncAccount::SignedOut, SyncStatus::Idle, None, 0),
            ),
            (
                "signed in, no stamp",
                html(signed_in(), SyncStatus::Idle, None, 0),
            ),
        ];
        for (why, html) in hidden {
            assert_eq!(html, "", "{why}");
        }
    }

    /// No em dash in a rendered line, in any language.
    #[test]
    fn no_line_holds_an_em_dash() {
        for language in [Language::En, Language::Fr, Language::Vi] {
            for status in [
                SyncStatus::Offline,
                SyncStatus::DriveNewer,
                SyncStatus::Failed(SyncError::Popup),
            ] {
                let cycle = Some((signed_in(), status, Some(1.0), 3, true));
                assert!(!render_with(language, cycle).contains('—'));
            }
        }
    }
}
