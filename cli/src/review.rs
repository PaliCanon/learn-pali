//! The review loop.
//!
//! Per question: the prompt of its facet, Enter, the answer, then `y` (right)
//! or `n` (wrong); anything else asks the `y`/`n` line again. The state is
//! written after every answer, so a closed terminal loses nothing: the
//! question that was not answered stays due.

use std::io::{BufRead, Write};

use learn_core::date::Date;
use learn_core::deck::{Deck, Term};
use learn_core::facet::Facet;
use learn_core::forms::forms_question;
use learn_core::progress::{Progress, State};
use learn_core::session::{Answer, Question, Session};

use crate::CliError;
use crate::store::StateSlot;

/// What a review did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Questions answered that count (practice questions apart).
    pub answered: usize,
    /// Of those, answered `y`.
    pub passed: usize,
    /// Of those, answered `n`.
    pub failed: usize,
    /// Practice questions answered (they change nothing in the state).
    pub practice: usize,
    /// False when the input ended before the session did.
    pub finished: bool,
}

/// Asks every question that is due and saves the state after each answer.
///
/// A warning is written to `out` first when the state holds terms the deck
/// does not know (a wrong `--deck`). The end of the input stops the review
/// (`Outcome::finished` false); the unanswered question stays due.
///
/// # Errors
/// [`CliError::Session`] when `today` is before the state's last day,
/// [`CliError::Store`] when the state cannot be written (nothing more is
/// asked or written), [`CliError::Io`] when the terminal fails,
/// [`CliError::Deck`] when the deck lacks what a question needs.
pub fn review<R: BufRead, W: Write>(
    deck: &Deck,
    state: State,
    today: Date,
    seed: u64,
    input: R,
    out: W,
    slot: StateSlot<'_>,
) -> Result<Outcome, CliError> {
    let mut input = input;
    let mut out = out;
    let mut session = Session::start(deck, state, today, seed)?;
    if session.changed_at_start() {
        slot.save(&session.snapshot())?;
    }
    warn_orphans(&mut out, session.stats().orphans)?;
    let mut outcome = Outcome::default();
    let mut shown = false;
    while let Some(question) = session.next().cloned() {
        shown = true;
        let state = session.snapshot();
        let term = state
            .term(deck, &question.term)
            .ok_or_else(|| CliError::missing_term(&question.term))?;
        let progress = state
            .get(&question.term)
            .ok_or_else(|| CliError::missing_term(&question.term))?;
        let Some(answer) = ask(&mut input, &mut out, term, progress, &question)? else {
            writeln!(
                out,
                "\nEnd of input: the question left unanswered stays due; \
                 progress is saved in {}.",
                slot.path.display()
            )?;
            return Ok(outcome);
        };
        session.answer(&question.term, question.facet, answer)?;
        slot.save(&session.snapshot())?;
        outcome.count(question.practice, answer);
    }
    outcome.finished = true;
    summarise(&mut out, &outcome, shown)?;
    Ok(outcome)
}

impl Outcome {
    /// Records one answer.
    fn count(&mut self, practice: bool, answer: Answer) {
        if practice {
            self.practice += 1;
            return;
        }
        self.answered += 1;
        match answer {
            Answer::Pass => self.passed += 1,
            Answer::Fail => self.failed += 1,
        }
    }
}

/// The end of a finished session.
fn summarise<W: Write>(out: &mut W, outcome: &Outcome, shown: bool) -> Result<(), CliError> {
    if !shown {
        writeln!(out, "Nothing is due.")?;
        return Ok(());
    }
    writeln!(
        out,
        "\nSession over: {} answered, {} passed, {} failed, {} practice.",
        outcome.answered, outcome.passed, outcome.failed, outcome.practice
    )?;
    Ok(())
}

/// A wrong `--deck` shows as terms of the state the deck does not know.
fn warn_orphans<W: Write>(out: &mut W, orphans: usize) -> Result<(), CliError> {
    match orphans {
        0 => {}
        1 => writeln!(
            out,
            "Warning: 1 term of the state is not in the deck — a wrong --deck?"
        )?,
        n => writeln!(
            out,
            "Warning: {n} terms of the state are not in the deck — a wrong --deck?"
        )?,
    }
    Ok(())
}

