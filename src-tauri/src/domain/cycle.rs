//! Cycle domain: the time container hierarchy `month -> week -> day -> session`.
//!
//! Behaviour contract: `openspec/specs/planning-cycles/spec.md`.
//! The database also enforces `NOT (started = 0 AND finished = 1)`; the
//! functions here are the application-side authority for state transitions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CycleType {
    Session,
    Day,
    Week,
    Month,
}impl CycleType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "session" => Some(Self::Session),
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            _ => None,
        }
    }

    /// `month` is the product's "Long-term cycle".
    pub fn is_long_term(self) -> bool {
        matches!(self, Self::Month)
    }

    /// Whether `cycle_key`/`calendar_key` identity applies to this type.
    /// Long-term cycles are identified by their date bounds instead.
    pub fn has_calendar_key(self) -> bool {
        matches!(self, Self::Day | Self::Week)
    }
}

/// One row of `cycles`, as it travels across IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cycle {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub cycle_type: CycleType,
    pub parent_id: Option<String>,
    pub position: i64,
    pub archived: bool,
    pub started: bool,
    pub finished: bool,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    /// Milliseconds; `None` = duration not set yet (commitment not made).
    pub duration: Option<i64>,
    /// Accumulated focus time in milliseconds.
    pub focused_time: i64,
    /// Local dates, `YYYY-MM-DD`.
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub calendar_key: Option<String>,
    /// Template this session was generated from; cleared when the template is
    /// removed (unlink, never cascade).
    pub repeat_id: Option<String>,
    pub created_at: i64,
}

impl Cycle {
    pub fn lifecycle(&self) -> Option<LifecycleState> {
        LifecycleState::from_flags(self.started, self.finished)
    }
}

/// Id of the always-present "Later" holding container.
pub const LATER_CYCLE_ID: &str = "later";

pub const MS_PER_DAY: i64 = 86_400_000;
pub const MS_PER_WEEK: i64 = 7 * MS_PER_DAY;

/// One "month" in this product is exactly 28 days (4 weeks), not a calendar
/// month. Upstream proof: the seeded onboarding month used
/// `2419200000 // 28 days in milliseconds`, and the create flow promises
/// "12 weeks left" for a 3-month cycle (= 84 days), which calendar months
/// (91 days) would not satisfy.
pub const DAYS_PER_MONTH_UNIT: i64 = 28;

/// Durations offered for a long-term cycle, in product "months".
pub const LONG_TERM_DURATIONS_MONTHS: [i64; 3] = [1, 3, 6];

/// Milliseconds for a long-term duration expressed in product months.
pub const fn long_term_duration_ms(months: i64) -> i64 {
    months * DAYS_PER_MONTH_UNIT * MS_PER_DAY
}

/// Fixed duration of a `week` cycle. Upstream copy:
/// "Weekly planning cycles use a fixed 7-day duration."
pub const WEEK_DURATION_MS: i64 = MS_PER_WEEK;

/// Fixed duration of a `day` cycle.
pub const DAY_DURATION_MS: i64 = MS_PER_DAY;

/// Whether `duration_ms` is one of the three permitted long-term durations.
pub fn is_valid_long_term_duration(duration_ms: i64) -> bool {
    LONG_TERM_DURATIONS_MONTHS
        .iter()
        .any(|m| long_term_duration_ms(*m) == duration_ms)
}

/// Whether `duration_ms` is a whole number of weeks.
///
/// Non-whole-week durations are rejected so that remaining-time readouts
/// ("11 weeks left") stay integral.
pub fn is_whole_weeks(duration_ms: i64) -> bool {
    duration_ms > 0 && duration_ms % MS_PER_WEEK == 0
}

/// Long-term duration in days for a number of product months.
pub fn long_term_duration_days(months: i64) -> i64 {
    months * DAYS_PER_MONTH_UNIT
}

/// Lifecycle of a cycle, derived from its `started` / `finished` flags.
/// The database CHECK `NOT (started = 0 AND finished = 1)` makes the
/// `FinishedButNotStarted` input impossible to store; the domain still
/// treats it as invalid input rather than panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    NotStarted,
    Started,
    Finished,
}

