//! `build_deck` — builds the deck JSON from the DPD database.
//!
//! The command is `build-deck <dpd.db> --terms <file> -o <out.json>
//! [--strict] [--api <url> [--cache <dir>]]`. The logic lives here
//! ([`parse_args`], [`run`]) so it is tested; `main` only passes the
//! process arguments and the exit code.
//! Exit codes: 0 built, 1 the build failed (an unreadable file, a missing
//! key, a `--strict` warning, `--api` without a token), 2 the arguments are
//! wrong.
//!
//! With `--api`, the examples' translations are fetched from the PaliCanon
//! API at that base URL ([`translate`]), with the bearer token in
//! `PALI_API_TOKEN`; `--cache` keeps the answers in a folder so a rebuild
//! fetches nothing twice. The token is never printed.

pub mod api;
/// The passage modules now live in `learn_passage` (062); the paths stay.
pub use learn_passage::{blank, segments, source};
pub mod build;
pub mod inflections;
pub mod passages;
pub mod reader;
pub mod script;
pub mod translate;
pub mod words;

use std::io::Write;
use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

use crate::api::{Cached, HttpFetch};

/// The usage line, printed with every argument error.
pub const USAGE: &str = "usage: build-deck <dpd.db> --terms <file> -o <out.json> [--strict] [--api <url> [--cache <dir>]]";

/// The environment variable holding the API token.
pub const TOKEN_VAR: &str = "PALI_API_TOKEN";

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
    /// The PaliCanon API's base URL, to fetch the examples' translations.
    pub api: Option<String>,
    /// Where the API's answers are kept between runs.
    pub cache: Option<PathBuf>,
}

/// Parses the arguments (the program name excluded).
///
/// # Errors
/// A message naming what is wrong: a missing database, `--terms` or `-o`,
/// a flag without its value, an unknown flag, a second database.
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut db, mut terms, mut out, mut strict) = (None, None, None, false);
    let (mut api, mut cache) = (None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--strict" => strict = true,
            "--terms" | "-o" | "--api" | "--cache" => {
                let value = rest.next().ok_or_else(|| format!("{arg} needs a value"))?;
                match arg.as_str() {
                    "-o" => out = Some(PathBuf::from(value)),
                    "--terms" => terms = Some(PathBuf::from(value)),
                    "--api" => api = Some(value.clone()),
                    _ => cache = Some(PathBuf::from(value)),
                }
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            path if db.is_none() => db = Some(PathBuf::from(path)),
            path => return Err(format!("unexpected argument {path}")),
        }
    }
    let args = Args {
        db: db.ok_or("missing <dpd.db>")?,
        terms: terms.ok_or("missing --terms <file>")?,
        out: out.ok_or("missing -o <out.json>")?,
        strict,
        api,
        cache,
    };
    if args.cache.is_some() && args.api.is_none() {
        return Err("--cache needs --api".to_string());
    }
    Ok(args)
}

/// Runs the command, the token read from [`TOKEN_VAR`]; the report and the
/// errors go to `err`. Returns the exit code.
pub fn run(args: &[String], err: &mut dyn Write) -> i32 {
    run_with_token(args, std::env::var(TOKEN_VAR).ok(), err)
}

/// [`run`] with the API token given (only read with `--api`).
pub fn run_with_token(args: &[String], token: Option<String>, err: &mut dyn Write) -> i32 {
    if args.first().is_some_and(|first| first == "passages") {
        return match build_passages(&args[1..], err) {
            Ok(()) => 0,
            Err(message) => {
                let _ = writeln!(err, "build-deck: {message}\n{PASSAGES_USAGE}");
                2
            }
        };
    }
    if args.first().is_some_and(|first| first == "devanagari") {
        return match build_script(&args[1..]) {
            Ok(()) => 0,
            Err(message) => {
                let _ = writeln!(err, "build-deck: {message}\n{SCRIPT_USAGE}");
                2
            }
        };
    }
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(message) => {
            let _ = writeln!(err, "build-deck: {message}\n{USAGE}");
            return 2;
        }
    };
    match build_file(&args, token, err) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(err, "build-deck: {message}");
            1
        }
    }
}

/// Reads the inputs, builds, prints the report, writes the deck. A
/// `--strict` failure writes nothing.
/// The usage of the `passages` subcommand (068: every dictionary entry's
/// line from the suttas, from the local corpus databases, no API).
pub const PASSAGES_USAGE: &str = "usage: build-deck passages --dictionary <dictionary.db> --corpus <corpus.db> [--corpus-fr <db>] [--corpus-vi <db>] -o <passages.db>";

