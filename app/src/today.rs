//! `today` for a session: the browser's local calendar date
//! ([`browser_today`]), never before the state's last day
//! ([`session_today`], Q2: a time zone moved west never refuses a review),
//! pinned for a session; the home recomputes it when the page becomes
//! visible again ([`on_visible`]).
//!
//! The browser glue exists in every build so the pages compile without
//! `cfg`; under `ssr` it is a stub, never called (no clock on the server's
//! first paint).

use learn_core::date::Date;
use learn_core::progress::State;

/// The [`Date`] of a calendar day (the browser's `getFullYear`, its
/// `getMonth() + 1`, `getDate()`); `None` when it is not a day on or after
/// 1970-01-01.
#[cfg_attr(
    not(any(feature = "hydrate", test)),
    expect(
        dead_code,
        reason = "read by `browser_today` under hydrate (and the tests); the server has no clock"
    )
)]
pub(crate) fn date_from_ymd(year: i64, month: u32, day: u32) -> Option<Date> {
    // `Date::parse` checks the calendar (leap years included) and the epoch;
    // a year outside four digits is not a day it reads.
    if !(0..=9999).contains(&year) {
        return None;
    }
    Date::parse(&format!("{year:04}-{month:02}-{day:02}")).ok()
}

/// The session's `today`: the later of `browser` and `state.last_today`
/// (Q2: a time zone moved west never refuses a review).
pub(crate) fn session_today(browser: Date, state: &State) -> Date {
    state.last_today.map_or(browser, |last| browser.max(last))
}

/// The browser's local calendar date (`js_sys::Date`: year, month + 1,
/// day); on the server, 1970-01-01 (never called there).
pub(crate) fn browser_today() -> Date {
    #[cfg(feature = "hydrate")]
    {
        let now = js_sys::Date::new_0();
        date_from_ymd(
            i64::from(now.get_full_year()),
            now.get_month() + 1,
            now.get_date(),
        )
        .unwrap_or_else(|| Date::from_days(0))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        Date::from_days(0)
    }
}

/// Calls `on_visible` each time the page becomes visible again
/// (`visibilitychange`, which bubbles from the document to the window),
/// for as long as the current reactive owner lives (the listener is removed
/// in its cleanup); nothing on the server.
pub(crate) fn on_visible(on_visible: impl Fn() + 'static) {
    #[cfg(feature = "hydrate")]
    {
        use leptos::prelude::{on_cleanup, window_event_listener};
        let handle = window_event_listener(leptos::ev::visibilitychange, move |_| {
            let visible = web_sys::window()
                .and_then(|window| window.document())
                .is_some_and(|document| {
                    document.visibility_state() == web_sys::VisibilityState::Visible
                });
            if visible {
                on_visible();
            }
        });
        on_cleanup(move || handle.remove());
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = on_visible;
}

#[cfg(test)]
mod tests {
    use super::{date_from_ymd, session_today};
    use learn_core::date::Date;
    use learn_core::progress::State;

    fn day(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn a_calendar_day_is_a_date() {
        assert_eq!(date_from_ymd(2026, 10, 5), Some(day("2026-10-05")));
        assert_eq!(date_from_ymd(1970, 1, 1), Some(Date::from_days(0)));
    }

    #[test]
    fn leap_days_are_days_only_in_leap_years() {
        assert_eq!(date_from_ymd(2024, 2, 29), Some(day("2024-02-29")));
        assert_eq!(date_from_ymd(2023, 2, 29), None);
        assert_eq!(date_from_ymd(2000, 2, 29), Some(day("2000-02-29")));
        assert_eq!(date_from_ymd(1900, 2, 29), None);
    }

    #[test]
    fn what_is_not_a_day_is_none() {
        assert_eq!(date_from_ymd(2026, 4, 31), None);
        assert_eq!(date_from_ymd(2026, 13, 1), None);
        assert_eq!(date_from_ymd(2026, 0, 1), None);
        assert_eq!(date_from_ymd(2026, 1, 0), None);
        assert_eq!(date_from_ymd(1969, 12, 31), None);
        assert_eq!(date_from_ymd(-5, 1, 1), None);
        assert_eq!(date_from_ymd(12345, 1, 1), None);
    }

    #[test]
    fn today_is_the_browser_date_without_a_last_day() {
        let state = State::empty();
        assert_eq!(session_today(day("2026-10-05"), &state), day("2026-10-05"));
    }

    #[test]
    fn today_never_goes_before_the_last_day() {
        let mut state = State::empty();
        state.last_today = Some(day("2026-10-06"));
        assert_eq!(session_today(day("2026-10-05"), &state), day("2026-10-06"));
    }

    #[test]
    fn today_is_the_browser_date_when_it_is_later_or_equal() {
        let mut state = State::empty();
        state.last_today = Some(day("2026-10-05"));
        assert_eq!(session_today(day("2026-10-05"), &state), day("2026-10-05"));
        assert_eq!(session_today(day("2026-10-09"), &state), day("2026-10-09"));
    }
}
