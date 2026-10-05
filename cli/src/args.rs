//! Argument parsing.
//!
//! `learn-cli --deck deck.json --state state.json [--today YYYY-MM-DD]
//! <review | stats | new N | advance K>`. Options may come before or after
//! the command. `today` defaults to the UTC calendar date of the system
//! clock ([`utc_today`]); the standard library has no time zone, so the
//! local date needs `--today`.

use std::fmt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use learn_core::date::Date;

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The deck file (`--deck`).
    pub deck: PathBuf,
    /// The state file (`--state`); it may not exist yet.
    pub state: PathBuf,
    /// The day given with `--today`, if any.
    pub today: Option<Date>,
    /// The command to run.
    pub command: Command,
}

/// The four commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Ask the due questions.
    Review,
    /// Print the stages and the counts.
    Stats,
    /// Start this many new terms.
    New(usize),
    /// Bring this many terms forward.
    Advance(usize),
}

/// Why the command line was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    /// A required option (or the value of an option) is absent.
    Missing(&'static str),
    /// An option the program does not know.
    Unknown(String),
    /// The `--today` value is not a date (the date error's message).
    BadDate(String),
    /// The count of `new` or `advance` is not a positive whole number
    /// (empty when it is absent).
    BadCount(String),
    /// No command was given.
    NoCommand,
    /// A second command or a trailing word.
    Extra(String),
    /// `--help` or `-h`: the caller prints [`usage`] and exits 0.
    Help,
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgsError::Missing(what) => write!(f, "missing {what}"),
            ArgsError::Unknown(option) => write!(f, "unknown option {option}"),
            ArgsError::BadDate(cause) => write!(f, "bad --today: {cause}"),
            ArgsError::BadCount(text) if text.is_empty() => {
                f.write_str("this command needs a count (a positive whole number)")
            }
            ArgsError::BadCount(text) => {
                write!(f, "{text:?} is not a positive whole number")
            }
            ArgsError::NoCommand => f.write_str("no command (review, stats, new N or advance K)"),
            ArgsError::Extra(word) => write!(f, "unexpected {word:?}"),
            ArgsError::Help => f.write_str("help requested"),
        }
    }
}

impl std::error::Error for ArgsError {}

/// The synopsis and one line per command.
pub fn usage() -> &'static str {
    "learn-cli --deck deck.json --state state.json [--today YYYY-MM-DD] \
<review | stats | new N | advance K>
  review      ask the questions that are due
  stats       print the terms per stage, what is due and what is new
  new N       start N new terms (by rank)
  advance K   bring K terms forward after a pause (only when nothing is due)
  --today     the day to use (default: today's UTC date)"
}

/// Reads the command line (the arguments without the program name).
///
/// # Errors
/// An [`ArgsError`] naming what is missing, unknown or malformed;
/// [`ArgsError::Help`] when `--help` or `-h` is present.
pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Args, ArgsError> {
    let args: Vec<String> = args.into_iter().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Err(ArgsError::Help);
    }
    let mut deck = None;
    let mut state = None;
    let mut today = None;
    let mut words = Vec::new();
    let mut rest = args.into_iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--deck" => deck = Some(PathBuf::from(value(&mut rest, "--deck")?)),
            "--state" => state = Some(PathBuf::from(value(&mut rest, "--state")?)),
            "--today" => {
                let text = value(&mut rest, "--today")?;
                today = Some(Date::parse(&text).map_err(|e| ArgsError::BadDate(e.to_string()))?);
            }
            _ if is_option(&arg) => return Err(ArgsError::Unknown(arg)),
            _ => words.push(arg),
        }
    }
    let command = parse_command(&words)?;
    Ok(Args {
        deck: deck.ok_or(ArgsError::Missing("--deck"))?,
        state: state.ok_or(ArgsError::Missing("--state"))?,
        today,
        command,
    })
}

