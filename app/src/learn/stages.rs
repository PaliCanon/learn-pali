//! The six stages as the screens name them: the name in the page's
//! language, and how many known terms sit in each.

use learn_core::deck::Deck;
use learn_core::progress::State;
use learn_core::stage::Stage;
use leptos_i18n::td_string;

use crate::catalogue::Strings;

/// How many known terms sit in each stage, Day to Year; a term `deck` does
/// not know (an orphan) counts for nothing.
pub(crate) fn stage_counts(state: &State, deck: &Deck) -> [usize; 6] {
    Stage::ALL.map(|stage| {
        state
            .progress
            .iter()
            .filter(|progress| {
                progress.stage == stage && state.term(deck, &progress.term).is_some()
            })
            .count()
    })
}

/// The stage's name in the page's language.
pub(crate) fn stage_label(t: Strings, stage: Stage) -> &'static str {
    match stage {
        Stage::Day => td_string!(t.locale(), stage_day),
        Stage::Week => td_string!(t.locale(), stage_week),
        Stage::Month => td_string!(t.locale(), stage_month),
        Stage::Quarter => td_string!(t.locale(), stage_quarter),
        Stage::HalfYear => td_string!(t.locale(), stage_half_year),
        Stage::Year => td_string!(t.locale(), stage_year),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::deck;
    use crate::test_support::progress_fixture;

    #[test]
    fn the_counts_are_per_stage_and_leave_the_orphans_out() {
        assert_eq!(
            stage_counts(&progress_fixture(), deck()),
            [2, 1, 0, 0, 0, 0]
        );
        assert_eq!(stage_counts(&State::empty(), deck()), [0; 6]);
    }

    #[test]
    fn a_started_added_word_counts_in_its_stage() {
        let state = crate::test_support::added_started(Stage::Month, crate::test_support::today());
        assert_eq!(stage_counts(&state, deck()), [0, 0, 1, 0, 0, 0]);
    }
}
