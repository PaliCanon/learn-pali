//! Statistics over a session's state and deck.
//!
//! [`Session::stats`] counts the terms the deck knows (a term of the state
//! the deck does not know is an orphan and is counted apart), so
//! `per_stage` sums to the state's size minus the orphans.

use crate::session::Session;

/// What [`Session::stats`] reports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stats {
    /// Started terms known to the deck, per stage, indexed by
    /// [`crate::stage::Stage::index`] (Day first, Year last).
    pub per_stage: [usize; 6],
    /// Started terms known to the deck with `due <= today`.
    pub due_today: usize,
    /// Deck terms not started yet (what `request_new` can still give).
    pub new_left: usize,
    /// The lapses of the started terms known to the deck, summed.
    pub lapses_total: u32,
    /// Terms of the state that the deck does not know.
    pub orphans: usize,
}

impl Session<'_> {
    /// The counts of [`Stats`] for the state as it stands now.
    pub fn stats(&self) -> Stats {
        let mut stats = Stats::default();
        for progress in &self.state.progress {
            if self.deck.term(&progress.term).is_none() {
                stats.orphans += 1;
                continue;
            }
            stats.per_stage[progress.stage.index() as usize] += 1;
            stats.lapses_total += progress.lapses;
            if progress.due <= self.today {
                stats.due_today += 1;
            }
        }
        stats.new_left = self.deck.terms.len() - (self.state.progress.len() - stats.orphans);
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::Date;
    use crate::deck::TermId;
    use crate::deck::samples::{deck as deck_of, indeclinable};
    use crate::facet::FacetSet;
    use crate::progress::{Progress, State};
    use crate::session::Answer;
    use crate::stage::Stage;

    const TODAY: u32 = 20731;

    fn entry(term: &str, stage: Stage, due: u32, lapses: u32) -> Progress {
        Progress {
            term: TermId(term.to_string()),
            stage,
            passed: FacetSet::empty(),
            due: Date::from_days(due),
            lapses,
        }
    }

    fn state_of(progress: Vec<Progress>) -> State {
        State {
            version: State::VERSION,
            last_today: None,
            progress,
        }
    }

    fn five_terms() -> crate::deck::Deck {
        deck_of((1..=5).map(|n| indeclinable(&format!("t{n}"), n)).collect())
    }

    #[test]
    fn an_empty_state_has_everything_left_to_start() {
        let deck = five_terms();
        let session = Session::start(&deck, State::empty(), Date::from_days(TODAY), 0).unwrap();
        let counts = session.stats();
        assert_eq!(
            counts,
            Stats {
                new_left: 5,
                ..Stats::default()
            }
        );
    }

    #[test]
    fn terms_are_counted_per_stage_due_new_lapses_and_orphans() {
        let deck = five_terms();
        let state = state_of(vec![
            entry("t1", Stage::Day, TODAY, 0),
            entry("t2", Stage::Week, TODAY - 3, 2),
            entry("t3", Stage::Week, TODAY + 4, 1),
            entry("ghost", Stage::Year, TODAY - 9, 7),
        ]);
        let session = Session::start(&deck, state, Date::from_days(TODAY), 0).unwrap();
        let counts = session.stats();
        assert_eq!(counts.per_stage, [1, 2, 0, 0, 0, 0]);
        assert_eq!(counts.due_today, 2, "an orphan is never due");
        assert_eq!(counts.new_left, 2);
        assert_eq!(counts.lapses_total, 3, "an orphan's lapses are not counted");
        assert_eq!(counts.orphans, 1);
    }

    #[test]
    fn the_stats_follow_the_session() {
        let deck = five_terms();
        let state = state_of(vec![entry("t1", Stage::Day, TODAY, 0)]);
        let mut session = Session::start(&deck, state, Date::from_days(TODAY), 0).unwrap();
        assert_eq!(session.stats().due_today, 1);
        for _ in 0..2 {
            let question = session.next().cloned().unwrap();
            session
                .answer(&question.term, question.facet, Answer::Pass)
                .unwrap();
        }
        let counts = session.stats();
        assert_eq!(counts.per_stage, [0, 1, 0, 0, 0, 0]);
        assert_eq!(counts.due_today, 0);
        assert_eq!(session.request_new(2), 2);
        let counts = session.stats();
        assert_eq!(counts.per_stage, [2, 1, 0, 0, 0, 0]);
        assert_eq!(counts.due_today, 2);
        assert_eq!(counts.new_left, 2);
    }
}