/// One question: prompt, Enter, answer, `y`/`n`. `None` when the input ends.
fn ask<R: BufRead, W: Write>(
    input: &mut R,
    out: &mut W,
    term: &Term,
    progress: &Progress,
    question: &Question,
) -> Result<Option<Answer>, CliError> {
    let tag = if question.practice { "(practice) " } else { "" };
    writeln!(out)?;
    for line in prompt_lines(term, progress, question.facet)? {
        writeln!(out, "{tag}{line}")?;
    }
    write!(out, "[Enter to see the answer] ")?;
    out.flush()?;
    if read_line(input)?.is_none() {
        return Ok(None);
    }
    for line in answer_lines(term, progress, question.facet)? {
        writeln!(out, "  {line}")?;
    }
    loop {
        write!(out, "Did you know it? [y/n] ")?;
        out.flush()?;
        match read_line(input)?.as_deref().map(str::trim) {
            None => return Ok(None),
            Some("y" | "Y") => return Ok(Some(Answer::Pass)),
            Some("n" | "N") => return Ok(Some(Answer::Fail)),
            Some(_) => writeln!(out, "Please answer y or n.")?,
        }
    }
}

/// The next line of the input without its end of line; `None` at the end.
fn read_line<R: BufRead>(input: &mut R) -> Result<Option<String>, CliError> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim_end_matches(['\n', '\r']).to_owned()))
}

/// The meaning with its literal sense in brackets when there is one.
fn meaning_line(term: &Term) -> String {
    match &term.meaning.literal {
        Some(literal) => format!("{} [lit. {literal}]", term.meaning.en),
        None => term.meaning.en.clone(),
    }
}

/// What is shown before the answer, by facet.
fn prompt_lines(term: &Term, progress: &Progress, facet: Facet) -> Result<Vec<String>, CliError> {
    Ok(match facet {
        Facet::Meaning => vec![format!("{} ({})", term.lemma, term.pos.0)],
        Facet::Recall => vec![meaning_line(term)],
        Facet::Forms => {
            let form = forms_question(term, progress).ok_or_else(|| no(term, "forms"))?;
            vec![format!("{} — {}?", term.lemma, form.label)]
        }
        Facet::Context => {
            let example = term.example.as_ref().ok_or_else(|| no(term, "example"))?;
            let blank = example.blank.as_ref().ok_or_else(|| no(term, "blank"))?;
            vec![
                format!("{}___{}", blank.before, blank.after),
                format!("({} — {})", example.source, example.sutta),
            ]
        }
    })
}

/// What is shown once the learner asked for the answer, by facet.
fn answer_lines(term: &Term, progress: &Progress, facet: Facet) -> Result<Vec<String>, CliError> {
    Ok(match facet {
        Facet::Meaning => {
            let mut lines = vec![meaning_line(term)];
            lines.extend(
                term.construction
                    .iter()
                    .map(|c| format!("construction: {c}")),
            );
            lines.extend(
                term.root
                    .iter()
                    .map(|root| format!("root: {} ({})", root.key, root.meaning)),
            );
            lines
        }
        Facet::Recall => vec![term.lemma.clone()],
        Facet::Forms => {
            let form = forms_question(term, progress).ok_or_else(|| no(term, "forms"))?;
            vec![form.values.join(", ")]
        }
        Facet::Context => {
            let blank = term
                .example
                .as_ref()
                .and_then(|example| example.blank.as_ref())
                .ok_or_else(|| no(term, "blank"))?;
            vec![blank.answer.clone()]
        }
    })
}

