//! The keys of a review: the card's presentation key (a new one per
//! question shown, so the card resets) and the grade keys' rule.

use learn_core::deck::TermId;
use learn_core::facet::Facet;
use learn_core::session::Answer;

/// The key of one presentation of a card: unique across terms, facets and
/// the session's counter (a practice question of the same term and facet
/// gets a new one). `term|Facet|counter`: the facet and the counter hold no
/// `|`, so two different inputs never give the same key, whatever the term
/// id holds.
pub(crate) fn presentation_key(term: &TermId, facet: Facet, counter: u32) -> String {
    format!("{term}|{facet:?}|{counter}")
}

/// What decides whether a key press grades the card.
#[allow(
    clippy::struct_excessive_bools,
    reason = "six independent conditions of the design's key rule, each tested in pairs"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct GradeGate {
    /// The card shows its answer.
    pub(crate) revealed: bool,
    /// A Dialog is open (the stop Dialog).
    pub(crate) dialog_open: bool,
    /// The session is paused (the state changed in another tab).
    pub(crate) paused: bool,
    /// The event's target is editable (an input, a textarea,
    /// `contenteditable`).
    pub(crate) editable_target: bool,
    /// Ctrl, Alt or Meta is held.
    pub(crate) modifier: bool,
    /// The key is auto-repeating.
    pub(crate) repeat: bool,
}

/// The answer the key `key` (a `KeyboardEvent.key`) gives under `gate`:
/// `y` / `Y` / `ArrowRight` = Knew it ([`Answer::Pass`]), `n` / `N` /
/// `ArrowLeft` = Didn't know ([`Answer::Fail`]), only when revealed, no
/// Dialog open, not paused, the target not editable, no modifier and no
/// repeat (A8); anything else `None`. Space and Enter are the card's.
pub(crate) fn grade_key(key: &str, gate: GradeGate) -> Option<Answer> {
    let blocked = !gate.revealed
        || gate.dialog_open
        || gate.paused
        || gate.editable_target
        || gate.modifier
        || gate.repeat;
    if blocked {
        return None;
    }
    match key {
        "y" | "Y" | "ArrowRight" => Some(Answer::Pass),
        "n" | "N" | "ArrowLeft" => Some(Answer::Fail),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn id(text: &str) -> TermId {
        TermId(text.to_string())
    }

    /// A gate that lets a key through: revealed, nothing else in the way.
    fn open() -> GradeGate {
        GradeGate {
            revealed: true,
            ..GradeGate::default()
        }
    }

    #[test]
    fn the_presentation_key_is_unique_across_terms_facets_and_counters() {
        let mut seen = HashSet::new();
        let terms = ["kho", "ca 2.1", "ca 2", "ca", "2.1", "a|b", "a", "b"];
        for term in terms {
            for facet in Facet::ALL {
                for counter in [0, 1, 2, 10, 11, 100] {
                    let key = presentation_key(&id(term), facet, counter);
                    assert!(seen.insert(key.clone()), "{key} twice");
                }
            }
        }
        assert_eq!(seen.len(), terms.len() * 4 * 6);
    }

    #[test]
    fn a_term_that_looks_like_a_key_does_not_collide_with_another() {
        let tricky = presentation_key(&id("a|Meaning|1"), Facet::Recall, 2);
        let plain = presentation_key(&id("a"), Facet::Meaning, 1);
        assert_ne!(tricky, plain);
        assert_ne!(
            presentation_key(&id("a"), Facet::Meaning, 12),
            presentation_key(&id("a"), Facet::Meaning, 1),
        );
    }

    #[test]
    fn the_presentation_key_is_stable_for_equal_inputs() {
        let one = presentation_key(&id("kho"), Facet::Forms, 3);
        let two = presentation_key(&id("kho"), Facet::Forms, 3);
        assert_eq!(one, two);
        assert_ne!(one, "");
    }

    #[test]
    fn the_same_term_and_facet_asked_again_gets_a_new_key() {
        assert_ne!(
            presentation_key(&id("kho"), Facet::Meaning, 4),
            presentation_key(&id("kho"), Facet::Meaning, 5)
        );
    }

    #[test]
    fn y_and_right_arrow_are_knew_it() {
        for key in ["y", "Y", "ArrowRight"] {
            assert_eq!(grade_key(key, open()), Some(Answer::Pass), "{key}");
        }
    }

    #[test]
    fn n_and_left_arrow_are_did_not_know() {
        for key in ["n", "N", "ArrowLeft"] {
            assert_eq!(grade_key(key, open()), Some(Answer::Fail), "{key}");
        }
    }

    #[test]
    fn any_other_key_grades_nothing() {
        for key in [
            " ",
            "Enter",
            "Escape",
            "Tab",
            "a",
            "1",
            "ArrowUp",
            "ArrowDown",
            "yy",
            "",
            "Shift",
        ] {
            assert_eq!(grade_key(key, open()), None, "{key:?}");
        }
    }

    #[test]
    fn nothing_grades_before_the_reveal() {
        let gate = GradeGate::default();
        for key in ["y", "n", "ArrowRight", "ArrowLeft"] {
            assert_eq!(grade_key(key, gate), None, "{key}");
        }
    }

    #[test]
    fn nothing_grades_while_a_dialog_is_open() {
        let gate = GradeGate {
            dialog_open: true,
            ..open()
        };
        assert_eq!(grade_key("y", gate), None);
        assert_eq!(grade_key("ArrowLeft", gate), None);
    }

    #[test]
    fn nothing_grades_while_paused() {
        let gate = GradeGate {
            paused: true,
            ..open()
        };
        assert_eq!(grade_key("n", gate), None);
        assert_eq!(grade_key("ArrowRight", gate), None);
    }

    #[test]
    fn nothing_grades_from_an_editable_target() {
        let gate = GradeGate {
            editable_target: true,
            ..open()
        };
        assert_eq!(grade_key("y", gate), None);
        assert_eq!(grade_key("n", gate), None);
    }

    #[test]
    fn nothing_grades_with_a_modifier_held() {
        let gate = GradeGate {
            modifier: true,
            ..open()
        };
        assert_eq!(grade_key("y", gate), None);
        assert_eq!(grade_key("ArrowLeft", gate), None);
    }

    #[test]
    fn nothing_grades_on_a_key_repeat() {
        let gate = GradeGate {
            repeat: true,
            ..open()
        };
        assert_eq!(grade_key("n", gate), None);
        assert_eq!(grade_key("ArrowRight", gate), None);
    }

    #[test]
    fn every_rule_blocks_alone_and_all_together() {
        let blockers: [fn(&mut GradeGate); 5] = [
            |gate| gate.dialog_open = true,
            |gate| gate.paused = true,
            |gate| gate.editable_target = true,
            |gate| gate.modifier = true,
            |gate| gate.repeat = true,
        ];
        let mut all = open();
        for block in blockers {
            let mut gate = open();
            block(&mut gate);
            assert_eq!(grade_key("y", gate), None);
            block(&mut all);
        }
        assert_eq!(grade_key("y", all), None);
        assert_eq!(grade_key("y", open()), Some(Answer::Pass));
    }
}