/// The next argument, as the value of `option`.
fn value(
    rest: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<String, ArgsError> {
    rest.next().ok_or(ArgsError::Missing(option))
}

/// A word that starts like an option; `-1` is a number, not an option.
fn is_option(arg: &str) -> bool {
    let mut chars = arg.chars();
    chars.next() == Some('-') && chars.next().is_some_and(|c| !c.is_ascii_digit())
}

/// The command and its count from the words that are not options.
fn parse_command(words: &[String]) -> Result<Command, ArgsError> {
    let (name, rest) = words.split_first().ok_or(ArgsError::NoCommand)?;
    let (command, rest) = match name.as_str() {
        "review" => (Command::Review, rest),
        "stats" => (Command::Stats, rest),
        "new" | "advance" => {
            let (count, rest) = rest
                .split_first()
                .ok_or_else(|| ArgsError::BadCount(String::new()))?;
            let count = parse_count(count)?;
            let command = if name == "new" {
                Command::New(count)
            } else {
                Command::Advance(count)
            };
            (command, rest)
        }
        other => return Err(ArgsError::Extra(other.to_owned())),
    };
    match rest.first() {
        Some(word) => Err(ArgsError::Extra(word.clone())),
        None => Ok(command),
    }
}

/// A positive whole number, digits only.
fn parse_count(text: &str) -> Result<usize, ArgsError> {
    let bad = || ArgsError::BadCount(text.to_owned());
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    match text.parse::<usize>() {
        Ok(count) if count > 0 => Ok(count),
        _ => Err(bad()),
    }
}

impl Args {
    /// `--today` when given, else [`utc_today`].
    pub fn today_or_now(&self) -> Date {
        self.today.unwrap_or_else(utc_today)
    }
}

/// The UTC calendar date of the system clock.
///
/// The standard library has no time zone: this is the UTC day, so late in
/// the evening west of Greenwich it may already be tomorrow; `--today`
/// overrides it.
pub fn utc_today() -> Date {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Date::from_days(u32::try_from(seconds / 86_400).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Args, ArgsError> {
        parse_args(line.split_whitespace().map(str::to_owned))
    }

    fn ok(line: &str) -> Args {
        parse(line).unwrap_or_else(|e| panic!("{line:?} refused: {e}"))
    }

    #[test]
    fn review_with_the_two_required_options() {
        let args = ok("--deck d.json --state s.json review");
        assert_eq!(args.deck, PathBuf::from("d.json"));
        assert_eq!(args.state, PathBuf::from("s.json"));
        assert_eq!(args.today, None);
        assert_eq!(args.command, Command::Review);
    }

    #[test]
    fn stats_command() {
        assert_eq!(ok("--deck d --state s stats").command, Command::Stats);
    }

    #[test]
    fn new_takes_a_count() {
        assert_eq!(ok("--deck d --state s new 5").command, Command::New(5));
    }

    #[test]
    fn advance_takes_a_count() {
        assert_eq!(
            ok("--deck d --state s advance 12").command,
            Command::Advance(12)
        );
    }

    #[test]
    fn today_is_parsed() {
        let args = ok("--deck d --state s --today 2026-10-05 review");
        assert_eq!(args.today, Some(Date::parse("2026-10-05").unwrap()));
    }

    #[test]
    fn options_may_follow_the_command() {
        let args = ok("review --deck d --state s --today 1970-01-02");
        assert_eq!(args.command, Command::Review);
        assert_eq!(args.deck, PathBuf::from("d"));
        assert_eq!(args.today, Some(Date::from_days(1)));
        let args = ok("--deck d new 3 --state s");
        assert_eq!(args.command, Command::New(3));
        assert_eq!(args.state, PathBuf::from("s"));
    }

    #[test]
    fn options_in_any_order() {
        let args = ok("--today 2026-10-05 --state s --deck d stats");
        assert_eq!(args.deck, PathBuf::from("d"));
        assert_eq!(args.state, PathBuf::from("s"));
    }

    #[test]
    fn missing_deck() {
        assert_eq!(parse("--state s review"), Err(ArgsError::Missing("--deck")));
    }

    #[test]
    fn missing_state() {
        assert_eq!(parse("--deck d review"), Err(ArgsError::Missing("--state")));
    }

    #[test]
    fn option_without_its_value() {
        assert_eq!(
            parse("--state s review --deck"),
            Err(ArgsError::Missing("--deck"))
        );
        assert_eq!(
            parse("--deck d --state s review --today"),
            Err(ArgsError::Missing("--today"))
        );
    }

    #[test]
    fn unknown_option() {
        assert_eq!(
            parse("--deck d --state s --colour review"),
            Err(ArgsError::Unknown("--colour".to_owned()))
        );
        assert_eq!(
            parse("--deck d --state s -x review"),
            Err(ArgsError::Unknown("-x".to_owned()))
        );
    }

    #[test]
    fn bad_date_wraps_the_date_error() {
        let wanted = Date::parse("2026-13-01").unwrap_err().to_string();
        assert_eq!(
            parse("--deck d --state s --today 2026-13-01 review"),
            Err(ArgsError::BadDate(wanted))
        );
        assert!(matches!(
            parse("--deck d --state s --today yesterday review"),
            Err(ArgsError::BadDate(_))
        ));
    }

    #[test]
    fn bad_zero_and_missing_counts() {
        for line in ["new many", "new 0", "advance -1", "new 2.5"] {
            let full = format!("--deck d --state s {line}");
            assert!(
                matches!(parse(&full), Err(ArgsError::BadCount(_))),
                "{line}"
            );
        }
        assert_eq!(
            parse("--deck d --state s new"),
            Err(ArgsError::BadCount(String::new()))
        );
        assert_eq!(
            parse("--deck d --state s advance"),
            Err(ArgsError::BadCount(String::new()))
        );
        assert_eq!(
            parse("--deck d --state s new 0"),
            Err(ArgsError::BadCount("0".to_owned()))
        );
    }

    #[test]
    fn no_command() {
        assert_eq!(parse("--deck d --state s"), Err(ArgsError::NoCommand));
        assert_eq!(parse(""), Err(ArgsError::NoCommand));
    }

    #[test]
    fn a_second_command_or_a_trailing_word_is_extra() {
        assert_eq!(
            parse("--deck d --state s review stats"),
            Err(ArgsError::Extra("stats".to_owned()))
        );
        assert_eq!(
            parse("--deck d --state s review now"),
            Err(ArgsError::Extra("now".to_owned()))
        );
        assert_eq!(
            parse("--deck d --state s new 3 4"),
            Err(ArgsError::Extra("4".to_owned()))
        );
    }

    #[test]
    fn an_unknown_word_is_not_a_command() {
        assert_eq!(
            parse("--deck d --state s frobnicate"),
            Err(ArgsError::Extra("frobnicate".to_owned()))
        );
    }

    #[test]
    fn help_wins_over_everything() {
        assert_eq!(parse("--help"), Err(ArgsError::Help));
        assert_eq!(parse("-h"), Err(ArgsError::Help));
        assert_eq!(parse("--deck d review -h"), Err(ArgsError::Help));
    }

    #[test]
    fn errors_have_readable_messages() {
        assert!(ArgsError::Missing("--deck").to_string().contains("--deck"));
        assert!(
            ArgsError::Unknown("--x".to_owned())
                .to_string()
                .contains("--x")
        );
        assert!(
            ArgsError::BadCount("0".to_owned())
                .to_string()
                .contains('0')
        );
        assert!(
            ArgsError::Extra("word".to_owned())
                .to_string()
                .contains("word")
        );
        assert_ne!(ArgsError::NoCommand.to_string(), "");
        assert_ne!(ArgsError::BadCount(String::new()).to_string(), "");
    }

    #[test]
    fn usage_names_every_command() {
        let text = usage();
        assert!(text.starts_with("learn-cli --deck deck.json --state state.json"));
        for word in ["review", "stats", "new N", "advance K", "--today"] {
            assert!(text.contains(word), "{word}");
        }
    }

    #[test]
    fn utc_today_is_after_this_task_was_written() {
        assert!(utc_today() >= Date::parse("2026-10-05").unwrap());
    }

    #[test]
    fn today_option_wins_over_the_clock() {
        let args = ok("--deck d --state s --today 2030-01-01 review");
        assert_eq!(args.today_or_now(), Date::parse("2030-01-01").unwrap());
        let args = ok("--deck d --state s review");
        assert!(args.today_or_now() >= Date::parse("2026-10-05").unwrap());
    }
}