fn no(term: &Term, what: &str) -> CliError {
    CliError::Deck(format!("term {} has no {what} to ask", term.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{StoreError, load_state, save_state};
    use learn_core::book::DeckId;
    use learn_core::deck::TermId;
    use learn_core::facet::Facet;
    use learn_core::progress::Progress;
    use learn_core::session::Session;
    use learn_core::stage::Stage;
    use std::fs;
    use std::path::{Path, PathBuf};

    const DECK: &str = r#"{
      "version": 1,
      "source": "test",
      "terms": [
        {"id": "alpha", "lemma": "alpha", "pos": "noun",
         "meaning": {"en": "first letter", "literal": "ox"},
         "construction": "al + pha", "root": {"key": "√al", "meaning": "to ox"},
         "forms": [], "rank": 1},
        {"id": "beta", "lemma": "beta", "pos": "verb",
         "meaning": {"en": "second letter"},
         "example": {"text": "one beta two", "source": "AN1", "sutta": "suttam",
                     "blank": {"before": "one ", "answer": "beta", "after": " two"}},
         "forms": [{"label": "nom pl", "values": ["betas", "betai"], "rare": false}],
         "rank": 2}
      ]
    }"#;

    const TODAY: &str = "2026-10-05";

    fn deck() -> Deck {
        Deck::from_json(DECK).unwrap()
    }

    fn day(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    /// A fresh directory per test, under the system's temporary folder.
    fn dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir()
            .join(format!("learn-cli-review-{}", std::process::id()))
            .join(name);
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn started(terms: &[&str]) -> State {
        let mut state = State::empty();
        for term in terms {
            state
                .progress
                .push(Progress::new(TermId((*term).to_owned()), day(TODAY)));
        }
        state
    }

    /// `answers` are the `y`/`n` lines; each is preceded by the Enter line.
    fn typed(answers: &[&str]) -> String {
        answers.iter().flat_map(|a| ["\n", a, "\n"]).collect()
    }

    /// A seed whose shuffle leaves the questions of `state` in the deck's
    /// order (by term, then facet; the test deck's ids sort as its ranks):
    /// the tests below read the questions in a known sequence.
    fn deck_order_seed(state: &State) -> u64 {
        let deck = deck();
        (0..10_000)
            .find(|seed| {
                let Ok(mut session) = Session::start(&deck, state.clone(), day(TODAY), *seed)
                else {
                    return true; // a refused start asks nothing, any seed will do
                };
                let mut asked = Vec::new();
                while let Some(question) = session.next().cloned() {
                    asked.push((question.term.0.clone(), question.facet));
                    let (term, facet) = (question.term, question.facet);
                    session.answer(&term, facet, Answer::Pass).unwrap();
                }
                asked.windows(2).all(|pair| pair[0] <= pair[1])
            })
            .expect("a seed that keeps the deck's order")
    }

    fn run_review(state: State, input: &str, path: &Path) -> (Result<Outcome, CliError>, String) {
        run_review_in(state, input, path, &DeckId::words())
    }

    fn run_review_in(
        state: State,
        input: &str,
        path: &Path,
        id: &DeckId,
    ) -> (Result<Outcome, CliError>, String) {
        let mut out = Vec::new();
        let seed = deck_order_seed(&state);
        let result = review(
            &deck(),
            state,
            day(TODAY),
            seed,
            input.as_bytes(),
            &mut out,
            StateSlot { path, id },
        );
        (result, String::from_utf8(out).unwrap())
    }

    /// What a review prints with `seed`, every answer `y`.
    fn printed(seed: u64, name: &str) -> String {
        let path = dir(name).join("state.json");
        let mut out = Vec::new();
        let input = typed(&["y"; 6]);
        let state = started(&["alpha", "beta"]);
        review(
            &deck(),
            state,
            day(TODAY),
            seed,
            input.as_bytes(),
            &mut out,
            StateSlot {
                path: &path,
                id: &DeckId::words(),
            },
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn an_added_word_of_the_state_is_reviewed_like_a_deck_term() {
        let path = dir("added").join("state.json");
        let mut state = started(&["gamma"]);
        state.added.push(learn_core::deck::Term {
            id: TermId("gamma".to_owned()),
            lemma: "gamma".to_owned(),
            pos: learn_core::deck::Pos("ind".to_owned()),
            meaning: learn_core::deck::Meaning {
                en: "an added sense".to_owned(),
                fr: None,
                vi: None,
                literal: None,
            },
            construction: None,
            root: None,
            example: None,
            forms: Vec::new(),
            rank: u32::MAX,
        });
        let (result, out) = run_review(state, &typed(&["y"; 2]), &path);
        let outcome = result.unwrap();
        assert_eq!((outcome.answered, outcome.passed), (2, 2), "{out}");
        assert!(out.contains("an added sense"), "{out}");
    }

    #[test]
    fn the_seed_sets_the_order_the_same_seed_repeats_it() {
        assert_eq!(printed(3, "seed-a"), printed(3, "seed-b"));
        let base = printed(0, "seed-c");
        assert!(
            (1..20).any(|seed| printed(seed, "seed-d") != base),
            "some seed asks in another order"
        );
    }

    #[test]
    fn answering_yes_to_everything_moves_both_terms_to_week() {
        let path = dir("pass").join("state.json");
        let input = typed(&["y"; 6]);
        let (result, out) = run_review(started(&["alpha", "beta"]), &input, &path);
        let outcome = result.unwrap();
        assert_eq!(
            outcome,
            Outcome {
                answered: 6,
                passed: 6,
                failed: 0,
                practice: 0,
                finished: true
            }
        );
        let saved = load_state(&path, &DeckId::words()).unwrap();
        assert!(saved.progress.iter().all(|p| p.stage == Stage::Week));
        assert_eq!(saved.last_today, Some(day(TODAY)));
        assert!(
            out.contains("6 answered, 6 passed, 0 failed, 0 practice"),
            "{out}"
        );
    }

    #[test]
    fn each_facet_has_its_prompt_and_its_answer() {
        let path = dir("prompts").join("state.json");
        let input = typed(&["y"; 6]);
        let (_, out) = run_review(started(&["alpha", "beta"]), &input, &path);
        // Meaning: lemma (pos); then en, literal, construction, root.
        assert!(out.contains("alpha (noun)"), "{out}");
        assert!(out.contains("first letter"), "{out}");
        assert!(out.contains("ox"), "{out}");
        assert!(out.contains("al + pha"), "{out}");
        assert!(out.contains("√al"), "{out}");
        // Recall: the meaning with the literal in brackets; the answer is the lemma.
        assert!(out.contains("first letter [lit. ox]"), "{out}");
        // Forms: lemma — label?; the answer lists every value.
        assert!(out.contains("beta — nom pl?"), "{out}");
        assert!(out.contains("betas, betai"), "{out}");
        // Context: the blank and the source; the answer is the word.
        assert!(out.contains("one ___ two"), "{out}");
        assert!(out.contains("AN1 — suttam"), "{out}");
    }

    #[test]
    fn a_wrong_answer_sends_the_term_to_day_and_asks_the_facet_again_as_practice() {
        let path = dir("fail").join("state.json");
        // alpha Meaning n (alpha Recall is dropped); beta 4 y; practice y.
        let input = typed(&["n", "y", "y", "y", "y", "y"]);
        let (result, out) = run_review(started(&["alpha", "beta"]), &input, &path);
        let outcome = result.unwrap();
        assert_eq!(
            outcome,
            Outcome {
                answered: 5,
                passed: 4,
                failed: 1,
                practice: 1,
                finished: true
            }
        );
        assert_eq!(out.matches("(practice)").count(), 1, "{out}");
        assert!(
            out.contains("5 answered, 4 passed, 1 failed, 1 practice"),
            "{out}"
        );
        let saved = load_state(&path, &DeckId::words()).unwrap();
        let alpha = saved.get(&TermId("alpha".to_owned())).unwrap();
        assert_eq!((alpha.stage, alpha.lapses), (Stage::Day, 1));
        assert_eq!(alpha.due, day(TODAY) + 1);
        let beta = saved.get(&TermId("beta".to_owned())).unwrap();
        assert_eq!(beta.stage, Stage::Week);
    }

    #[test]
    fn the_state_is_written_after_every_answer() {
        let path = dir("every").join("state.json");
        // One answer, then the input ends at the next Enter line.
        let input = format!("{}\n", typed(&["y"]));
        let (result, _) = run_review(started(&["alpha", "beta"]), &input, &path);
        assert!(!result.unwrap().finished);
        let saved = load_state(&path, &DeckId::words()).unwrap();
        let alpha = saved.get(&TermId("alpha".to_owned())).unwrap();
        assert!(alpha.passed.contains(Facet::Meaning), "{alpha:?}");
    }

    #[test]
    fn the_end_of_input_at_the_enter_line_stops_and_leaves_the_question_due() {
        let path = dir("eof-enter").join("state.json");
        let input = typed(&["y"]);
        let (result, out) = run_review(started(&["alpha", "beta"]), &input, &path);
        let outcome = result.unwrap();
        assert_eq!((outcome.answered, outcome.finished), (1, false));
        assert!(out.contains("End of input"), "{out}");
        assert!(
            !out.contains("answered,"),
            "no summary of a finished session: {out}"
        );
        // The next session starts with the question that was left.
        let saved = load_state(&path, &DeckId::words()).unwrap();
        let deck = deck();
        let seed = deck_order_seed(&saved);
        let mut session = Session::start(&deck, saved, day(TODAY), seed).unwrap();
        let next = session.next().unwrap();
        assert_eq!((next.term.0.as_str(), next.facet), ("alpha", Facet::Recall));
    }

    #[test]
    fn the_end_of_input_at_the_yes_no_line_changes_nothing_for_that_question() {
        let path = dir("eof-yn").join("state.json");
        let input = format!("{}\n", typed(&["y"])); // the 2nd Enter, then EOF
        let (result, out) = run_review(started(&["alpha"]), &input, &path);
        let outcome = result.unwrap();
        assert_eq!((outcome.answered, outcome.finished), (1, false));
        assert!(out.contains("End of input"), "{out}");
        let deck = deck();
        let saved = load_state(&path, &DeckId::words()).unwrap();
        let seed = deck_order_seed(&saved);
        let mut session = Session::start(&deck, saved, day(TODAY), seed).unwrap();
        assert_eq!(session.next().unwrap().facet, Facet::Recall);
    }

    #[test]
    fn anything_but_y_or_n_asks_the_line_again() {
        let path = dir("reask").join("state.json");
        let input = "\nmaybe\n\ny\n";
        let (result, out) = run_review(started(&["alpha"]), input, &path);
        let outcome = result.unwrap();
        assert_eq!(
            (outcome.answered, outcome.passed, outcome.finished),
            (1, 1, false)
        );
        assert!(out.contains("Please answer y or n"), "{out}");
    }

    #[test]
    fn a_write_error_is_fatal_and_nothing_more_is_asked() {
        let path = dir("fatal").join("nowhere").join("state.json");
        let input = typed(&["y"; 6]);
        let (result, out) = run_review(started(&["alpha", "beta"]), &input, &path);
        assert!(
            matches!(result, Err(CliError::Store(StoreError::Io { .. }))),
            "{result:?}"
        );
        assert_eq!(out.matches("alpha (noun)").count(), 1, "{out}");
        assert!(!out.contains("beta"), "{out}");
        assert!(!path.exists());
    }

    #[test]
    fn terms_unknown_to_the_deck_are_warned_about_and_never_asked() {
        let path = dir("orphan").join("state.json");
        let (result, out) = run_review(started(&["alpha", "ghost", "spirit"]), "", &path);
        assert!(result.is_ok());
        assert!(
            out.contains("2 terms of the state are not in the deck"),
            "{out}"
        );
        assert!(out.contains("wrong --deck"), "{out}");
        assert!(
            !out.contains("ghost (") && !out.contains("spirit ("),
            "{out}"
        );
    }

    #[test]
    fn one_orphan_reads_in_the_singular() {
        let path = dir("orphan1").join("state.json");
        let (_, out) = run_review(started(&["ghost"]), "", &path);
        assert!(
            out.contains("1 term of the state is not in the deck"),
            "{out}"
        );
    }

    #[test]
    fn nothing_due_says_so_and_writes_nothing() {
        let path = dir("none").join("state.json");
        let (result, out) = run_review(State::empty(), "", &path);
        let outcome = result.unwrap();
        assert_eq!(
            outcome,
            Outcome {
                finished: true,
                ..Outcome::default()
            }
        );
        assert!(out.contains("Nothing is due"), "{out}");
        assert!(!path.exists());
    }

    #[test]
    fn a_day_before_the_last_one_is_refused() {
        let path = dir("early").join("state.json");
        let mut state = started(&["alpha"]);
        state.last_today = Some(day("2026-10-09"));
        let (result, out) = run_review(state, "", &path);
        let error = result.unwrap_err();
        assert!(matches!(error, CliError::Session(_)), "{error}");
        assert!(error.to_string().contains("2026-10-05"), "{error}");
        assert!(error.to_string().contains("2026-10-09"), "{error}");
        assert_eq!(out, "");
    }

    #[test]
    fn a_saved_state_can_be_resumed_by_a_second_review() {
        let path = dir("resume").join("state.json");
        let first = format!("{}\n", typed(&["y", "y"]));
        let (result, _) = run_review(started(&["alpha", "beta"]), &first, &path);
        assert!(!result.unwrap().finished);
        let state = load_state(&path, &DeckId::words()).unwrap();
        let rest = typed(&["y"; 4]);
        let (result, _) = run_review(state, &rest, &path);
        assert!(result.unwrap().finished);
        let saved = load_state(&path, &DeckId::words()).unwrap();
        assert!(saved.progress.iter().all(|p| p.stage == Stage::Week));
        save_state(&path, &DeckId::words(), &saved).unwrap();
    }

    #[test]
    fn the_state_is_written_into_the_chosen_entry_after_each_answer() {
        let path = dir("entry").join("state.json");
        let zz = DeckId("zz".to_owned());
        let input = format!("{}\n", typed(&["y"]));
        let (result, _) = run_review_in(started(&["alpha", "beta"]), &input, &path, &zz);
        assert!(!result.unwrap().finished);
        let saved = load_state(&path, &zz).unwrap();
        let alpha = saved.get(&TermId("alpha".to_owned())).unwrap();
        assert!(alpha.passed.contains(Facet::Meaning), "{alpha:?}");
        assert_eq!(
            load_state(&path, &DeckId::words()).unwrap(),
            State::empty(),
            "the words entry is untouched"
        );
    }
}
