//! Leitner stages and their intervals.
//!
//! A term sits in one stage. Its stage says how long to wait after a full
//! pass: [`Stage::Day`] 1 day, [`Stage::Week`] 7, [`Stage::Month`] 30,
//! [`Stage::Quarter`] 90, [`Stage::HalfYear`] 180, [`Stage::Year`] 365. A
//! full pass moves the term to [`Stage::next`]; the last stage stays itself.

use serde::{Deserialize, Serialize};

/// A Leitner stage, serialised as its name (`"HalfYear"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Stage {
    /// 1 day: where every new or failed term starts.
    Day,
    /// 7 days.
    Week,
    /// 30 days.
    Month,
    /// 90 days.
    Quarter,
    /// 180 days.
    HalfYear,
    /// 365 days: the last stage.
    Year,
}

impl Stage {
    /// Every stage, from the shortest interval to the longest.
    pub const ALL: [Stage; 6] = [
        Stage::Day,
        Stage::Week,
        Stage::Month,
        Stage::Quarter,
        Stage::HalfYear,
        Stage::Year,
    ];

    /// Days to wait after a full pass at this stage.
    pub fn interval_days(self) -> u32 {
        match self {
            Stage::Day => 1,
            Stage::Week => 7,
            Stage::Month => 30,
            Stage::Quarter => 90,
            Stage::HalfYear => 180,
            Stage::Year => 365,
        }
    }

    /// The stage after a full pass; [`Stage::Year`] stays [`Stage::Year`].
    #[must_use]
    pub fn next(self) -> Stage {
        match self {
            Stage::Day => Stage::Week,
            Stage::Week => Stage::Month,
            Stage::Month => Stage::Quarter,
            Stage::Quarter => Stage::HalfYear,
            Stage::HalfYear | Stage::Year => Stage::Year,
        }
    }

    /// The position in [`Stage::ALL`], 0 for Day to 5 for Year.
    pub fn index(self) -> u32 {
        self as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_are_the_leitner_days() {
        let days: Vec<u32> = Stage::ALL
            .iter()
            .map(|stage| stage.interval_days())
            .collect();
        assert_eq!(days, vec![1, 7, 30, 90, 180, 365]);
    }

    #[test]
    fn next_climbs_one_stage_and_year_stays_year() {
        let chain: Vec<Stage> = Stage::ALL.iter().map(|stage| stage.next()).collect();
        assert_eq!(
            chain,
            vec![
                Stage::Week,
                Stage::Month,
                Stage::Quarter,
                Stage::HalfYear,
                Stage::Year,
                Stage::Year
            ]
        );
    }

    #[test]
    fn index_runs_from_zero_to_five() {
        let indexes: Vec<u32> = Stage::ALL.iter().map(|stage| stage.index()).collect();
        assert_eq!(indexes, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn stages_order_by_interval() {
        assert!(Stage::Day < Stage::Week && Stage::Quarter < Stage::HalfYear);
        assert!(
            Stage::ALL
                .windows(2)
                .all(|pair| pair[0].interval_days() < pair[1].interval_days())
        );
    }

    #[test]
    fn serde_is_the_name() {
        assert_eq!(
            serde_json::to_string(&Stage::HalfYear).unwrap(),
            "\"HalfYear\""
        );
        for stage in Stage::ALL {
            let json = serde_json::to_string(&stage).unwrap();
            assert_eq!(serde_json::from_str::<Stage>(&json).unwrap(), stage);
        }
        assert!(serde_json::from_str::<Stage>("\"Decade\"").is_err());
    }
}
