//! `learn_cli` — try the learning loop at the terminal.
//!
//! [`run`] is the whole program with its input and outputs given, so it is
//! tested without a process; `main` only passes the real ones.

use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::path::Path;

use learn_core::deck::{Deck, TermId};
use learn_core::session::{Session, SessionError};
use learn_core::stats::Stats;

use crate::args::{Args, ArgsError, Command, parse_args, usage};
use crate::review::review;
use crate::stats::print_stats;
use crate::store::{StateSlot, StoreError, load_state, save_state};

pub mod args;
pub mod review;
pub mod stats;
pub mod store;

/// Why a command stopped.
#[derive(Debug)]
pub enum CliError {
    /// The state file could not be read or written.
    Store(StoreError),
    /// The session refused (a date before the state's last day, a wrong
    /// answer).
    Session(SessionError),
    /// The terminal (input or output) failed.
    Io(String),
    /// The deck lacks what a question needs (a term, a form, an example).
    Deck(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Store(error) => error.fmt(f),
            CliError::Session(error) => error.fmt(f),
            CliError::Io(cause) => write!(f, "terminal: {cause}"),
            CliError::Deck(cause) => write!(f, "deck: {cause}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<StoreError> for CliError {
    fn from(error: StoreError) -> CliError {
        CliError::Store(error)
    }
}

impl From<SessionError> for CliError {
    fn from(error: SessionError) -> CliError {
        CliError::Session(error)
    }
}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> CliError {
        CliError::Io(error.to_string())
    }
}

impl CliError {
    /// A term the deck does not have.
    pub(crate) fn missing_term(term: &TermId) -> CliError {
        CliError::Deck(format!("term {term} is not in the deck"))
    }
}

/// Runs the program: `args` without the program name, answers read from
/// `input`, results written to `out`, messages about failures to `err`.
/// Returns the exit code: 0 done (or `--help`), 1 a failure, 2 a wrong
/// command line.
pub fn run<I, R, W, E>(args: I, input: R, out: W, err: E) -> i32
where
    I: IntoIterator<Item = String>,
    R: BufRead,
    W: Write,
    E: Write,
{
    let (mut out, mut err) = (out, err);
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(ArgsError::Help) => {
            let _ = writeln!(out, "{}", usage());
            return 0;
        }
        Err(error) => {
            let _ = writeln!(err, "learn-cli: {error}\n{}", usage());
            return 2;
        }
    };
    match execute(&args, input, &mut out) {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(err, "learn-cli: {error}");
            1
        }
    }
}

/// Runs the command of `args`; the first failure stops it.
fn execute<R: BufRead, W: Write>(args: &Args, input: R, mut out: W) -> Result<(), CliError> {
    let deck = load_deck(&args.deck)?;
    let state = load_state(&args.state, &args.deck_id)?;
    let today = args.today_or_now();
    let seed = args.seed_or_now();
    if args.command == Command::Review {
        let slot = StateSlot {
            path: &args.state,
            id: &args.deck_id,
        };
        review(&deck, state, today, seed, input, out, slot)?;
        return Ok(());
    }
    let mut session = Session::start(&deck, state, today, seed)?;
    match args.command {
        Command::Review => {}
        Command::Stats => {
            save_if(false, &session, args)?;
            print_stats(&session.stats(), today, out)?;
        }
        Command::New(n) => {
            let started = session.request_new(n);
            save_if(started > 0, &session, args)?;
            let plural = if started == 1 { "" } else { "s" };
            writeln!(
                out,
                "Started {started} new term{plural} (stage Day, due {today})."
            )?;
        }
        Command::Advance(k) => {
            let why = refusal(&session.stats());
            let moved = session.advance(k);
            save_if(moved > 0, &session, args)?;
            let plural = if moved == 1 { "" } else { "s" };
            if moved > 0 {
                writeln!(out, "Advanced {moved} term{plural} to {today}.")?;
            } else {
                writeln!(out, "Advanced 0 terms: {why}.")?;
            }
        }
    }
    Ok(())
}

/// Why `advance` gives 0, from the counts before it ran.
fn refusal(stats: &Stats) -> &'static str {
    if stats.due_today > 0 {
        "terms are still due"
    } else if stats.new_left > 0 {
        "new terms are left"
    } else {
        "nothing in the long stages"
    }
}

