//! The command end to end: the real binary, the committed fixture deck, a
//! state file in a temporary folder and answers piped to the standard input.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use learn_core::book::{Book, DeckId};
use learn_core::date::Date;
use learn_core::deck::{Deck, Term, TermId};
use learn_core::facet::{Facet, FacetSet};
use learn_core::progress::{Progress, State};
use learn_core::stage::Stage;

const TODAY: &str = "2026-10-05";

/// What a run printed and how it ended.
struct Run {
    code: i32,
    out: String,
    err: String,
}

/// One test's folder, deck and state paths.
struct Env {
    dir: PathBuf,
    deck: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let dir = std::env::temp_dir()
            .join(format!("learn-cli-t8-{}", std::process::id()))
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let deck = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/fixtures/deck.json");
        Env { dir, deck }
    }

    fn state(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    fn load(&self) -> State {
        State::from_json(&fs::read_to_string(self.state()).unwrap()).unwrap()
    }

    /// Runs `learn-cli` on this folder's deck and state.
    fn run(&self, today: &str, command: &[&str], input: &str) -> Run {
        self.run_with_deck(&self.deck, today, command, input)
    }

    fn run_with_deck(&self, deck: &Path, today: &str, command: &[&str], input: &str) -> Run {
        let mut child = Command::new(env!("CARGO_BIN_EXE_learn-cli"))
            .arg("--deck")
            .arg(deck)
            .arg("--state")
            .arg(self.state())
            .args(["--today", today])
            .args(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        match child.stdin.take().unwrap().write_all(input.as_bytes()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                // The program exited before reading all the input; this is expected.
            }
            Err(e) => panic!("{}", e),
        }
        let output = child.wait_with_output().unwrap();
        Run {
            code: output.status.code().unwrap(),
            out: String::from_utf8(output.stdout).unwrap(),
            err: String::from_utf8(output.stderr).unwrap(),
        }
    }
}

/// `n` times: Enter, then `y`.
fn yes(n: usize) -> String {
    "\ny\n".repeat(n)
}

#[test]
fn new_creates_the_state_with_the_first_terms_at_day() {
    let env = Env::new("new");
    let run = env.run(TODAY, &["new", "3"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("Started 3 new terms"), "{}", run.out);
    let state = env.load();
    assert_eq!(state.progress.len(), 3);
    assert!(state.progress.iter().all(|p| p.stage == Stage::Day));
    let ids: Vec<&str> = state.progress.iter().map(|p| p.term.0.as_str()).collect();
    assert_eq!(&ids[..2], ["kho", "ca 2.1"]);
}

#[test]
fn review_with_piped_answers_advances_the_state_file() {
    let env = Env::new("review");
    env.run(TODAY, &["new", "3"], "");
    let run = env.run(TODAY, &["review"], &yes(40));
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("Session over"), "{}", run.out);
    assert!(run.out.contains("0 failed"), "{}", run.out);
    let state = env.load();
    assert!(state.progress.iter().all(|p| p.stage == Stage::Week));
    assert_eq!(
        state.last_today.map(|d| d.to_string()).as_deref(),
        Some(TODAY)
    );
}

#[test]
fn the_week_simulated_with_today() {
    let env = Env::new("week");
    env.run(TODAY, &["new", "3"], "");
    env.run(TODAY, &["review"], &yes(40));
    // Not due yet the next day.
    let early = env.run("2026-10-06", &["review"], &yes(40));
    assert!(early.out.contains("Nothing is due"), "{}", early.out);
    // Due a week later: the stage goes up again.
    let later = env.run("2026-10-12", &["review"], &yes(40));
    assert_eq!(later.code, 0, "{}", later.err);
    assert!(env.load().progress.iter().all(|p| p.stage == Stage::Month));
}

