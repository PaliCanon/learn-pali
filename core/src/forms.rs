//! The Forms question: which labelled form is asked.
//!
//! The question is a pure function of the term and its progress: the term's
//! non-rare forms (the rare, greyed ones are never asked) in deck order, and
//! the one at index `(stage index + lapses) mod count`. The same state always
//! gives the same label; a pass (a higher stage) or a lapse moves to another
//! one. The answer to show is all the values of that label
//! ([`Form::values`]). A term without a non-rare form has no Forms facet
//! ([`crate::facet::facets_of`]), so it is never asked and this returns
//! `None`.

use crate::deck::{Form, Term};
use crate::progress::Progress;

/// The form to ask for this term in this state: the non-rare form at index
/// `(stage.index() + lapses) mod count`, or `None` when the term has no
/// non-rare form.
pub fn forms_question<'t>(term: &'t Term, progress: &Progress) -> Option<&'t Form> {
    let regular: Vec<&Form> = term.forms.iter().filter(|form| !form.rare).collect();
    let count = u64::try_from(regular.len())
        .ok()
        .filter(|count| *count > 0)?;
    let at = (u64::from(progress.stage.index()) + u64::from(progress.lapses)) % count;
    regular.get(usize::try_from(at).ok()?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;
    use crate::deck::samples::{deck, form, indeclinable, noun, rare_form};
    use crate::facet::{Facet, FacetSet};
    use crate::progress::State;
    use crate::session::Session;
    use crate::stage::Stage;

    fn declined() -> Term {
        Term {
            forms: vec![
                form("a", "a1"),
                rare_form("r", "r1"),
                Form {
                    label: "b".to_string(),
                    values: vec!["b1".to_string(), "b2".to_string()],
                    rare: false,
                },
                form("c", "c1"),
            ],
            ..noun("sati 1.1", 1)
        }
    }

    fn state_at(stage: Stage, lapses: u32) -> Progress {
        Progress {
            term: declined().id,
            stage,
            passed: FacetSet::empty(),
            due: Date::from_days(0),
            lapses,
        }
    }

    fn label(stage: Stage, lapses: u32) -> String {
        forms_question(&declined(), &state_at(stage, lapses))
            .unwrap()
            .label
            .clone()
    }

    #[test]
    fn the_same_state_gives_the_same_label() {
        for stage in Stage::ALL {
            assert_eq!(label(stage, 2), label(stage, 2));
        }
    }

    #[test]
    fn the_stage_index_and_the_lapses_move_the_label() {
        assert_eq!(label(Stage::Day, 0), "a");
        assert_eq!(label(Stage::Week, 0), "b");
        assert_eq!(label(Stage::Month, 0), "c");
        assert_eq!(label(Stage::Quarter, 0), "a", "wraps after three labels");
        assert_eq!(label(Stage::Day, 1), "b");
        assert_eq!(label(Stage::Day, 2), "c");
        assert_eq!(
            label(Stage::Week, 1),
            label(Stage::Month, 0),
            "the sum counts"
        );
        assert_eq!(label(Stage::Year, u32::MAX), label(Stage::Year, u32::MAX));
    }

    #[test]
    fn only_non_rare_labels_are_ever_asked() {
        for stage in Stage::ALL {
            for lapses in 0..20 {
                assert_ne!(label(stage, lapses), "r");
            }
        }
    }

    #[test]
    fn the_answer_is_every_value_of_the_label() {
        let term = declined();
        let asked = forms_question(&term, &state_at(Stage::Week, 0)).unwrap();
        assert_eq!(asked.label, "b");
        assert_eq!(asked.values, vec!["b1", "b2"]);
    }

    #[test]
    fn no_non_rare_form_means_no_question() {
        let progress = state_at(Stage::Day, 0);
        assert_eq!(forms_question(&indeclinable("kho", 1), &progress), None);
        let rare_only = Term {
            forms: vec![rare_form("r", "r1")],
            ..indeclinable("kho", 1)
        };
        assert_eq!(forms_question(&rare_only, &progress), None);
        assert!(!crate::facet::facets_of(&rare_only).contains(Facet::Forms));
    }

    #[test]
    fn a_term_that_gains_forms_is_asked_at_its_next_due_not_before() {
        // Started without forms (Meaning and Recall passed), due tomorrow;
        // the deck now gives it forms.
        let term = Term {
            example: None, // no blank: Forms is the only facet gained
            ..noun("sati 1.1", 1)
        };
        let deck = deck(vec![term.clone()]);
        let tomorrow = Date::from_days(20732);
        let progress = Progress {
            term: term.id,
            stage: Stage::Week,
            passed: [Facet::Meaning, Facet::Recall].into_iter().collect(),
            due: tomorrow,
            lapses: 0,
        };
        let state = State {
            version: State::VERSION,
            last_today: None,
            progress: vec![progress],
        };
        let mut today = Session::start(&deck, state.clone(), Date::from_days(20731)).unwrap();
        assert_eq!(today.next(), None, "not asked today");
        assert_eq!(today.stats().due_today, 0, "not pulled forward");
        let mut due = Session::start(&deck, state, tomorrow).unwrap();
        let queued: Vec<Facet> = due.queue.iter().map(|q| q.facet).collect();
        assert_eq!(queued, [Facet::Forms], "exactly the gained facet");
        let first = due.next().unwrap();
        assert_eq!(first.facet, Facet::Forms, "asked at its next due");
    }

    #[test]
    fn a_lapse_moves_the_label_by_one_over_the_non_rare_labels() {
        // Three non-rare labels (a, b, c) and a rare one in between.
        let at = |lapses| label(Stage::Day, lapses);
        assert_eq!(
            (at(0), at(1), at(2), at(3)),
            ("a".into(), "b".into(), "c".into(), "a".into())
        );
        let rare_between = declined();
        let asked: Vec<&str> = (0..3)
            .map(|lapses| {
                forms_question(&rare_between, &state_at(Stage::Day, lapses))
                    .unwrap()
                    .label
                    .as_str()
            })
            .collect();
        assert_eq!(
            asked,
            ["a", "b", "c"],
            "the rare label is skipped, not counted"
        );
    }
}