impl LifecycleState {
    pub fn from_flags(started: bool, finished: bool) -> Option<Self> {
        match (started, finished) {
            (false, false) => Some(Self::NotStarted),
            (true, false) => Some(Self::Started),
            (true, true) => Some(Self::Finished),
            (false, true) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    Start,
    Finish,
}

/// A rejected transition. `code` is stable and travels across IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleError {
    pub code: &'static str,
    pub message: &'static str,
}

/// Pure lifecycle state machine. Illegal transitions are rejected here so the
/// service layer never writes a state the database CHECK would abort on.
pub fn transition(state: LifecycleState, action: LifecycleAction) -> Result<LifecycleState, LifecycleError> {
    match (state, action) {
        (LifecycleState::NotStarted, LifecycleAction::Start) => Ok(LifecycleState::Started),
        (LifecycleState::Started, LifecycleAction::Finish) => Ok(LifecycleState::Finished),
        (LifecycleState::Started, LifecycleAction::Start) => Err(LifecycleError {
            code: "cycle_already_started",
            message: "This cycle has already been started.",
        }),
        (LifecycleState::NotStarted, LifecycleAction::Finish) => Err(LifecycleError {
            code: "cycle_not_started",
            message: "This cycle has not been started yet.",
        }),
        (LifecycleState::Finished, LifecycleAction::Start) => Err(LifecycleError {
            code: "cycle_already_finished",
            message: "This cycle has already finished.",
        }),
        (LifecycleState::Finished, LifecycleAction::Finish) => Err(LifecycleError {
            code: "cycle_already_finished",
            message: "This cycle has already finished.",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_term_durations_in_ms() {
        assert_eq!(long_term_duration_ms(1), 2_419_200_000);
        assert_eq!(long_term_duration_ms(3), 7_257_600_000);
        assert_eq!(long_term_duration_ms(6), 14_515_200_000);
        assert!(is_valid_long_term_duration(2_419_200_000));
        assert!(is_valid_long_term_duration(7_257_600_000));
        assert!(is_valid_long_term_duration(14_515_200_000));
        // 3 calendar months (91 days) is not a permitted duration.
        assert!(!is_valid_long_term_duration(91 * MS_PER_DAY));
        assert!(!is_valid_long_term_duration(0));
    }

    #[test]
    fn whole_week_check() {
        assert!(is_whole_weeks(MS_PER_WEEK));
        assert!(is_whole_weeks(28 * MS_PER_DAY));
        assert!(is_whole_weeks(84 * MS_PER_DAY));
        assert!(is_whole_weeks(168 * MS_PER_DAY));
        // 91 days is 13 whole weeks, but it is rejected by the allowed-set
        // check, not by the whole-weeks check (see the test above).
        assert!(is_whole_weeks(91 * MS_PER_DAY));
        assert!(!is_whole_weeks(90 * MS_PER_DAY));
        assert!(!is_whole_weeks(0));
        assert!(!is_whole_weeks(-MS_PER_WEEK));
    }

    #[test]
    fn state_from_flags() {
        assert_eq!(LifecycleState::from_flags(false, false), Some(LifecycleState::NotStarted));
        assert_eq!(LifecycleState::from_flags(true, false), Some(LifecycleState::Started));
        assert_eq!(LifecycleState::from_flags(true, true), Some(LifecycleState::Finished));
        assert_eq!(LifecycleState::from_flags(false, true), None);
    }

    #[test]
    fn legal_transitions() {
        let s = |a, b| transition(a, b).unwrap();
        assert_eq!(s(LifecycleState::NotStarted, LifecycleAction::Start), LifecycleState::Started);
        assert_eq!(s(LifecycleState::Started, LifecycleAction::Finish), LifecycleState::Finished);
    }

    #[test]
    fn illegal_transitions_are_rejected() {
        let err = |a, b| transition(a, b).unwrap_err().code;
        // every illegal move, one case each
        assert_eq!(err(LifecycleState::Started, LifecycleAction::Start), "cycle_already_started");
        assert_eq!(err(LifecycleState::Finished, LifecycleAction::Start), "cycle_already_finished");
        assert_eq!(err(LifecycleState::Finished, LifecycleAction::Finish), "cycle_already_finished");
        assert_eq!(err(LifecycleState::NotStarted, LifecycleAction::Finish), "cycle_not_started");
    }
}