#[test]
fn a_wrong_answer_returns_the_term_to_day_and_counts_a_lapse() {
    let env = Env::new("lapse");
    env.run(TODAY, &["new", "1"], "");
    let run = env.run(TODAY, &["review"], "\nn\n\ny\n\ny\n\ny\n");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("(practice)"), "{}", run.out);
    let state = env.load();
    assert_eq!(state.progress[0].stage, Stage::Day);
    assert_eq!(state.progress[0].lapses, 1);
}

#[test]
fn the_end_of_input_leaves_the_question_due_for_the_next_run() {
    let env = Env::new("eof");
    env.run(TODAY, &["new", "1"], "");
    let first = env.run(TODAY, &["review"], &yes(1));
    assert_eq!(first.code, 0, "{}", first.err);
    assert!(first.out.contains("End of input"), "{}", first.out);
    assert_eq!(env.load().progress[0].stage, Stage::Day);
    let second = env.run(TODAY, &["review"], &yes(10));
    assert!(second.out.contains("Session over"), "{}", second.out);
    assert_eq!(env.load().progress[0].stage, Stage::Week);
}

#[test]
fn stats_prints_the_six_stages_and_the_totals() {
    let env = Env::new("stats");
    env.run(TODAY, &["new", "3"], "");
    let run = env.run(TODAY, &["stats"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    for name in ["Day", "Week", "Month", "Quarter", "HalfYear", "Year"] {
        assert!(
            run.out.lines().any(|line| line.starts_with(name)),
            "{name} in {}",
            run.out
        );
    }
    let line = |name: &str| {
        run.out
            .lines()
            .find(|l| l.starts_with(name))
            .unwrap()
            .to_owned()
    };
    assert!(line("Day").ends_with('3'));
    assert!(line("due today").ends_with('3'));
    assert!(line("new left").ends_with("47"));
}

#[test]
fn no_state_file_is_the_empty_state_and_stats_does_not_create_one() {
    let env = Env::new("empty");
    let run = env.run(TODAY, &["stats"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out
            .lines()
            .any(|l| l.starts_with("new left") && l.ends_with("50"))
    );
    assert!(!env.state().exists());
}

#[test]
fn a_corrupt_state_stops_every_command_and_keeps_its_bytes() {
    let env = Env::new("corrupt");
    fs::write(env.state(), b"{ this is not a state").unwrap();
    for command in [
        &["review"][..],
        &["stats"],
        &["new", "2"],
        &["advance", "2"],
    ] {
        let run = env.run(TODAY, command, &yes(5));
        assert_eq!(run.code, 1, "{command:?}");
        assert!(run.err.contains("left untouched"), "{}", run.err);
        assert_eq!(fs::read(env.state()).unwrap(), b"{ this is not a state");
    }
}

#[test]
fn a_day_before_the_last_one_exits_1_naming_both_dates() {
    let env = Env::new("early");
    env.run("2026-10-09", &["new", "1"], "");
    let before = fs::read(env.state()).unwrap();
    let run = env.run(TODAY, &["review"], &yes(5));
    assert_eq!(run.code, 1);
    assert!(run.err.contains("2026-10-05"), "{}", run.err);
    assert!(run.err.contains("2026-10-09"), "{}", run.err);
    assert_eq!(fs::read(env.state()).unwrap(), before);
}

#[test]
fn advance_is_refused_while_terms_are_due_and_changes_nothing() {
    let env = Env::new("refused");
    env.run(TODAY, &["new", "2"], "");
    let before = fs::read(env.state()).unwrap();
    let run = env.run(TODAY, &["advance", "2"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("Advanced 0 terms: terms are still due"),
        "{}",
        run.out
    );
    assert_eq!(fs::read(env.state()).unwrap(), before);
}

#[test]
fn advance_brings_long_stage_terms_forward_once_everything_is_done() {
    let env = Env::new("advance");
    env.run(TODAY, &["new", "50"], "");
    env.run(TODAY, &["review"], &yes(400));
    env.run("2026-10-12", &["review"], &yes(400));
    // Everything is in Month, due 2026-11-11; nothing is due on 2026-10-12.
    let run = env.run("2026-10-12", &["advance", "4"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("Advanced 4 terms to 2026-10-12"),
        "{}",
        run.out
    );
    let due_now = env
        .load()
        .progress
        .iter()
        .filter(|p| p.due.to_string() == "2026-10-12")
        .count();
    assert_eq!(due_now, 4);
    let review = env.run("2026-10-12", &["review"], &yes(100));
    assert!(review.out.contains("Session over"), "{}", review.out);
    let quarter = env
        .load()
        .progress
        .iter()
        .filter(|p| p.stage == Stage::Quarter)
        .count();
    assert_eq!(quarter, 4);
}

#[test]
fn help_prints_the_usage_and_exits_0() {
    let env = Env::new("help");
    let run = env.run(TODAY, &["--help"], "");
    assert_eq!(run.code, 0);
    assert!(run.out.contains("learn-cli --deck"), "{}", run.out);
    assert_eq!(run.err, "");
}

#[test]
fn a_wrong_command_line_exits_2_with_the_usage() {
    let env = Env::new("usage");
    let run = env.run(TODAY, &["frobnicate"], "");
    assert_eq!(run.code, 2);
    assert!(run.err.contains("frobnicate"), "{}", run.err);
    assert!(run.err.contains("learn-cli --deck"), "{}", run.err);
}

#[test]
fn a_wrong_deck_warns_about_the_terms_it_does_not_know() {
    let env = Env::new("orphans");
    env.run(TODAY, &["new", "3"], "");
    let other = env.dir.join("other-deck.json");
    fs::write(
        &other,
        r#"{"version":1,"source":"test","terms":[
            {"id":"zzz","lemma":"zzz","pos":"ind","meaning":{"en":"nothing"},"forms":[],"rank":1}]}"#,
    )
    .unwrap();
    let run = env.run_with_deck(&other, TODAY, &["review"], &yes(5));
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("3 terms of the state are not in the deck"),
        "{}",
        run.out
    );
    assert!(run.out.contains("wrong --deck"), "{}", run.out);
    let stats = env.run_with_deck(&other, TODAY, &["stats"], "");
    assert!(
        stats
            .out
            .lines()
            .any(|l| l.starts_with("orphans") && l.ends_with('3'))
    );
}

#[test]
fn a_wrong_deck_makes_stats_write_nothing() {
    let env = Env::new("orphans-unwritten");
    env.run(TODAY, &["new", "2"], "");
    let other = env.dir.join("other-deck.json");
    fs::write(
        &other,
        r#"{"version":1,"source":"test","terms":[
            {"id":"zzz","lemma":"zzz","pos":"ind","meaning":{"en":"nothing"},"forms":[],"rank":1}]}"#,
    )
    .unwrap();
    // A known, older mtime: a rewrite (even of the same bytes) would move it.
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    fs::File::options()
        .write(true)
        .open(env.state())
        .unwrap()
        .set_modified(old)
        .unwrap();
    let bytes = fs::read(env.state()).unwrap();
    let run = env.run_with_deck(&other, TODAY, &["stats"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    // `stats` shows the orphans as a line (the warning is the review's).
    assert!(
        run.out
            .lines()
            .any(|l| l.starts_with("orphans") && l.ends_with('2')),
        "{}",
        run.out
    );
    assert_eq!(fs::read(env.state()).unwrap(), bytes, "same bytes");
    assert_eq!(
        fs::metadata(env.state()).unwrap().modified().unwrap(),
        old,
        "the file was not rewritten"
    );
    assert!(!env.dir.join("state.json.tmp").exists(), "no temp file");
}

/// Every term of the fixture deck is started and not due, except
/// `bhikkhave` (forms and a blank: four facets): it is at `stage`, every
/// facet passed, due yesterday, so it has nothing left to ask.
fn fully_passed_due_term(env: &Env, stage: Stage) {
    let deck = Deck::from_json(&fs::read_to_string(&env.deck).unwrap()).unwrap();
    let today = Date::parse(TODAY).unwrap();
    let mut saved = State::empty();
    for term in &deck.terms {
        let mut progress = Progress::new(term.id.clone(), today);
        progress.due = today + 100;
        if term.id == TermId("bhikkhave".to_owned()) {
            progress.stage = stage;
            progress.passed = [Facet::Meaning, Facet::Recall, Facet::Forms, Facet::Context]
                .into_iter()
                .collect::<FacetSet>();
            progress.due = Date::parse("2026-10-04").unwrap();
        }
        saved.progress.push(progress);
    }
    fs::write(env.state(), saved.to_json()).unwrap();
}

#[test]
fn every_command_saves_a_promotion_made_at_the_session_s_start() {
    // Week -> Month (due today + 30) for the commands that ask nothing;
    // `advance` needs no long-stage term left, so it starts from Day.
    let cases: [(&str, &[&str], Stage, Stage, u32); 4] = [
        ("stats", &["stats"], Stage::Week, Stage::Month, 30),
        ("new", &["new", "5"], Stage::Week, Stage::Month, 30),
        ("review", &["review"], Stage::Week, Stage::Month, 30),
        ("advance", &["advance", "1"], Stage::Day, Stage::Week, 7),
    ];
    for (name, command, before, after, days) in cases {
        let env = Env::new(&format!("promoted-{name}"));
        fully_passed_due_term(&env, before);
        let first = env.run(TODAY, command, "");
        assert_eq!(first.code, 0, "{name}: {}", first.err);
        let saved = env.load();
        let term = saved.get(&TermId("bhikkhave".to_owned())).unwrap();
        assert_eq!(term.stage, after, "{name}: promoted on disk");
        assert_eq!(
            term.due,
            Date::parse(TODAY).unwrap() + days,
            "{name}: due moved"
        );
        assert_eq!(term.passed, FacetSet::empty(), "{name}");
        assert_eq!(
            saved.last_today,
            Some(Date::parse(TODAY).unwrap()),
            "{name}: last_today is the day of the write"
        );
        let bytes = fs::read(env.state()).unwrap();
        let second = env.run(TODAY, command, "");
        assert_eq!(second.code, 0, "{name}: {}", second.err);
        assert_eq!(fs::read(env.state()).unwrap(), bytes, "{name}: run 2");
    }
}

#[test]
fn a_write_failure_exits_1_with_a_message_and_leaves_no_file() {
    let env = Env::new("write-failure");
    // The folder of the state file does not exist: it cannot be written.
    let state = env.dir.join("sub").join("state.json");
    let run = Command::new(env!("CARGO_BIN_EXE_learn-cli"))
        .arg("--deck")
        .arg(&env.deck)
        .arg("--state")
        .arg(&state)
        .args(["--today", TODAY, "new", "1"])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    let err = String::from_utf8(run.stderr).unwrap();
    assert!(err.contains("state file"), "{err}");
    assert!(err.contains("sub"), "{err}");
    assert!(!state.exists());
    assert!(!env.dir.join("sub").exists(), "no folder created");
    let left: Vec<_> = fs::read_dir(&env.dir).unwrap().collect();
    assert_eq!(
        left.len(),
        0,
        "no state file and no .tmp anywhere: {left:?}"
    );
}

#[test]
fn a_deck_with_fewer_facets_promotes_and_stats_writes() {
    let env = Env::new("fewer-facets");
    // `kho` of the fixture has a blanked example: Meaning, Recall, Context.
    env.run(TODAY, &["new", "1"], "");
    let kho = TermId("kho".to_owned());
    let today = Date::parse(TODAY).unwrap();
    let mut state = env.load();
    assert_eq!(state.progress[0].term, kho);
    state.progress[0].stage = Stage::Week;
    state.progress[0].passed = [Facet::Meaning, Facet::Recall].into_iter().collect();
    state.progress[0].due = today;
    fs::write(env.state(), state.to_json()).unwrap();
    // The same term, now without example (so without Context) or forms.
    let poorer = env.dir.join("poorer-deck.json");
    fs::write(
        &poorer,
        r#"{"version":1,"source":"test","terms":[
            {"id":"kho","lemma":"kho","pos":"ind","meaning":{"en":"indeed"},"forms":[],"rank":1}]}"#,
    )
    .unwrap();
    // A known, older mtime: only a rewrite moves it.
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    fs::File::options()
        .write(true)
        .open(env.state())
        .unwrap()
        .set_modified(old)
        .unwrap();
    let run = env.run_with_deck(&poorer, TODAY, &["stats"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert_ne!(
        fs::metadata(env.state()).unwrap().modified().unwrap(),
        old,
        "the file was rewritten"
    );
    let saved = env.load();
    let term = saved.get(&kho).unwrap();
    assert_eq!(term.stage, Stage::Month);
    assert_eq!(term.due, today + 30);
    assert_eq!(term.passed, FacetSet::empty());
    assert_eq!(saved.last_today, Some(today));
}

#[test]
fn a_wrong_deck_with_large_input_does_not_panic_on_broken_pipe() {
    let env = Env::new("broken-pipe-test");
    env.run(TODAY, &["new", "3"], "");
    let other = env.dir.join("other-deck.json");
    fs::write(
        &other,
        r#"{"version":1,"source":"test","terms":[
            {"id":"zzz","lemma":"zzz","pos":"ind","meaning":{"en":"nothing"},"forms":[],"rank":1}]}"#,
    )
    .unwrap();
    // Create a large input (~200 KB) that cannot fit in a pipe buffer.
    // The program will exit early because of the wrong deck, and the write
    // will fail with BrokenPipe, which should be ignored gracefully.
    let large_input = "y\n".repeat(100_000);
    let run = env.run_with_deck(&other, TODAY, &["review"], &large_input);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("3 terms of the state are not in the deck"),
        "{}",
        run.out
    );
}

