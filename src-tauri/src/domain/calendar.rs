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

// --- calendar-view grid projection (change: add-calendar-time-view §1) ------
//
// Pure helpers for the calendar view's range query. Month and week grids both
// span whole weeks, so aligning both bounds of the requested range to the
// setting's week boundaries guarantees the returned cells tile the grid with
// no holes regardless of which month/week the user is looking at.

/// Expands `[start, end]` to the enclosing whole weeks: `start` back to the
/// week's first day, `end` forward to the week's last day. A reversed range
/// (`start > end`) is returned unchanged; the caller rejects it.
pub fn align_range_to_weeks(
    start: NaiveDate,
    end: NaiveDate,
    week_start_day: u32,
) -> (NaiveDate, NaiveDate) {
    if start > end {
        return (start, end);
    }
    let grid_start = start_of_week(start, week_start_day);
    let grid_end = add_days(start_of_week(end, week_start_day), 6);
    (grid_start, grid_end)
}

/// Every date in `[start, end]`, inclusive, in ascending order. An empty or
/// reversed range yields no dates.
pub fn dates_between(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        dates.push(cursor);
        cursor = add_days(cursor, 1);
    }
    dates
}

/// Local-midnight bounds of the day containing the instant `at_ms`:
/// `(day_start_ms, next_day_start_ms)` in epoch milliseconds. Like
/// [`today_local`], this is one of the module's few clock-aware reads; every
/// schedule projection derives from it. Ambiguous local times (DST fold) take
/// the earlier reading; a nonexistent local time (DST gap, `at_ms` can still
/// be represented as an instant but its naive fields repeat) falls back to a
/// UTC interpretation so the function is total.
pub fn local_day_bounds_ms(at_ms: i64) -> (i64, i64) {
    use chrono::{LocalResult, TimeZone};
    let at = match Local.timestamp_millis_opt(at_ms) {
        LocalResult::Single(at) => at,
        LocalResult::Ambiguous(at, _) => at,
        LocalResult::None => return (at_ms, at_ms),
    };
    let to_ms = |naive: chrono::NaiveDateTime| -> i64 {
        match Local.from_local_datetime(&naive) {
            LocalResult::Single(at) => at.timestamp_millis(),
            LocalResult::Ambiguous(at, _) => at.timestamp_millis(),
            LocalResult::None => naive.and_utc().timestamp_millis(),
        }
    };
    let day_start = at.date_naive().and_hms_opt(0, 0, 0).expect("midnight");
    let next_start = add_days(at.date_naive(), 1)
        .and_hms_opt(0, 0, 0)
        .expect("midnight");
    (to_ms(day_start), to_ms(next_start))
}

/// Clamps the planned interval `[starts_at, starts_at + duration_ms)` to the
/// local day that contains `starts_at`, returning `(ends_at, truncated)`.
/// Cross-midnight schedules stay on their day and are flagged so the timeline
/// can mark them (spec: 跨午夜按时长截断到当日并标注). Non-positive durations
/// produce a zero-length interval at `starts_at` (never truncated).
pub fn truncate_schedule_to_day(starts_at: i64, duration_ms: i64) -> (i64, bool) {
    if duration_ms <= 0 {
        return (starts_at, false);
    }
    let (_, day_end) = local_day_bounds_ms(starts_at);
    let raw_end = starts_at.saturating_add(duration_ms);
    if raw_end > day_end {
        (day_end, true)
    } else {
        (raw_end, false)
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

/// Tests for the calendar-view grid projection helpers (change:
/// add-calendar-time-view §1.3's pure-function share). Kept as a separate
/// module so the pre-existing `tests` module above stays untouched.
#[cfg(test)]
mod grid_tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).expect("test date parses")
    }

    #[test]
    fn align_range_to_weeks_pads_month_edges() {
        // October 2026: Thu 10-01 … Sat 10-31. Monday weeks → 09-28 … 11-01.
        let (start, end) = align_range_to_weeks(d("2026-10-01"), d("2026-10-31"), 1);
        assert_eq!(
            (format_date(start), format_date(end)),
            ("2026-09-28".into(), "2026-11-01".into())
        );
        // The whole padded range is 5 full weeks — no holes in the grid.
        assert_eq!(dates_between(start, end).len(), 35);
    }

    #[test]
    fn align_range_keeps_aligned_bounds_and_handles_cross_month() {
        // Already week-aligned: unchanged.
        let (start, end) = align_range_to_weeks(d("2026-09-14"), d("2026-09-20"), 1);
        assert_eq!(
            (format_date(start), format_date(end)),
            ("2026-09-14".into(), "2026-09-20".into())
        );
        // A cross-month week under Sunday start: 2027-01-01 is a Friday, so
        // the week runs 2026-12-27 … 2027-01-02 (covers the year boundary).
        let (start, end) = align_range_to_weeks(d("2027-01-01"), d("2027-01-01"), 7);
        assert_eq!(
            (format_date(start), format_date(end)),
            ("2026-12-27".into(), "2027-01-02".into())
        );
    }

    #[test]
    fn dates_between_is_inclusive_and_reversed_is_empty() {
        let days = dates_between(d("2026-09-30"), d("2026-10-02"));
        assert_eq!(
            days.iter().map(|d| format_date(*d)).collect::<Vec<_>>(),
            ["2026-09-30", "2026-10-01", "2026-10-02"]
        );
        assert!(dates_between(d("2026-10-02"), d("2026-09-30")).is_empty());
        assert_eq!(dates_between(d("2026-09-30"), d("2026-09-30")).len(), 1);
    }

    #[test]
    fn schedule_inside_day_is_not_truncated() {
        let (day_start, day_end) = local_day_bounds_ms(local_noon_ms());
        let (ends_at, truncated) = truncate_schedule_to_day(day_start + 3 * 3_600_000, 45 * 60_000);
        assert!(!truncated);
        assert_eq!(ends_at, day_start + 3 * 3_600_000 + 45 * 60_000);
        assert!(ends_at <= day_end);
    }

    #[test]
    fn cross_midnight_schedule_is_cut_at_midnight_and_flagged() {
        let (_day_start, day_end) = local_day_bounds_ms(local_noon_ms());
        // 23:00 for two hours crosses midnight: the end is clipped to the day
        // boundary and the flag tells the timeline to mark it.
        let (ends_at, truncated) = truncate_schedule_to_day(day_end - 3_600_000, 2 * 3_600_000);
        assert!(truncated);
        assert_eq!(ends_at, day_end);
        // Exactly reaching midnight is not a truncation.
        let (ends_at, truncated) = truncate_schedule_to_day(day_end - 3_600_000, 3_600_000);
        assert!(!truncated);
        assert_eq!(ends_at, day_end);
    }

    #[test]
    fn zero_and_negative_durations_never_truncate() {
        let now = local_noon_ms();
        assert_eq!(truncate_schedule_to_day(now, 0), (now, false));
        assert_eq!(truncate_schedule_to_day(now, -1_000), (now, false));
    }

    /// A fixed reference instant: local noon of whatever day the test runs on,
    /// so `local_day_bounds_ms` assertions stay timezone-independent.
    fn local_noon_ms() -> i64 {
        use chrono::TimeZone;
        let noon = chrono::Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .expect("noon");
        Local
            .from_local_datetime(&noon)
            .single()
            .map(|t| t.timestamp_millis())
            .expect("local noon resolves unambiguously in test timezones")
    }
}
