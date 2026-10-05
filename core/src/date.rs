//! Calendar dates as days since 1970-01-01.
//!
//! The core never reads a clock: `today` is always an input. A [`Date`] is a
//! whole number of days since 1970-01-01 (the Unix epoch, UTC calendar), so
//! adding an interval is an integer addition and ordering is the order of the
//! numbers. It is written and read as `YYYY-MM-DD` (JSON and the command
//! line). The civil-calendar conversions are Howard Hinnant's algorithms
//! (`days_from_civil`, `civil_from_days`), proleptic Gregorian.

use std::fmt;
use std::ops::{Add, Sub};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why a date could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateError {
    /// The text is not `YYYY-MM-DD` (four digits, dash, two digits, dash,
    /// two digits).
    Format(String),
    /// The text has the right shape but is not a calendar day (month 13,
    /// 2023-02-29) or is before 1970-01-01.
    Invalid(String),
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DateError::Format(text) => write!(f, "{text:?} is not a date written YYYY-MM-DD"),
            DateError::Invalid(text) => {
                write!(f, "{text:?} is not a calendar day on or after 1970-01-01")
            }
        }
    }
}

impl std::error::Error for DateError {}

/// A day: the number of days since 1970-01-01. `Date(0)` is 1970-01-01,
/// `Date(20731)` is 2026-10-05.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(u32);

impl Date {
    /// The date `days` days after 1970-01-01.
    pub fn from_days(days: u32) -> Date {
        Date(days)
    }

    /// Days since 1970-01-01.
    pub fn days(self) -> u32 {
        self.0
    }

    /// Reads exactly `YYYY-MM-DD`: a real calendar day (leap years
    /// included) on or after 1970-01-01, otherwise an error naming the text.
    ///
    /// # Errors
    /// [`DateError::Format`] when the text is not of the `YYYY-MM-DD` shape,
    /// [`DateError::Invalid`] when it is not a calendar day or is before 1970.
    pub fn parse(text: &str) -> Result<Date, DateError> {
        let format = || DateError::Format(text.to_string());
        let invalid = || DateError::Invalid(text.to_string());
        let bytes = text.as_bytes();
        let shaped = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
        if !shaped {
            return Err(format());
        }
        let number = |range: std::ops::Range<usize>| -> Result<i64, DateError> {
            text[range].parse().map_err(|_| format())
        };
        let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return Err(invalid());
        }
        let days = days_from_civil(year, month, day);
        u32::try_from(days).map(Date).map_err(|_| invalid())
    }

    /// The calendar day as `(year, month, day)`.
    pub fn ymd(self) -> (i64, u32, u32) {
        let (year, month, day) = civil_from_days(i64::from(self.0));
        // month is 1..=12 and day 1..=31 by construction.
        (
            year,
            u32::try_from(month).unwrap_or(1),
            u32::try_from(day).unwrap_or(1),
        )
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = self.ymd();
        write!(f, "{year:04}-{month:02}-{day:02}")
    }
}

impl Add<u32> for Date {
    type Output = Date;

    /// `date + days`: the day `days` days later.
    fn add(self, days: u32) -> Date {
        Date(self.0 + days)
    }
}

impl Sub for Date {
    type Output = i64;

    /// `later - earlier`: the signed number of days between two dates.
    fn sub(self, earlier: Date) -> i64 {
        i64::from(self.0) - i64::from(earlier.0)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        let text = String::deserialize(deserializer)?;
        Date::parse(&text).map_err(serde::de::Error::custom)
    }
}

fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Hinnant's `days_from_civil`: days since 1970-01-01 of a proleptic
/// Gregorian day (`month` 1..=12, `day` 1..=31).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Hinnant's `civil_from_days`: `(year, month, day)` of a day count since
/// 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn epoch_is_day_zero() {
        assert_eq!(d("1970-01-01").days(), 0);
        assert_eq!(Date::from_days(0).to_string(), "1970-01-01");
    }

    #[test]
    fn known_day_numbers() {
        assert_eq!(d("2026-10-05").days(), 20731);
        assert_eq!(Date::from_days(20731).to_string(), "2026-10-05");
        assert_eq!(d("2000-03-01").days(), 11017);
        assert_eq!(d("1970-01-02").days(), 1);
    }

    #[test]
    fn every_day_round_trips_through_text() {
        for days in (0..60_000).step_by(7).chain([20_731, 59_999]) {
            let date = Date::from_days(days);
            assert_eq!(Date::parse(&date.to_string()).unwrap(), date, "{days}");
        }
    }

    #[test]
    fn ymd_gives_the_calendar_day() {
        assert_eq!(Date::from_days(0).ymd(), (1970, 1, 1));
        assert_eq!(Date::from_days(20_731).ymd(), (2026, 10, 5));
        assert_eq!(d("1999-12-31").ymd(), (1999, 12, 31));
        assert_eq!(d("2000-02-29").ymd(), (2000, 2, 29));
        assert_eq!(d("2024-03-01").ymd(), (2024, 3, 1));
        assert_eq!(d("2100-03-01").ymd(), (2100, 3, 1));
    }

    #[test]
    fn ymd_is_the_inverse_of_parse_for_every_day() {
        for days in (0..80_000).step_by(3) {
            let (year, month, day) = Date::from_days(days).ymd();
            let text = format!("{year:04}-{month:02}-{day:02}");
            assert_eq!(d(&text).days(), days, "{text}");
        }
    }

    #[test]
    fn leap_years() {
        assert!(Date::parse("2024-02-29").is_ok());
        assert!(Date::parse("2000-02-29").is_ok());
        assert!(matches!(
            Date::parse("2023-02-29"),
            Err(DateError::Invalid(_))
        ));
        assert!(matches!(
            Date::parse("2100-02-29"),
            Err(DateError::Invalid(_))
        ));
        assert!(matches!(
            Date::parse("2024-04-31"),
            Err(DateError::Invalid(_))
        ));
    }

    #[test]
    fn difference_across_a_leap_day() {
        assert_eq!(d("2024-03-01") - d("2024-02-28"), 2);
        assert_eq!(d("2024-02-28") - d("2024-03-01"), -2);
        assert_eq!(d("2025-03-01") - d("2024-03-01"), 365);
    }

    #[test]
    fn adding_days_crosses_month_and_year() {
        assert_eq!((d("2026-12-31") + 1).to_string(), "2027-01-01");
        assert_eq!((d("2026-10-05") + 365).to_string(), "2027-10-05");
        assert_eq!((d("2024-02-28") + 2).to_string(), "2024-03-01");
        assert_eq!(d("2026-10-05") + 0, d("2026-10-05"));
    }

    #[test]
    fn ordering_follows_the_calendar() {
        assert!(d("2026-10-05") < d("2026-10-06"));
        assert!(d("2027-01-01") > d("2026-12-31"));
    }

    #[test]
    fn malformed_text_is_a_format_error() {
        for text in [
            "",
            "2026-10",
            "2026/10/05",
            "26-10-05",
            "2026-1-05",
            "2026-10-5",
            "2026-10-05 ",
            "abcd-ef-gh",
            "+026-10-05",
            "2026-10-050",
        ] {
            assert!(
                matches!(Date::parse(text), Err(DateError::Format(_))),
                "{text:?}"
            );
        }
    }

    #[test]
    fn impossible_days_are_invalid() {
        for text in [
            "2026-00-10",
            "2026-13-01",
            "2026-01-00",
            "2026-01-32",
            "1969-12-31",
        ] {
            assert!(
                matches!(Date::parse(text), Err(DateError::Invalid(_))),
                "{text:?}"
            );
        }
    }

    #[test]
    fn errors_name_the_text() {
        let message = Date::parse("2023-02-29").unwrap_err().to_string();
        assert!(message.contains("2023-02-29"), "{message}");
        let message = Date::parse("nope").unwrap_err().to_string();
        assert!(message.contains("nope"), "{message}");
    }

    #[test]
    fn serde_is_the_string() {
        let json = serde_json::to_string(&d("2026-10-05")).unwrap();
        assert_eq!(json, "\"2026-10-05\"");
        assert_eq!(
            serde_json::from_str::<Date>(&json).unwrap(),
            d("2026-10-05")
        );
        assert!(serde_json::from_str::<Date>("\"2023-02-29\"").is_err());
        assert!(serde_json::from_str::<Date>("20731").is_err());
    }
}