fn zz() -> DeckId {
    DeckId("zz".to_owned())
}

impl Env {
    fn book(&self) -> Book {
        Book::from_json(&fs::read_to_string(self.state()).unwrap()).unwrap()
    }

    fn text(&self) -> String {
        fs::read_to_string(self.state()).unwrap()
    }
}

/// An added word, as the site's "add a word" would store it.
fn added_word() -> Term {
    let deck = Deck::from_json(
        r#"{"version":1,"source":"test","terms":[
            {"id":"sānuvajja","lemma":"sānuvajja","pos":"ind","meaning":{"en":"blameworthy"},
             "forms":[],"rank":4294967295}]}"#,
    )
    .unwrap();
    deck.terms[0].clone()
}

#[test]
fn a_words_only_file_stays_flat_byte_for_byte_after_new() {
    let stamp_1 = State::empty()
        .to_json()
        .replacen("\"version\": 2", "\"version\": 1", 1);
    let mut with_added = State::empty();
    with_added.added.push(added_word());
    for (name, start) in [("flat-1", stamp_1), ("flat-2", with_added.to_json())] {
        let env = Env::new(name);
        fs::write(env.state(), &start).unwrap();
        let run = env.run(TODAY, &["new", "3"], "");
        assert_eq!(run.code, 0, "{name}: {}", run.err);
        let text = env.text();
        assert_eq!(text, env.load().to_json(), "{name}: the state's own text");
        assert_eq!(env.load().progress.len(), 3, "{name}");
        assert!(
            text.contains("\"version\": 1") || text.contains("\"version\": 2"),
            "{name}: {text}"
        );
        assert!(!text.contains("\"decks\""), "{name}: {text}");
    }
}

