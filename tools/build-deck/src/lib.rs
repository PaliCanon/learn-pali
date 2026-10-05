//! `build_deck` — builds the deck JSON from the DPD database.
//!
//! The command is `build-deck <dpd.db> --terms <file> -o <out.json>
//! [--strict]`. The logic lives here ([`parse_args`], [`run`]) so it is
//! tested; `main` only passes the process arguments and the exit code.
//! Exit codes: 0 built, 1 the build failed (an unreadable file, a missing
//! key, a `--strict` warning), 2 the arguments are wrong.

pub mod blank;
pub mod build;
pub mod inflections;
pub mod reader;

use std::io::Write;
use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

/// The usage line, printed with every argument error.
pub const USAGE: &str = "usage: build-deck <dpd.db> --terms <file> -o <out.json> [--strict]";

/// The parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The DPD database, opened read-only.
    pub db: PathBuf,
    /// The terms file: one lemma key per line.
    pub terms: PathBuf,
    /// Where the deck JSON is written.
    pub out: PathBuf,
    /// Fail on any warning.
    pub strict: bool,
}

/// Parses the arguments (the program name excluded).
///
/// # Errors
/// A message naming what is wrong: a missing database, `--terms` or `-o`,
/// a flag without its value, an unknown flag, a second database.
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut db, mut terms, mut out, mut strict) = (None, None, None, false);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--strict" => strict = true,
            "--terms" | "-o" => {
                let value = rest.next().ok_or_else(|| format!("{arg} needs a value"))?;
                let slot = if arg == "-o" { &mut out } else { &mut terms };
                *slot = Some(PathBuf::from(value));
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            path if db.is_none() => db = Some(PathBuf::from(path)),
            path => return Err(format!("unexpected argument {path}")),
        }
    }
    Ok(Args {
        db: db.ok_or("missing <dpd.db>")?,
        terms: terms.ok_or("missing --terms <file>")?,
        out: out.ok_or("missing -o <out.json>")?,
        strict,
    })
}

/// Runs the command; the report and the errors go to `err`. Returns the
/// exit code.
pub fn run(args: &[String], err: &mut dyn Write) -> i32 {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(message) => {
            let _ = writeln!(err, "build-deck: {message}\n{USAGE}");
            return 2;
        }
    };
    match build_file(&args, err) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(err, "build-deck: {message}");
            1
        }
    }
}