/// `build-deck passages …`: the passages asset.
fn build_passages(args: &[String], err: &mut dyn Write) -> Result<(), String> {
    let (mut dictionary, mut corpus, mut fr, mut vi, mut out) = (None, None, None, None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{arg} needs a value"))?;
        let path = PathBuf::from(value);
        match arg.as_str() {
            "--dictionary" => dictionary = Some(path),
            "--corpus" => corpus = Some(path),
            "--corpus-fr" => fr = Some(path),
            "--corpus-vi" => vi = Some(path),
            "-o" => out = Some(path),
            other => return Err(format!("unknown option {other}")),
        }
    }
    let dictionary = dictionary.ok_or("missing --dictionary <dictionary.db>")?;
    let corpus = corpus.ok_or("missing --corpus <corpus.db>")?;
    let out = out.ok_or("missing -o <passages.db>")?;
    if out.exists() {
        return Err(format!("{} exists already", out.display()));
    }
    let dict = Connection::open_with_flags(&dictionary, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot open {}: {e}", dictionary.display()))?;
    let entries = passages::read_entries(&dict).map_err(|e| e.to_string())?;
    let mut fetch = passages::CorpusFetch::open(&corpus, fr.as_deref(), vi.as_deref())?;
    // Built beside, moved into place once complete: a build that stops
    // leaves no asset that looks whole (review 068).
    let part = out.with_extension("part");
    let _ = std::fs::remove_file(&part);
    let db =
        Connection::open(&part).map_err(|e| format!("cannot create {}: {e}", part.display()))?;
    let counts = passages::build(&entries, &mut fetch, &db).map_err(|e| e.to_string())?;
    drop(db);
    // The file itself is read back (not the connection that wrote it), and
    // what it holds must be what the build counted.
    let written = Connection::open_with_flags(&part, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot reopen {}: {e}", part.display()))?;
    let read = passages::check_asset(&written)?;
    drop(written);
    if (read.entries, read.found) != (counts.entries, counts.found) {
        return Err(format!(
            "the asset holds {} rows, {} found; the build counted {}, {}",
            read.entries, read.found, counts.entries, counts.found
        ));
    }
    // `out` was refused above when it existed, so the move never overwrites
    // (and build-deck runs on Unix; a stale `.part` is replaced).
    std::fs::rename(&part, &out)
        .map_err(|e| format!("cannot move {} into place: {e}", part.display()))?;
    let _ = writeln!(
        err,
        "build-deck passages: {} entries, {} found, misses {:?}, lines {:?}",
        counts.entries, counts.found, counts.misses, counts.lines
    );
    Ok(())
}

/// The usage of the `devanagari` subcommand (063b: a script deck from the
/// reviewed inventory table, no DPD and no API).
pub const SCRIPT_USAGE: &str = "usage: build-deck devanagari --inventory <table.tsv> -o <out.json>";

/// `build-deck devanagari --inventory <tsv> -o <out.json>`: the script deck
/// of the table, written as the site's deck JSON.
fn build_script(args: &[String]) -> Result<(), String> {
    let (mut inventory, mut out) = (None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{arg} needs a value"))?;
        match arg.as_str() {
            "--inventory" => inventory = Some(PathBuf::from(value)),
            "-o" => out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    let inventory = inventory.ok_or("missing --inventory <table.tsv>")?;
    let out = out.ok_or("missing -o <out.json>")?;
    let text = std::fs::read_to_string(&inventory)
        .map_err(|e| format!("cannot read {}: {e}", inventory.display()))?;
    let items = script::parse_inventory(&text)?;
    let source = format!("Devanāgarī inventory {}", inventory.display());
    let deck = script::script_deck(&items, &source);
    std::fs::write(&out, deck.to_json() + "\n")
        .map_err(|e| format!("cannot write {}: {e}", out.display()))
}

fn build_file(args: &Args, token: Option<String>, err: &mut dyn Write) -> Result<(), String> {
    let fetch = match &args.api {
        Some(base) => Some(api_fetch(base, token, args.cache.as_ref())?),
        None => None,
    };
    let conn = Connection::open_with_flags(&args.db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot open {}: {e}", args.db.display()))?;
    let terms = std::fs::read_to_string(&args.terms)
        .map_err(|e| format!("cannot read {}: {e}", args.terms.display()))?;
    let keys = build::parse_terms(&terms);
    let (mut deck, mut report) =
        build::build(&conn, &keys, args.strict).map_err(|e| e.to_string())?;
    if let Some(mut fetch) = fetch {
        translate::translate_terms(&mut deck.terms, &mut fetch, &mut report);
    }
    print_report(&report, err);
    std::fs::write(&args.out, deck.to_json() + "\n")
        .map_err(|e| format!("cannot write {}: {e}", args.out.display()))
}

/// The API client: the token required, the cache folder created.
fn api_fetch(
    base: &str,
    token: Option<String>,
    cache: Option<&PathBuf>,
) -> Result<Cached<HttpFetch>, String> {
    let token = token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| format!("--api needs the API token in {TOKEN_VAR}"))?;
    if let Some(dir) = cache {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    Ok(Cached::new(HttpFetch::new(base, token), cache.cloned()))
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
    let _ = writeln!(
        err,
        "words: {} of {} forms found",
        report.words_found, report.word_forms
    );
    if !report.translated.is_empty() {
        let counts: Vec<String> = report
            .translated
            .iter()
            .map(|(lang, count)| format!("{lang} {count}"))
            .collect();
        let _ = writeln!(err, "translations: {}", counts.join(", "));
    }
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
            api: None,
            cache: None,
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
    fn the_api_and_its_cache_are_optional_arguments() {
        let args = parse_args(&strings(&[
            "d",
            "--terms",
            "t",
            "-o",
            "o",
            "--cache",
            "c",
            "--api",
            "https://x",
        ]))
        .unwrap();
        assert_eq!(args.api.as_deref(), Some("https://x"));
        assert_eq!(args.cache, Some(PathBuf::from("c")));
        for (list, message) in [
            (
                &["d", "--terms", "t", "-o", "o", "--api"][..],
                "--api needs a value",
            ),
            (
                &["d", "--terms", "t", "-o", "o", "--api", "u", "--cache"][..],
                "--cache needs a value",
            ),
            (
                &["d", "--terms", "t", "-o", "o", "--cache", "c"][..],
                "--cache needs --api",
            ),
        ] {
            let error = parse_args(&strings(list)).unwrap_err();
            assert!(error.contains(message), "{list:?}: {error}");
        }
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
        assert!(report.contains("words: 1 of 7 forms found"), "{report}");
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

    const SECRET: &str = "tok-SECRET-0987654321";

    #[test]
    fn the_api_without_a_token_fails_and_writes_nothing() {
        let (dir, mut args) = scratch("no-token", "kho\n");
        args.extend(strings(&["--api", "http://127.0.0.1:9"]));
        let mut err = Vec::new();
        let code = run_with_token(&args, None, &mut err);
        let report = String::from_utf8(err).unwrap();
        assert_eq!(code, 1, "{report}");
        assert!(report.contains(TOKEN_VAR), "{report}");
        assert!(!dir.join("out.json").exists());
    }

    #[test]
    fn an_api_that_cannot_be_reached_skips_the_translations_and_never_shows_the_token() {
        let (dir, mut args) = scratch("unreachable", "kho\nsati 1.1\n");
        args.extend(strings(&["--api", "http://127.0.0.1:9"]));
        let mut err = Vec::new();
        let code = run_with_token(&args, Some(SECRET.to_string()), &mut err);
        let report = String::from_utf8(err).unwrap();
        assert_eq!(code, 0, "{report}");
        assert!(
            report.contains("translations: en 0, fr 0, vi 0"),
            "{report}"
        );
        assert!(report.contains("sati 1.1: no translation: "), "{report}");
        assert!(
            report.contains("127.0.0.1:9") || report.contains("/v1/sutta/an4.189"),
            "{report}"
        );
        assert!(!report.contains(SECRET), "{report}");
        assert!(!report.contains("Bearer"), "{report}");
        let deck = fs::read_to_string(dir.join("out.json")).unwrap();
        assert!(!deck.contains(SECRET));
        assert!(!deck.contains("translations"), "{deck}");
    }

    #[test]
    fn without_the_api_nothing_is_fetched_and_no_translation_line_is_printed() {
        let (_, args) = scratch("offline", "kho\n");
        let mut err = Vec::new();
        assert_eq!(run_with_token(&args, Some(SECRET.to_string()), &mut err), 0);
        let report = String::from_utf8(err).unwrap();
        assert!(!report.contains("translations:"), "{report}");
    }

    #[test]
    fn an_empty_terms_file_fails() {
        let (_, args) = scratch("empty", "# nothing\n\n");
        assert_eq!(run_to_string(&args).0, 1);
    }
}