#[test]
fn new_on_another_deck_makes_the_file_stamp_3_and_leaves_words_whole() {
    let env = Env::new("deck-id-new");
    env.run(TODAY, &["new", "1"], "");
    let words_before = env.load();
    let run = env.run(TODAY, &["new", "2", "--deck-id", "zz"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("Started 2 new terms"), "{}", run.out);
    assert!(env.text().contains("\"version\": 3"), "{}", env.text());
    let book = env.book();
    assert_eq!(book.state(&DeckId::words()), words_before);
    assert_eq!(book.state(&zz()).progress.len(), 2);
}

#[test]
fn stats_on_a_deck_the_file_lacks_prints_all_new_and_writes_nothing() {
    let env = Env::new("deck-id-stats");
    env.run(TODAY, &["new", "2"], "");
    let before = fs::read(env.state()).unwrap();
    let run = env.run(TODAY, &["stats", "--deck-id", "zz"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out
            .lines()
            .any(|l| l.starts_with("new left") && l.ends_with("50")),
        "{}",
        run.out
    );
    assert_eq!(fs::read(env.state()).unwrap(), before);
}

#[test]
fn the_words_commands_leave_another_decks_entry_intact() {
    let env = Env::new("deck-id-intact");
    env.run(TODAY, &["new", "2", "--deck-id", "zz"], "");
    let zz_before = env.book().state(&zz());
    assert_eq!(zz_before.progress.len(), 2);
    let run = env.run(TODAY, &["new", "1"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    let book = env.book();
    assert_eq!(book.state(&zz()), zz_before);
    assert_eq!(book.state(&DeckId::words()).progress.len(), 1);
}

#[test]
fn review_saves_into_the_chosen_entry() {
    let env = Env::new("deck-id-review");
    env.run(TODAY, &["new", "3", "--deck-id", "zz"], "");
    let run = env.run(TODAY, &["review", "--deck-id", "zz"], &yes(40));
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("Session over"), "{}", run.out);
    let book = env.book();
    let progress = book.state(&zz()).progress;
    assert_eq!(progress.len(), 3);
    assert!(progress.iter().all(|p| p.stage == Stage::Week));
    assert_eq!(book.state(&DeckId::words()), State::empty());
}

#[test]
fn a_refused_file_is_left_untouched_with_a_deck_id() {
    for (name, bytes) in [
        ("stamp4", &b"{\"version\": 4, \"decks\": {}}"[..]),
        ("garbage", &b"{ this is not a state"[..]),
    ] {
        let env = Env::new(&format!("deck-id-refused-{name}"));
        fs::write(env.state(), bytes).unwrap();
        for command in [
            &["review", "--deck-id", "zz"][..],
            &["stats", "--deck-id", "zz"],
            &["new", "2", "--deck-id", "zz"],
            &["advance", "2", "--deck-id", "zz"],
        ] {
            let run = env.run(TODAY, command, &yes(5));
            assert_eq!(run.code, 1, "{name} {command:?}");
            assert!(run.err.contains("left untouched"), "{}", run.err);
            assert_eq!(fs::read(env.state()).unwrap(), bytes, "{name} {command:?}");
        }
    }
}

#[test]
fn deck_id_and_the_flat_format_rule_pair() {
    let chosen = Env::new("pair-chosen");
    let run = chosen.run(TODAY, &["new", "1", "--deck-id", "zz"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        chosen.text().contains("\"version\": 3"),
        "{}",
        chosen.text()
    );
    let flat = Env::new("pair-flat");
    fs::write(flat.state(), State::empty().to_json()).unwrap();
    let run = flat.run(TODAY, &["new", "1"], "");
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(!flat.text().contains("\"decks\""), "{}", flat.text());
}

#[test]
fn an_empty_deck_id_exits_2_and_a_missing_value_names_the_option() {
    let env = Env::new("deck-id-bad");
    let run = env.run(TODAY, &["stats", "--deck-id", ""], "");
    assert_eq!(run.code, 2);
    assert!(run.err.contains("--deck-id"), "{}", run.err);
    let run = env.run(TODAY, &["stats", "--deck-id"], "");
    assert_eq!(run.code, 2);
    assert!(run.err.contains("--deck-id"), "{}", run.err);
    assert!(!env.state().exists());
}