/// Writes the session's state when the command `changed` it, or when
/// [`Session::start`] did (a promotion made at the start must reach the
/// file, or it is redone from the stale `due` at every run).
fn save_if(changed: bool, session: &Session<'_>, args: &Args) -> Result<(), CliError> {
    if changed || session.changed_at_start() {
        save_state(&args.state, &args.deck_id, &session.snapshot())?;
    }
    Ok(())
}

/// Reads and checks the deck file.
fn load_deck(path: &Path) -> Result<Deck, CliError> {
    let text = fs::read_to_string(path)
        .map_err(|error| CliError::Deck(format!("{}: {error}", path.display())))?;
    Deck::from_json(&text).map_err(|error| CliError::Deck(format!("{}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture_deck() -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/fixtures/deck.json")
            .display()
            .to_string()
    }

    /// A fresh directory per test.
    fn dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir()
            .join(format!("learn-cli-run-{}", std::process::id()))
            .join(name);
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    /// Runs the program; returns the exit code, stdout and stderr.
    fn go(words: &[&str], input: &str) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args = words.iter().map(|w| (*w).to_owned());
        let code = run(args, input.as_bytes(), &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn with(state: &Path, today: &str, command: &[&str]) -> Vec<String> {
        let mut words = vec![
            "--deck".to_owned(),
            fixture_deck(),
            "--state".to_owned(),
            state.display().to_string(),
            "--today".to_owned(),
            today.to_owned(),
        ];
        words.extend(command.iter().map(|w| (*w).to_owned()));
        words
    }

    fn go_with(state: &Path, today: &str, command: &[&str], input: &str) -> (i32, String, String) {
        let words = with(state, today, command);
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        go(&refs, input)
    }

    #[test]
    fn help_prints_the_usage_and_exits_0() {
        let (code, out, err) = go(&["--help"], "");
        assert_eq!(code, 0);
        assert!(out.starts_with("learn-cli --deck"), "{out}");
        assert_eq!(err, "");
    }

    #[test]
    fn a_wrong_command_line_prints_the_message_and_the_usage_and_exits_2() {
        let (code, out, err) = go(&["stats"], "");
        assert_eq!(code, 2);
        assert_eq!(out, "");
        assert!(err.contains("missing --deck"), "{err}");
        assert!(err.contains("learn-cli --deck"), "{err}");
    }

    #[test]
    fn a_missing_deck_file_exits_1_naming_it() {
        let state = dir("nodeck").join("state.json");
        let words = ["--deck", "/nowhere/deck.json", "--state"];
        let state_text = state.display().to_string();
        let (code, _, err) = go(&[words[0], words[1], words[2], &state_text, "stats"], "");
        assert_eq!(code, 1);
        assert!(err.contains("/nowhere/deck.json"), "{err}");
    }

    #[test]
    fn a_deck_that_is_not_a_deck_exits_1() {
        let folder = dir("baddeck");
        let deck = folder.join("deck.json");
        fs::write(&deck, "{ nope").unwrap();
        let (deck_text, state_text) = (
            deck.display().to_string(),
            folder.join("state.json").display().to_string(),
        );
        let (code, _, err) = go(&["--deck", &deck_text, "--state", &state_text, "stats"], "");
        assert_eq!(code, 1);
        assert!(err.contains("not a deck"), "{err}");
    }

    #[test]
    fn a_corrupt_state_exits_1_and_is_never_overwritten() {
        let state = dir("corrupt").join("state.json");
        fs::write(&state, "garbage").unwrap();
        for command in [
            &["stats"][..],
            &["new", "2"],
            &["advance", "1"],
            &["review"],
        ] {
            let (code, _, err) = go_with(&state, "2026-10-05", command, "");
            assert_eq!(code, 1, "{command:?}");
            assert!(err.contains("left untouched"), "{err}");
            assert_eq!(fs::read_to_string(&state).unwrap(), "garbage");
        }
    }

    #[test]
    fn new_starts_terms_by_rank_and_saves_without_asking() {
        let state = dir("new").join("state.json");
        let (code, out, _) = go_with(&state, "2026-10-05", &["new", "3"], "");
        assert_eq!(code, 0);
        assert!(out.contains("Started 3 new terms"), "{out}");
        assert!(!out.contains("Did you know"), "{out}");
        let saved = store::load_state(&state, &learn_core::book::DeckId::words()).unwrap();
        assert_eq!(saved.progress.len(), 3);
        assert_eq!(
            saved.last_today.map(|d| d.to_string()).as_deref(),
            Some("2026-10-05")
        );
    }

    /// What `review --seed` prints for 5 new terms, no answer typed (the
    /// first question only).
    fn first_question(seed: &str) -> String {
        // The same folder every time: the output names the state file.
        let state = dir("seed-order").join("state.json");
        go_with(&state, "2026-10-05", &["new", "5"], "");
        let (code, out, err) = go_with(&state, "2026-10-05", &["--seed", seed, "review"], "");
        assert_eq!((code, err.as_str()), (0, ""), "{out}");
        out
    }

    #[test]
    fn review_with_a_seed_repeats_its_order_and_another_seed_changes_it() {
        assert_eq!(first_question("7"), first_question("7"));
        let base = first_question("0");
        assert!(
            (1..20).any(|seed| first_question(&seed.to_string()) != base),
            "some seed asks another first question"
        );
    }

    #[test]
    fn a_bad_seed_exits_2_naming_the_option() {
        let state = dir("badseed").join("state.json");
        let (code, _, err) = go_with(&state, "2026-10-05", &["--seed", "x", "review"], "");
        assert_eq!(code, 2);
        assert!(err.contains("--seed"), "{err}");
    }

    #[test]
    fn new_with_nothing_left_says_so_and_writes_nothing() {
        let state = dir("newall").join("state.json");
        go_with(&state, "2026-10-05", &["new", "50"], "");
        let before = fs::read_to_string(&state).unwrap();
        let (code, out, _) = go_with(&state, "2026-10-05", &["new", "5"], "");
        assert_eq!(code, 0);
        assert!(out.contains("Started 0 new terms"), "{out}");
        assert_eq!(fs::read_to_string(&state).unwrap(), before);
    }

    #[test]
    fn stats_on_a_missing_state_is_the_empty_state_and_creates_no_file() {
        let state = dir("stats").join("state.json");
        let (code, out, _) = go_with(&state, "2026-10-05", &["stats"], "");
        assert_eq!(code, 0);
        assert!(out.contains("new left"), "{out}");
        assert!(out.contains("50"), "{out}");
        assert!(!state.exists());
    }

    #[test]
    fn advance_is_refused_with_its_reason() {
        let state = dir("advance").join("state.json");
        go_with(&state, "2026-10-05", &["new", "2"], "");
        let (code, out, _) = go_with(&state, "2026-10-05", &["advance", "2"], "");
        assert_eq!(code, 0);
        assert!(
            out.contains("Advanced 0 terms: terms are still due"),
            "{out}"
        );
    }

    #[test]
    fn advance_with_new_terms_left_says_so() {
        let state = dir("advance-new").join("state.json");
        let (_, out, _) = go_with(&state, "2026-10-05", &["advance", "1"], "");
        assert!(
            out.contains("Advanced 0 terms: new terms are left"),
            "{out}"
        );
    }

    #[test]
    fn advance_with_nothing_in_the_long_stages_says_so() {
        let state = dir("advance-none").join("state.json");
        go_with(&state, "2026-10-05", &["new", "50"], "");
        let answers = "\ny\n".repeat(400);
        let (code, _, _) = go_with(&state, "2026-10-05", &["review"], &answers);
        assert_eq!(code, 0);
        let (_, out, _) = go_with(&state, "2026-10-05", &["advance", "1"], "");
        assert!(
            out.contains("Advanced 0 terms: nothing in the long stages"),
            "{out}"
        );
    }

    #[test]
    fn a_day_before_the_state_s_last_day_exits_1_naming_both() {
        let state = dir("early").join("state.json");
        go_with(&state, "2026-10-09", &["new", "1"], "");
        for command in [
            &["stats"][..],
            &["review"],
            &["new", "1"],
            &["advance", "1"],
        ] {
            let (code, _, err) = go_with(&state, "2026-10-05", command, "");
            assert_eq!(code, 1, "{command:?}");
            assert!(
                err.contains("2026-10-05") && err.contains("2026-10-09"),
                "{err}"
            );
        }
    }
}
