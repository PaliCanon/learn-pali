//! The stats command's output.

use std::io::{self, Write};

use learn_core::date::Date;
use learn_core::stage::Stage;
use learn_core::stats::Stats;

/// Prints the day, one line per stage ([`Stage::ALL`] order, with the
/// stage's interval), then what is due today, how many new terms are left,
/// the lapses and the orphans.
///
/// # Errors
/// The write error of `out`.
pub fn print_stats<W: Write>(stats: &Stats, today: Date, out: W) -> io::Result<()> {
    let mut out = out;
    writeln!(out, "{today}")?;
    for (stage, count) in Stage::ALL.iter().zip(stats.per_stage) {
        let name = format!("{stage:?}");
        let interval = format!("{} d", stage.interval_days());
        writeln!(out, "{name:<9} {interval:>6}  {count:>5}")?;
    }
    writeln!(out, "due today {:>15}", stats.due_today)?;
    writeln!(out, "new left  {:>15}", stats.new_left)?;
    writeln!(out, "lapses    {:>15}", stats.lapses_total)?;
    writeln!(out, "orphans   {:>15}", stats.orphans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(stats: &Stats, today: &str) -> String {
        let mut out = Vec::new();
        print_stats(stats, Date::parse(today).unwrap(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn sample() -> Stats {
        Stats {
            per_stage: [3, 0, 12, 1, 0, 7],
            due_today: 4,
            new_left: 27,
            lapses_total: 5,
            orphans: 2,
        }
    }

    #[test]
    fn the_day_comes_first() {
        let text = printed(&sample(), "2026-10-05");
        assert_eq!(text.lines().next(), Some("2026-10-05"), "{text}");
    }

    #[test]
    fn there_is_one_line_per_stage_in_order_with_its_count() {
        let text = printed(&sample(), "2026-10-05");
        let lines: Vec<&str> = text.lines().collect();
        let stages: Vec<&str> = lines[1..7].to_vec();
        let names: Vec<String> = Stage::ALL.iter().map(|s| format!("{s:?}")).collect();
        for ((line, name), count) in stages.iter().zip(&names).zip([3, 0, 12, 1, 0, 7]) {
            assert!(line.starts_with(name.as_str()), "{line}");
            assert!(line.trim_end().ends_with(&count.to_string()), "{line}");
        }
    }

    #[test]
    fn the_stage_lines_carry_the_interval() {
        let text = printed(&sample(), "2026-10-05");
        assert!(text.contains("Year") && text.contains("365 d"), "{text}");
        assert!(text.contains("Day") && text.contains("1 d"), "{text}");
    }

    #[test]
    fn the_totals_follow_the_stages() {
        let text = printed(&sample(), "2026-10-05");
        for expected in ["due today", "new left", "lapses", "orphans"] {
            assert!(text.contains(expected), "{expected} in {text}");
        }
        let line = |name: &str| {
            text.lines()
                .find(|l| l.starts_with(name))
                .unwrap()
                .to_owned()
        };
        assert!(line("due today").ends_with('4'));
        assert!(line("new left").ends_with("27"));
        assert!(line("lapses").ends_with('5'));
        assert!(line("orphans").ends_with('2'));
    }

    #[test]
    fn an_empty_state_prints_zeros() {
        let text = printed(&Stats::default(), "1970-01-01");
        assert_eq!(text.lines().count(), 11, "{text}");
        assert!(text.contains("1970-01-01"));
    }

    #[test]
    fn a_write_error_is_returned() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("closed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(print_stats(&sample(), Date::from_days(0), Broken).is_err());
    }
}