/// Reads the inputs, builds, prints the report, writes the deck. A
/// `--strict` failure writes nothing.
fn build_file(args: &Args, err: &mut dyn Write) -> Result<(), String> {
    let conn = Connection::open_with_flags(&args.db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot open {}: {e}", args.db.display()))?;
    let terms = std::fs::read_to_string(&args.terms)
        .map_err(|e| format!("cannot read {}: {e}", args.terms.display()))?;
    let keys = build::parse_terms(&terms);
    let (deck, report) = build::build(&conn, &keys, args.strict).map_err(|e| e.to_string())?;
    print_report(&report, err);
    std::fs::write(&args.out, deck.to_json() + "\n")
        .map_err(|e| format!("cannot write {}: {e}", args.out.display()))
}

/// The report on stderr: the counts, then each warning and note.
fn print_report(report: &build::Report, err: &mut dyn Write) {
    let _ = writeln!(
        err,
        "{} terms, {} with forms, {} with a blank, {} warnings, {} notes, empty cells: {}",
        report.terms,
        report.with_forms,
        report.with_blank,
        report.warnings.len(),
        report.notes.len(),
        report.empty_cells
    );
    for warning in &report.warnings {
        let _ = writeln!(err, "warning: {warning}");
    }
    for note in &report.notes {
        let _ = writeln!(err, "note: {note}");
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::reader::testdb::populate;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_arguments_are_parsed_in_any_order() {
        let want = Args {
            db: "dpd.db".into(),
            terms: "t.txt".into(),
            out: "o.json".into(),
            strict: true,
        };
        let a = parse_args(&strings(&[
            "dpd.db", "--terms", "t.txt", "-o", "o.json", "--strict",
        ]));
        let b = parse_args(&strings(&[
            "--strict", "-o", "o.json", "--terms", "t.txt", "dpd.db",
        ]));
        assert_eq!(a, Ok(want.clone()));
        assert_eq!(b, Ok(want));
        let plain = parse_args(&strings(&["d", "--terms", "t", "-o", "o"])).unwrap();
        assert!(!plain.strict);
    }

    #[test]
    fn wrong_arguments_are_refused_with_a_message() {
        let cases: [(&[&str], &str); 7] = [
            (&[], "missing <dpd.db>"),
            (&["d", "-o", "o"], "missing --terms"),
            (&["d", "--terms", "t"], "missing -o"),
            (&["d", "--terms"], "--terms needs a value"),
            (&["d", "--terms", "t", "-o"], "-o needs a value"),
            (
                &["d", "--terms", "t", "-o", "o", "--nope"],
                "unknown option --nope",
            ),
            (
                &["d", "e", "--terms", "t", "-o", "o"],
                "unexpected argument e",
            ),
        ];
        for (args, message) in cases {
            let error = parse_args(&strings(args)).unwrap_err();
            assert!(error.contains(message), "{args:?}: {error}");
        }
    }

    /// A scratch directory with a file database, a terms file and an output path.
    fn scratch(name: &str, terms: &str) -> (PathBuf, Vec<String>) {
        let dir = std::env::temp_dir().join(format!("build-deck-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        populate(&Connection::open(dir.join("dpd.db")).unwrap());
        fs::write(dir.join("terms.txt"), terms).unwrap();
        let path = |file: &str| dir.join(file).to_string_lossy().into_owned();
        let args = vec![
            path("dpd.db"),
            "--terms".into(),
            path("terms.txt"),
            "-o".into(),
            path("out.json"),
        ];
        (dir, args)
    }

    fn run_to_string(args: &[String]) -> (i32, String) {
        let mut err = Vec::new();
        let code = run(args, &mut err);
        (code, String::from_utf8(err).unwrap())
    }

    #[test]
    fn a_build_writes_the_deck_and_reports_on_stderr() {
        let (dir, args) = scratch("ok", "# glue\nkho\nsati 1.1\n");
        let (code, report) = run_to_string(&args);
        assert_eq!(code, 0, "{report}");
        assert!(report.contains("2 terms"), "{report}");
        let text = fs::read_to_string(dir.join("out.json")).unwrap();
        let deck = learn_core::deck::Deck::from_json(&text).unwrap();
        assert_eq!(deck.terms.len(), 2);
        // a second run writes the same bytes
        assert_eq!(run_to_string(&args).0, 0);
        assert_eq!(fs::read_to_string(dir.join("out.json")).unwrap(), text);
    }

    #[test]
    fn the_command_prints_the_empty_cell_count_and_strict_still_passes() {
        let (dir, mut args) = scratch("empty-cells", "kho\n");
        Connection::open(dir.join("dpd.db"))
            .unwrap()
            .execute(
                "UPDATE dpd_headwords SET inflections_html = \"<table class='inflection'><tr>\
                 <td title='masc nom sg'></td><td title='masc nom pl'>x<b>o</b></td>\
                 </tr></table>\" WHERE lemma_1 = 'kho'",
                [],
            )
            .unwrap();
        args.push("--strict".into());
        let (code, report) = run_to_string(&args);
        assert_eq!(code, 0, "{report}");
        assert!(report.contains("empty cells: 1"), "{report}");
        assert!(report.contains("0 warnings"), "{report}");
    }

    #[test]
    fn strict_fails_on_a_warning_and_writes_nothing() {
        let (dir, mut args) = scratch("strict", "kho\nbadtable 1\n");
        let (lenient, _) = run_to_string(&args);
        assert_eq!(lenient, 0);
        fs::remove_file(dir.join("out.json")).unwrap();
        args.push("--strict".into());
        let (code, report) = run_to_string(&args);
        assert_eq!(code, 1);
        assert!(report.contains("badtable 1"), "{report}");
        assert!(!Path::new(&dir.join("out.json")).exists());
    }

    #[test]
    fn a_missing_key_a_missing_file_and_bad_arguments_have_their_codes() {
        let (dir, args) = scratch("errors", "kho\nnope 9\n");
        let (code, report) = run_to_string(&args);
        assert_eq!(code, 1);
        assert!(report.contains("nope 9"), "{report}");
        assert!(!dir.join("out.json").exists());

        let mut no_db = args.clone();
        no_db[0] = dir.join("absent.db").to_string_lossy().into_owned();
        assert_eq!(run_to_string(&no_db).0, 1);

        let mut no_terms = args.clone();
        no_terms[2] = dir.join("absent.txt").to_string_lossy().into_owned();
        assert_eq!(run_to_string(&no_terms).0, 1);

        let (code, usage) = run_to_string(&strings(&["--bogus"]));
        assert_eq!(code, 2);
        assert!(usage.contains(USAGE), "{usage}");
    }

    #[test]
    fn an_empty_terms_file_fails() {
        let (_, args) = scratch("empty", "# nothing\n\n");
        assert_eq!(run_to_string(&args).0, 1);
    }
}
