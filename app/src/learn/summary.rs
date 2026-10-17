//! What a finished session changed ([`summary`]), counted in terms from
//! the state before and after it (A2): the end of a session shows no score.

use learn_core::date::Date;
use learn_core::deck::Deck;
use learn_core::progress::State;
use learn_core::stage::Stage;

/// The summary's three facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Summary {
    /// Terms whose stage rose.
    pub(crate) moved_up: usize,
    /// Terms whose lapses rose: back to Day, they start again tomorrow.
    pub(crate) back_to_day: usize,
    /// The earliest due date after the session among the terms the deck
    /// knows (orphans skipped); `None` when there is none.
    pub(crate) next_due: Option<Date>,
}

/// The summary of a session that turned `before` into `after`, over the
/// terms the deck knows (an orphan changes nothing and is skipped). A term
/// in `after` only was started during the session: it is compared with a
/// new term (stage Day, no lapse). `next_due` is the earliest `due` strictly
/// after `after.last_today` (the session's day); every known `due` is a
/// candidate when the state records no day.
pub(crate) fn summary(before: &State, after: &State, deck: &Deck) -> Summary {
    let mut summary = Summary::default();
    for now in &after.progress {
        if after.term(deck, &now.term).is_none() {
            continue;
        }
        let (stage, lapses) = before
            .get(&now.term)
            .map_or((Stage::Day, 0), |then| (then.stage, then.lapses));
        summary.moved_up += usize::from(now.stage > stage);
        summary.back_to_day += usize::from(now.lapses > lapses);
        let after_today = after.last_today.is_none_or(|today| now.due > today);
        if after_today && summary.next_due.is_none_or(|earliest| now.due < earliest) {
            summary.next_due = Some(now.due);
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use learn_core::deck::TermId;
    use learn_core::facet::FacetSet;
    use learn_core::progress::Progress;
    use learn_core::stage::Stage;

    use super::*;

    fn today() -> Date {
        Date::parse("2026-10-05").unwrap()
    }

    fn deck() -> &'static Deck {
        crate::deck::deck()
    }

    fn entry(term: &str, stage: Stage, due: Date, lapses: u32) -> Progress {
        Progress {
            term: TermId(term.to_string()),
            stage,
            passed: FacetSet::empty(),
            due,
            lapses,
        }
    }

    fn state_of(last_today: Option<Date>, entries: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            added: Vec::new(),
            last_today,
            progress: entries,
        }
    }

    fn started() -> State {
        state_of(
            Some(today()),
            vec![
                entry("kho", Stage::Day, today(), 0),
                entry("hoti 1", Stage::Week, today(), 0),
                entry("evaṃ 1", Stage::Month, today(), 2),
            ],
        )
    }

    #[test]
    fn a_session_that_changed_nothing_counts_nothing() {
        let before = started();
        let summary = summary(&before, &before, deck());
        assert_eq!(summary.moved_up, 0);
        assert_eq!(summary.back_to_day, 0);
    }

    #[test]
    fn a_term_whose_stage_rose_moved_up() {
        let before = started();
        let mut after = before.clone();
        after.progress[0].stage = Stage::Week;
        after.progress[0].due = today() + 7;
        after.progress[1].stage = Stage::Month;
        after.progress[1].due = today() + 30;
        let summary = summary(&before, &after, deck());
        assert_eq!(summary.moved_up, 2);
        assert_eq!(summary.back_to_day, 0);
    }

    #[test]
    fn a_term_whose_lapses_rose_is_back_to_day() {
        let before = started();
        let mut after = before.clone();
        after.progress[1].stage = Stage::Day;
        after.progress[1].lapses = 1;
        after.progress[1].due = today() + 1;
        let summary = summary(&before, &after, deck());
        assert_eq!(summary.moved_up, 0);
        assert_eq!(summary.back_to_day, 1);
    }

    #[test]
    fn a_day_term_failing_counts_though_its_stage_is_the_same() {
        let before = started();
        let mut after = before.clone();
        after.progress[0].lapses = 1;
        after.progress[0].due = today() + 1;
        let summary = summary(&before, &after, deck());
        assert_eq!(summary.moved_up, 0);
        assert_eq!(summary.back_to_day, 1);
    }

    #[test]
    fn a_session_can_move_some_up_and_send_others_back() {
        let before = started();
        let mut after = before.clone();
        after.progress[0].stage = Stage::Week;
        after.progress[0].due = today() + 7;
        after.progress[2].stage = Stage::Day;
        after.progress[2].lapses = 3;
        after.progress[2].due = today() + 1;
        let summary = summary(&before, &after, deck());
        assert_eq!(summary.moved_up, 1);
        assert_eq!(summary.back_to_day, 1);
    }

    #[test]
    fn a_term_started_in_the_session_is_counted_from_day() {
        let before = state_of(Some(today()), vec![]);
        let after = state_of(
            Some(today()),
            vec![
                entry("kho", Stage::Week, today() + 7, 0),
                entry("hoti 1", Stage::Day, today() + 1, 1),
                entry("evaṃ 1", Stage::Day, today(), 0),
            ],
        );
        let summary = summary(&before, &after, deck());
        assert_eq!(summary.moved_up, 1);
        assert_eq!(summary.back_to_day, 1);
    }

    #[test]
    fn a_term_the_deck_does_not_know_is_not_counted() {
        let before = state_of(
            Some(today()),
            vec![entry("not-in-the-deck", Stage::Day, today(), 0)],
        );
        let after = state_of(
            Some(today()),
            vec![entry("not-in-the-deck", Stage::Year, today() + 365, 4)],
        );
        let summary = summary(&before, &after, deck());
        assert_eq!(summary.moved_up, 0);
        assert_eq!(summary.back_to_day, 0);
        assert_eq!(summary.next_due, None);
    }

    #[test]
    fn the_next_due_is_the_earliest_date_after_today() {
        let after = state_of(
            Some(today()),
            vec![
                entry("kho", Stage::Week, today() + 7, 0),
                entry("hoti 1", Stage::Day, today() + 1, 1),
                entry("evaṃ 1", Stage::Month, today() + 30, 0),
            ],
        );
        let summary = summary(&started(), &after, deck());
        assert_eq!(summary.next_due, Some(today() + 1));
    }

    #[test]
    fn the_next_due_skips_orphans() {
        let after = state_of(
            Some(today()),
            vec![
                entry("not-in-the-deck", Stage::Day, today() + 1, 0),
                entry("kho", Stage::Week, today() + 7, 0),
            ],
        );
        let summary = summary(&started(), &after, deck());
        assert_eq!(summary.next_due, Some(today() + 7));
    }

    #[test]
    fn the_next_due_skips_what_is_still_due_today() {
        let after = state_of(
            Some(today()),
            vec![
                entry("kho", Stage::Day, today(), 0),
                entry("hoti 1", Stage::Week, today() + 7, 0),
            ],
        );
        let summary = summary(&started(), &after, deck());
        assert_eq!(summary.next_due, Some(today() + 7));
    }

    #[test]
    fn there_is_no_next_due_when_nothing_is_after_today() {
        let after = state_of(Some(today()), vec![entry("kho", Stage::Day, today(), 0)]);
        assert_eq!(summary(&started(), &after, deck()).next_due, None);
        let empty = state_of(Some(today()), vec![]);
        assert_eq!(summary(&empty, &empty, deck()).next_due, None);
    }

    #[test]
    fn without_a_last_today_every_known_due_date_is_a_candidate() {
        let after = state_of(
            None,
            vec![
                entry("kho", Stage::Week, today() + 7, 0),
                entry("hoti 1", Stage::Day, today() + 2, 0),
            ],
        );
        let summary = summary(&started(), &after, deck());
        assert_eq!(summary.next_due, Some(today() + 2));
    }

    #[test]
    fn an_added_word_that_moved_up_is_counted_like_a_deck_term() {
        let before = crate::test_support::added_started(Stage::Day, today());
        let mut after = before.clone();
        after.progress[0].stage = Stage::Week;
        after.progress[0].due = today() + 7;
        after.last_today = Some(today());
        assert_eq!(
            summary(&before, &after, deck()),
            Summary {
                moved_up: 1,
                back_to_day: 0,
                next_due: Some(today() + 7),
            }
        );
    }
}
