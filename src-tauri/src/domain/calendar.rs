//! Calendar identity and date arithmetic.
//!
//! This module is the only place that turns wall-clock time into the string
//! identities stored in `cycles.calendar_key` / `starts_on` / `ends_on`
//! (see `docs/architecture.md`). Formats are fixed by
//! `openspec/specs/planning-cycles/spec.md`:
//!
//! * `day`   -> `day:{YYYY-MM-DD}`            (local date)
//! * `week`  -> `week:{YYYY-MM-DD}`           (the week's first day, which
//!   follows the `week_start_day` setting; cycles created after a setting
//!   change use the new first day, existing cycles keep theirs)
//! * `month` -> `long-term:{starts_on}:{ends_on}`
//! * `session` / the Later container -> no calendar key
//!
//! Dates are local dates (`YYYY-MM-DD`), never UTC conversions, so "which
//! day" cannot drift across time zones. One product "month" is exactly 28
//! days (see `domain/cycle.rs`), so `ends_on` is a plain day addition and
//! never uses calendar-month arithmetic.

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};

use super::cycle::CycleType;

/// `week_start_day` values: ISO weekday numbers, `1` = Monday … `7` = Sunday.
pub const WEEK_START_MONDAY: u32 = 1;
pub const WEEK_START_SUNDAY: u32 = 7;

pub const DAY_KEY_PREFIX: &str = "day:";
pub const WEEK_KEY_PREFIX: &str = "week:";
pub const LONG_TERM_KEY_PREFIX: &str = "long-term:";

/// Today as a local date. This is the single clock read that produces dates;
/// everything else in this module is pure.
pub fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

pub fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

pub fn add_days(date: NaiveDate, days: i64) -> NaiveDate {
    date + Duration::days(days)
}

pub fn day_key(date: NaiveDate) -> String {
    format!("{}{}", DAY_KEY_PREFIX, format_date(date))
}

pub fn week_key(week_start: NaiveDate) -> String {
    format!("{}{}", WEEK_KEY_PREFIX, format_date(week_start))
}

pub fn long_term_key(starts_on: NaiveDate, ends_on: NaiveDate) -> String {
    format!(
        "{}{}:{}",
        LONG_TERM_KEY_PREFIX,
        format_date(starts_on),
        format_date(ends_on)
    )
}

/// First day of the week containing `date`, given `week_start_day`
/// (`1` = Monday … `7` = Sunday). Values outside `1..=7` fall back to Monday.
pub fn start_of_week(date: NaiveDate, week_start_day: u32) -> NaiveDate {
    let target = weekday_from_number(week_start_day);
    let diff =
        (date.weekday().num_days_from_monday() as i64 - target.num_days_from_monday() as i64 + 7)
            % 7;
    add_days(date, -diff)
}

fn weekday_from_number(day: u32) -> Weekday {
    match day {
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        7 => Weekday::Sun,
        _ => Weekday::Mon,
    }
}

/// Whether `week_start_day` is a valid setting value.
pub fn is_valid_week_start_day(day: i64) -> bool {
    (WEEK_START_MONDAY as i64..=WEEK_START_SUNDAY as i64).contains(&day)
}

/// Ends date of a long-term cycle: `starts_on + months * 28 days`. The addition
/// is day-based, so a span that includes a leap day stays exactly N×28 days.
pub fn calculate_ends_on(starts_on: NaiveDate, months: i64) -> NaiveDate {
    add_days(starts_on, months * super::cycle::DAYS_PER_MONTH_UNIT)
}

/// Local date bounds for a dated cycle created "on" `anchor`:
/// day -> `[anchor, anchor+1)`, week -> the setting's week containing the
/// anchor, month -> `[anchor, anchor + duration_days)`. Sessions are undated.
pub fn dated_cycle_bounds(
    anchor: NaiveDate,
    cycle_type: CycleType,
    week_start_day: u32,
    long_term_duration_days: i64,
) -> Option<(NaiveDate, NaiveDate)> {
    match cycle_type {
        CycleType::Day => Some((anchor, add_days(anchor, 1))),
        CycleType::Week => {
            let start = start_of_week(anchor, week_start_day);
            Some((start, add_days(start, 7)))
        }
        CycleType::Month => Some((anchor, add_days(anchor, long_term_duration_days))),
        CycleType::Session => None,
    }
}

/// Whole weeks from `today` until `ends_on`, floored at zero. Long-term
/// durations are whole weeks by construction, so this never produces a
/// fraction (spec: "剩余时间读数为整数").
pub fn remaining_whole_weeks(today: NaiveDate, ends_on: NaiveDate) -> i64 {
    let days = (ends_on - today).num_days();
    if days <= 0 {
        0
    } else {
        days / 7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).expect("test date parses")
    }

    #[test]
    fn parse_format_round_trip() {
        assert_eq!(format_date(d("2026-09-14")), "2026-09-14");
        assert_eq!(parse_date("not a date"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("2026-02-30"), None);
    }

    #[test]
    fn day_key_format() {
        assert_eq!(day_key(d("2026-09-14")), "day:2026-09-14");
    }

    #[test]
    fn week_key_uses_week_start_date() {
        // Monday-start: the week containing Thu 2026-09-10 starts Mon 2026-09-07.
        assert_eq!(
            week_key(start_of_week(d("2026-09-10"), 1)),
            "week:2026-09-07"
        );
    }

    #[test]
    fn long_term_key_contains_both_bounds() {
        let key = long_term_key(d("2026-09-14"), d("2026-12-07"));
        assert_eq!(key, "long-term:2026-09-14:2026-12-07");
    }

    #[test]
    fn start_of_week_monday() {
        // 2026-09-14 is a Monday.
        assert_eq!(start_of_week(d("2026-09-14"), 1), d("2026-09-14"));
        assert_eq!(start_of_week(d("2026-09-16"), 1), d("2026-09-14"));
        assert_eq!(start_of_week(d("2026-09-20"), 1), d("2026-09-14"));
    }

    #[test]
    fn start_of_week_sunday() {
        // Sunday-start: the week containing Mon 2026-09-14 starts Sun 2026-09-13.
        assert_eq!(start_of_week(d("2026-09-14"), 7), d("2026-09-13"));
        assert_eq!(start_of_week(d("2026-09-19"), 7), d("2026-09-13"));
        assert_eq!(start_of_week(d("2026-09-20"), 7), d("2026-09-20"));
    }

    #[test]
    fn start_of_week_saturday() {
        assert_eq!(start_of_week(d("2026-09-14"), 6), d("2026-09-12"));
    }

    #[test]
    fn start_of_week_across_year_boundary() {
        // 2027-01-01 is a Friday.
        assert_eq!(start_of_week(d("2027-01-01"), 1), d("2026-12-28"));
        assert_eq!(start_of_week(d("2027-01-01"), 7), d("2026-12-27"));
        // 2026-01-01 is a Thursday.
        assert_eq!(start_of_week(d("2026-01-01"), 1), d("2025-12-29"));
    }

    #[test]
    fn calendar_key_ignores_week_start_setting() {
        // The *bounds* move with the setting, but a day key never does and the
        // week key is derived from whichever bounds were chosen at creation.
        let day = d("2026-09-14");
        assert_eq!(day_key(day), day_key(day));
        let mon_start = start_of_week(day, 1);
        let sun_start = start_of_week(day, 7);
        assert_ne!(week_key(mon_start), week_key(sun_start));
    }

    #[test]
    fn leap_day_is_valid_and_adds_correctly() {
        let leap = d("2024-02-29");
        assert_eq!(format_date(leap), "2024-02-29");
        assert_eq!(format_date(add_days(leap, 1)), "2024-03-01");
        assert_eq!(parse_date("2023-02-29"), None);
        assert_eq!(parse_date("2100-02-29"), None); // century non-leap
        assert_eq!(parse_date("2000-02-29"), Some(d("2000-02-29"))); // 400-rule leap
    }

    #[test]
    fn calculate_ends_on_is_days_not_calendar_months() {
        // 3 product months = 84 days: 2026-09-14 + 84 = 2026-12-07
        // (the exact example in docs/architecture.md). Calendar months would
        // give 2026-12-14 (91 days), which is not a whole number of weeks.
        assert_eq!(calculate_ends_on(d("2026-09-14"), 3), d("2026-12-07"));
        assert_eq!(calculate_ends_on(d("2026-09-14"), 1), d("2026-10-12"));
        assert_eq!(calculate_ends_on(d("2026-09-14"), 6), d("2027-03-01"));
    }

    #[test]
    fn calculate_ends_on_across_leap_day() {
        // 2024-01-01 + 84 days lands on 2024-03-25 and includes 2024-02-29.
        assert_eq!(calculate_ends_on(d("2024-01-01"), 3), d("2024-03-25"));
    }

    #[test]
    fn calculate_ends_on_crosses_year() {
        assert_eq!(calculate_ends_on(d("2026-11-20"), 1), d("2026-12-18"));
        assert_eq!(calculate_ends_on(d("2026-12-01"), 3), d("2027-02-23"));
    }

    #[test]
    fn dated_bounds_per_type() {
        let today = d("2026-09-16"); // Wednesday
        let (ds, de) = dated_cycle_bounds(today, CycleType::Day, 1, 0).unwrap();
        assert_eq!(
            (format_date(ds), format_date(de)),
            ("2026-09-16".to_string(), "2026-09-17".to_string())
        );
        let (ws, we) = dated_cycle_bounds(today, CycleType::Week, 1, 0).unwrap();
        assert_eq!(
            (format_date(ws), format_date(we)),
            ("2026-09-14".to_string(), "2026-09-21".to_string())
        );
        let (ms, me) = dated_cycle_bounds(today, CycleType::Month, 1, 84).unwrap();
        assert_eq!(
            (format_date(ms), format_date(me)),
            ("2026-09-16".to_string(), "2026-12-09".to_string())
        );
        assert!(dated_cycle_bounds(today, CycleType::Session, 1, 0).is_none());
    }

    #[test]
    fn remaining_weeks_is_whole() {
        let today = d("2026-09-14");
        // 84-day cycle after 21 days: 63 days left = 9 weeks (never 8.9).
        assert_eq!(remaining_whole_weeks(today, add_days(today, 63)), 9);
        assert_eq!(remaining_whole_weeks(today, today), 0);
        assert_eq!(remaining_whole_weeks(today, add_days(today, -1)), 0);
    }

    #[test]
    fn week_start_day_validation() {
        assert!(is_valid_week_start_day(1));
        assert!(is_valid_week_start_day(7));
        assert!(!is_valid_week_start_day(0));
        assert!(!is_valid_week_start_day(8));
    }
}
