//! Typed, human-approved operations that do not fit a task-row snapshot.
//! Models can only stage these. The GUI owns the separate decision command.
use crate::providers::service::AiSettingsState;
use crate::{
    ai::prioritization,
    db::Db,
    error::{AppError, AppResult},
    repository as repo, service,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum CycleAction {
    Create {
        cycle_type: String,
        date: Option<String>,
        title: Option<String>,
        duration_months: Option<i64>,
        parent_id: Option<String>,
    },
    Start {
        cycle_id: String,
    },
    Finish {
        cycle_id: String,
    },
    Delete {
        cycle_id: String,
    },
    CopyUncompleted {
        cycle_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum FocusAction {
    Create {
        day_cycle_id: String,
        title: String,
        duration_minutes: i64,
    },
    Update {
        session_id: String,
        title: String,
        duration_minutes: i64,
    },
    Schedule {
        session_id: String,
        starts_at: Option<String>,
    },
    Reorder {
        day_cycle_id: String,
        session_ids: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskAction {
    Move {
        task_id: String,
        target_cycle_id: String,
    },
    Link {
        task_id: String,
        #[serde(deserialize_with = "required_nullable")]
        parent_id: Option<String>,
    },
    Color {
        task_id: String,
        #[serde(deserialize_with = "required_nullable")]
        color: Option<String>,
    },
    Reorder {
        cycle_id: String,
        #[serde(deserialize_with = "required_nullable")]
        parent_id: Option<String>,
        task_ids: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReminderAction {
    Create {
        target_kind: String,
        target_id: String,
        fire_at: String,
        respect_quiet_hours: bool,
    },
    Update {
        reminder_id: String,
        fire_at: String,
        respect_quiet_hours: bool,
    },
    Delete {
        reminder_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepeatAction {
    Create {
        session_id: String,
    },
    Update {
        repeat_id: String,
        title: String,
        duration_minutes: i64,
    },
    Stop {
        repeat_id: String,
    },
}
/// One setting per approval: no partial multi-setting commits or arbitrary keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "setting", rename_all = "snake_case", deny_unknown_fields)]
pub enum SettingAction {
    Locale {
        value: String,
    },
    Theme {
        value: String,
    },
    WeekStartDay {
        value: i64,
    },
    CoachIdleMinutes {
        value: i64,
    },
    PlanWithAi {
        value: bool,
    },
    DailyCapacityMinutes {
        #[serde(deserialize_with = "required_nullable")]
        value: Option<i64>,
    },
    DailyReminder {
        #[serde(deserialize_with = "required_nullable")]
        value: Option<String>,
    },
    QuietHours {
        #[serde(deserialize_with = "required_nullable")]
        start: Option<String>,
        #[serde(deserialize_with = "required_nullable")]
        end: Option<String>,
    },
    LogLevel {
        value: String,
    },
    ActiveModel {
        provider_id: String,
        model_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrioritizationAction {
    Update {
        cycle_id: String,
        update: prioritization::PrioritizationBreakdownUpdate,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayMove {
    pub cycle_id: String,
    pub target_date: String,
    pub strategy: Option<String>,
}

fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "change", rename_all = "snake_case")]
pub enum Action {
    Cycle(CycleAction),
    Focus(FocusAction),
    Task(TaskAction),
    Reminder(ReminderAction),
    Repeat(RepeatAction),
    Settings(SettingAction),
    Prioritization(PrioritizationAction),
    DayMove(DayMove),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: String,
    pub source_cycle_id: String,
    pub action: Action,
    pub rationale: String,
    pub summary: String,
    pub summary_key: String,
    pub details: Vec<crate::i18n::LocalizedMessage>,
    pub state: String,
    #[serde(skip)]
    legacy_details: bool,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::validation("invalid_arguments", message)
}
fn db_error(e: rusqlite::Error) -> AppError {
    AppError::Db(e.to_string())
}
pub fn timestamp(value: &str) -> AppResult<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|d| d.timestamp_millis())
        .map_err(|_| {
            invalid("Use RFC3339 with an explicit timezone, e.g. 2026-09-15T14:30:00+08:00")
        })
}
fn minutes(value: i64) -> AppResult<i64> {
    if !(1..=1440).contains(&value) {
        return Err(invalid("Duration must be 1–1440 whole minutes"));
    }
    Ok(value * 60_000)
}

impl Action {
    /// Shape/value validation happens before a proposal is offered; domain
    /// invariants are rechecked by the same services as GUI writes on approval.
    pub fn validate(&self) -> AppResult<()> {
        match self {
            Self::Focus(
                FocusAction::Create {
                    title,
                    duration_minutes,
                    ..
                }
                | FocusAction::Update {
                    title,
                    duration_minutes,
                    ..
                },
            )
            | Self::Repeat(RepeatAction::Update {
                title,
                duration_minutes,
                ..
            }) => {
                if title.trim().is_empty() {
                    return Err(invalid("Title cannot be empty"));
                }
                minutes(*duration_minutes)?;
            }
            Self::Focus(FocusAction::Schedule { starts_at, .. }) => {
                if let Some(start) = starts_at {
                    timestamp(start)?;
                }
            }
            Self::Reminder(ReminderAction::Create {
                target_kind,
                fire_at,
                ..
            }) => {
                if !["task", "session", "day", "cycle"].contains(&target_kind.as_str()) {
                    return Err(invalid("Unknown reminder target kind"));
                }
                timestamp(fire_at)?;
            }
            Self::Reminder(ReminderAction::Update { fire_at, .. }) => {
                timestamp(fire_at)?;
            }
            Self::Task(TaskAction::Color {
                color: Some(key), ..
            }) if !crate::domain::task::is_valid_root_color_key(key) => {
                return Err(invalid("Unknown goal color"))
            }
            Self::Settings(setting) => match setting {
                SettingAction::Locale { value } if crate::i18n::Locale::parse(value).is_none() => {
                    return Err(invalid("Choose English or Simplified Chinese"))
                }
                SettingAction::Theme { value } if !service::settings::is_valid_theme(value) => {
                    return Err(invalid("Theme must be white or gray"))
                }
                SettingAction::WeekStartDay { value } if !(1..=7).contains(value) => {
                    return Err(invalid("Week starts on 1 (Monday) through 7 (Sunday)"))
                }
                SettingAction::CoachIdleMinutes { value } if !(1..=1440).contains(value) => {
                    return Err(invalid("Coach timeout must be 1–1440 minutes"))
                }
                SettingAction::DailyCapacityMinutes { value: Some(value) }
                    if !(1..=1440).contains(value) =>
                {
                    return Err(invalid("Daily capacity must be 1–1440 minutes or null"))
                }
                SettingAction::DailyReminder { value: Some(value) }
                    if service::reminders::parse_hh_mm(value).is_none() =>
                {
                    return Err(invalid("Daily reminder must be HH:MM or null"))
                }
                SettingAction::QuietHours { start, end } => match (start, end) {
                    (None, None) => {}
                    (Some(a), Some(b))
                        if a != b
                            && service::reminders::parse_hh_mm(a).is_some()
                            && service::reminders::parse_hh_mm(b).is_some() => {}
                    _ => {
                        return Err(invalid(
                            "Quiet hours require two distinct HH:MM values, or both null",
                        ))
                    }
                },
                SettingAction::LogLevel { value }
                    if crate::logging::Level::parse(value).is_none() =>
                {
                    return Err(invalid("Log level must be error, warn, info or debug"))
                }
                _ => {}
            },
            Self::DayMove(change) => {
                chrono::NaiveDate::parse_from_str(&change.target_date, "%Y-%m-%d")
                    .map_err(|_| invalid("Date must be YYYY-MM-DD"))?;
                if change
                    .strategy
                    .as_deref()
                    .is_some_and(|s| !["merge", "swap"].contains(&s))
                {
                    return Err(invalid(
                        "Strategy must be merge, swap or null (fail if occupied)",
                    ));
                }
            }
            Self::Cycle(CycleAction::Create {
                cycle_type,
                duration_months,
                date,
                ..
            }) => {
                if !["month", "week", "day"].contains(&cycle_type.as_str()) {
                    return Err(invalid("cycle_type must be month, week or day"));
                }
                if cycle_type == "month" && !duration_months.is_some_and(|m| [1, 3, 6].contains(&m))
                {
                    return Err(invalid(
                        "Long-term duration must be 1, 3 or 6 product months",
                    ));
                }
                if cycle_type != "day" && date.is_some() {
                    return Err(invalid("Only day creation accepts date; weekly/long-term containers start from the current local date"));
                }
                if cycle_type != "month" && duration_months.is_some() {
                    return Err(invalid("duration_months applies only to long-term cycles"));
                }
                if let Some(date) = date {
                    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                        .map_err(|_| invalid("Date must be YYYY-MM-DD"))?;
                }
            }
            Self::Prioritization(PrioritizationAction::Update { cycle_id, .. })
                if cycle_id.trim().is_empty() =>
            {
                return Err(invalid("cycle_id cannot be empty"));
            }
            _ => {}
        }
        Ok(())
    }

    /// Labels resolve IDs from the database, not from untrusted model prose.
    /// The returned key and arguments are stable across interface locales, so
    /// approval freshness never changes when the user switches language.
    fn describe(&self, db: &Db) -> AppResult<(String, Vec<crate::i18n::LocalizedMessage>)> {
        let conn = db.pool().get()?;
        let cycle = |id: &str| -> AppResult<String> {
            let c = repo::cycles::require(&conn, id)?;
            Ok(format!(
                "{} · {}",
                c.title,
                c.starts_on.as_deref().unwrap_or(c.cycle_type.as_str())
            ))
        };
        let task = |id: &str| -> AppResult<String> { Ok(repo::tasks::require(&conn, id)?.title) };
        let m = |key: &str, args: Value| {
            crate::i18n::LocalizedMessage::new(format!("backend-actions:{key}"), args)
        };
        let target = |key: &str, value: String| m(key, json!({"target": value}));
        let cycle_type_message = |value: &str| {
            m(
                match value {
                    "month" => "cycle.type.month",
                    "week" => "cycle.type.week",
                    "day" => "cycle.type.day",
                    _ => "cycle.type.unknown",
                },
                json!({"code": value}),
            )
        };
        let color_message = |value: &str| {
            m(
                match value {
                    "red" => "task.color.red",
                    "amber" => "task.color.amber",
                    "gold" => "task.color.gold",
                    "green" => "task.color.green",
                    "teal" => "task.color.teal",
                    "blue" => "task.color.blue",
                    "indigo" => "task.color.indigo",
                    "plum" => "task.color.plum",
                    _ => "task.color.unknown",
                },
                json!({"code": value}),
            )
        };
        let strategy = |value: Option<&str>| match value {
            Some("merge") => m("day_move.strategy.merge", json!({"code": "merge"})),
            Some("swap") => m("day_move.strategy.swap", json!({"code": "swap"})),
            Some(value) => m("day_move.strategy.unknown", json!({"code": value})),
            None => m("day_move.strategy_fail", json!({})),
        };
        match self {
            Self::Cycle(CycleAction::Create {
                cycle_type,
                date,
                title,
                duration_months,
                parent_id,
            }) => Ok((
                "backend-actions:cycle.create".into(),
                vec![
                    m(
                        "cycle.create.type",
                        json!({
                            "type": serde_json::to_value(cycle_type_message(cycle_type)).unwrap(),
                            "type_code": cycle_type,
                            "name": title.as_deref().map(|value| Value::String(value.to_owned())).unwrap_or_else(|| serde_json::to_value(m("cycle.create.default_name", json!({}))).unwrap()),
                        }),
                    ),
                    m(
                        "cycle.create.date",
                        json!({
                            "date": date.as_deref().map(|value| Value::String(value.to_owned())).unwrap_or_else(|| serde_json::to_value(m("cycle.create.today", json!({}))).unwrap()),
                            "duration": duration_months.map(|months| serde_json::to_value(m("cycle.create.duration", json!({"months": months}))).unwrap()).unwrap_or_else(|| serde_json::to_value(m("cycle.create.no_duration", json!({}))).unwrap()),
                        }),
                    ),
                    m(
                        "cycle.create.parent",
                        json!({
                            "parent": parent_id.as_deref().map(&cycle).transpose()?.map(Value::String).unwrap_or_else(|| serde_json::to_value(m("cycle.create.no_parent", json!({}))).unwrap()),
                        }),
                    ),
                ],
            )),
            Self::Cycle(change) => {
                let (summary, cycle_id) = match change {
                    CycleAction::Start { cycle_id } => ("backend-actions:cycle.start", cycle_id),
                    CycleAction::Finish { cycle_id } => ("backend-actions:cycle.finish", cycle_id),
                    CycleAction::Delete { cycle_id } => ("backend-actions:cycle.delete", cycle_id),
                    CycleAction::CopyUncompleted { cycle_id } => {
                        ("backend-actions:cycle.copy_uncompleted", cycle_id)
                    }
                    CycleAction::Create { .. } => unreachable!(),
                };
                let mut details = vec![target("cycle.target", cycle(cycle_id)?)];
                if matches!(change, CycleAction::Delete { .. }) {
                    let impact = service::cycles::get_cycle_deletion_preview(db, cycle_id)?;
                    if let Some(code) = impact.guard_code {
                        return Err(AppError::validation(
                            code,
                            impact.guard_message.unwrap_or_default(),
                        ));
                    }
                    details.push(m(
                        "cycle.delete.impact",
                        json!({"cycles": impact.descendant_cycles, "tasks": impact.tasks}),
                    ));
                }
                Ok((summary.into(), details))
            }
            Self::Focus(FocusAction::Create {
                day_cycle_id,
                title,
                duration_minutes,
            }) => Ok((
                "backend-actions:focus.create".into(),
                vec![
                    target("focus.target", cycle(day_cycle_id)?),
                    m(
                        "focus.duration",
                        json!({"title": title, "minutes": duration_minutes}),
                    ),
                ],
            )),
            Self::Focus(FocusAction::Update {
                session_id,
                title,
                duration_minutes,
            }) => Ok((
                "backend-actions:focus.update".into(),
                vec![
                    target("focus.target", cycle(session_id)?),
                    m(
                        "focus.updated",
                        json!({"title": title, "minutes": duration_minutes}),
                    ),
                ],
            )),
            Self::Focus(FocusAction::Schedule {
                session_id,
                starts_at,
            }) => {
                let detail = match starts_at {
                    Some(value) => m("focus.schedule.start", json!({"startsAt": value})),
                    None => m("focus.schedule.unscheduled", json!({})),
                };
                Ok((
                    if starts_at.is_some() {
                        "backend-actions:focus.schedule"
                    } else {
                        "backend-actions:focus.unschedule"
                    }
                    .into(),
                    vec![target("focus.target", cycle(session_id)?), detail],
                ))
            }
            Self::Focus(FocusAction::Reorder {
                day_cycle_id,
                session_ids,
            }) => Ok((
                "backend-actions:focus.reorder".into(),
                vec![
                    target("focus.target", cycle(day_cycle_id)?),
                    m(
                        "focus.reorder.order",
                        json!({"items": session_ids.iter().map(|id| cycle(id)).collect::<AppResult<Vec<_>>>()?.join(" → ")}),
                    ),
                ],
            )),
            Self::Task(TaskAction::Move {
                task_id,
                target_cycle_id,
            }) => Ok((
                "backend-actions:task.move".into(),
                vec![
                    target("task.target", task(task_id)?),
                    m(
                        "task.destination",
                        json!({"target": cycle(target_cycle_id)?}),
                    ),
                ],
            )),
            Self::Task(TaskAction::Link { task_id, parent_id }) => Ok((
                "backend-actions:task.link".into(),
                vec![
                    target("task.target", task(task_id)?),
                    m(
                        "task.parent",
                        json!({
                            "parent": parent_id.as_deref().map(&task).transpose()?.map(Value::String).unwrap_or_else(|| serde_json::to_value(m("task.independent", json!({}))).unwrap()),
                        }),
                    ),
                ],
            )),
            Self::Task(TaskAction::Color { task_id, color }) => Ok((
                "backend-actions:task.color".into(),
                vec![
                    target("task.target", task(task_id)?),
                    m(
                        "task.color.value",
                        json!({
                            "color": color.as_deref().map(color_message).map(|message| serde_json::to_value(message).unwrap()).unwrap_or_else(|| serde_json::to_value(m("task.color.none", json!({}))).unwrap()),
                            "color_code": color,
                        }),
                    ),
                ],
            )),
            Self::Task(TaskAction::Reorder {
                cycle_id, task_ids, ..
            }) => Ok((
                "backend-actions:task.reorder".into(),
                vec![
                    target("task.target", cycle(cycle_id)?),
                    m(
                        "task.reorder.order",
                        json!({"items": task_ids.iter().map(|id| task(id)).collect::<AppResult<Vec<_>>>()?.join(" → ")}),
                    ),
                ],
            )),
            Self::Reminder(ReminderAction::Create {
                target_kind,
                target_id,
                fire_at,
                respect_quiet_hours,
            }) => Ok((
                "backend-actions:reminder.create".into(),
                vec![
                    target(
                        "reminder.target",
                        if target_kind == "task" {
                            task(target_id)?
                        } else {
                            cycle(target_id)?
                        },
                    ),
                    m("reminder.fire_at", json!({"fireAt": fire_at})),
                    m(
                        "reminder.quiet_hours",
                        json!({"respect": respect_quiet_hours}),
                    ),
                ],
            )),
            Self::Reminder(ReminderAction::Update {
                reminder_id,
                fire_at,
                respect_quiet_hours,
            }) => Ok((
                "backend-actions:reminder.update".into(),
                vec![
                    target("reminder.target", reminder_label(&conn, reminder_id)?),
                    m("reminder.fire_at", json!({"fireAt": fire_at})),
                    m(
                        "reminder.quiet_hours",
                        json!({"respect": respect_quiet_hours}),
                    ),
                ],
            )),
            Self::Reminder(ReminderAction::Delete { reminder_id }) => Ok((
                "backend-actions:reminder.delete".into(),
                vec![target(
                    "reminder.target",
                    reminder_label(&conn, reminder_id)?,
                )],
            )),
            Self::Repeat(RepeatAction::Create { session_id }) => Ok((
                "backend-actions:repeat.create".into(),
                vec![target("repeat.target", cycle(session_id)?)],
            )),
            Self::Repeat(RepeatAction::Update {
                repeat_id,
                title,
                duration_minutes,
            }) => Ok((
                "backend-actions:repeat.update".into(),
                vec![
                    target(
                        "repeat.target",
                        repo::repeats::require(&conn, repeat_id)?.title,
                    ),
                    m(
                        "repeat.updated",
                        json!({"title": title, "minutes": duration_minutes}),
                    ),
                ],
            )),
            Self::Repeat(RepeatAction::Stop { repeat_id }) => Ok((
                "backend-actions:repeat.stop".into(),
                vec![
                    target(
                        "repeat.target",
                        repo::repeats::require(&conn, repeat_id)?.title,
                    ),
                    m("repeat.keep_existing", json!({})),
                ],
            )),
            Self::DayMove(change) => Ok((
                "backend-actions:day_move".into(),
                vec![
                    target("day_move.target", cycle(&change.cycle_id)?),
                    m(
                        "day_move.destination",
                        json!({
                            "date": change.target_date,
                            "strategy": serde_json::to_value(strategy(change.strategy.as_deref())).unwrap(),
                            "strategy_code": change.strategy,
                        }),
                    ),
                ],
            )),
            Self::Settings(setting) => Ok((
                "backend-actions:settings".into(),
                setting_details_structured(db, setting)?,
            )),
            Self::Prioritization(PrioritizationAction::Update { cycle_id, update }) => {
                describe_prioritization_structured(db, cycle_id, update)
            }
        }
    }
}

fn describe_prioritization_structured(
    db: &Db,
    cycle_id: &str,
    update: &prioritization::PrioritizationBreakdownUpdate,
) -> AppResult<(String, Vec<crate::i18n::LocalizedMessage>)> {
    let conn = db.pool().get()?;
    let cycle = repo::cycles::require(&conn, cycle_id)?;
    let tasks = repo::tasks::list_visible_by_cycle(&conn, cycle_id)?;
    let candidates: std::collections::HashSet<String> =
        tasks.iter().map(|task| task.id.clone()).collect();
    let existing = prioritization::load_for_cycle(&conn, cycle_id)?.unwrap_or_default();
    let merged = prioritization::merge(&existing, update, &candidates)?;
    let titles: std::collections::HashMap<String, String> = tasks
        .iter()
        .map(|task| (task.id.clone(), task.title.clone()))
        .collect();
    let bucket = |items: &[prioritization::BucketItem]| {
        if items.is_empty() {
            return serde_json::to_value(crate::i18n::LocalizedMessage::new(
                "backend-actions:prioritization.none",
                json!({}),
            ))
            .unwrap();
        }
        Value::Array(items.iter().map(|item| serde_json::to_value(crate::i18n::LocalizedMessage::new(
            "backend-actions:prioritization.item",
            json!({
                "id": item.task_id,
                "title": titles.get(&item.task_id).map(|title| Value::String(title.clone())).unwrap_or_else(|| serde_json::to_value(crate::i18n::LocalizedMessage::new("backend-actions:prioritization.unknown_task", json!({}))).unwrap()),
                "reason": item.reason,
            }),
        )).unwrap()).collect())
    };
    let m = |key: &str, args: Value| {
        crate::i18n::LocalizedMessage::new(format!("backend-actions:{key}"), args)
    };
    Ok((
        "backend-actions:prioritization.update".into(),
        vec![
            m("prioritization.cycle", json!({"plan": cycle.title})),
            m(
                "prioritization.bucket",
                json!({"name": serde_json::to_value(m("prioritization.name.big_wins", json!({}))).unwrap(), "items": bucket(&merged.big_wins)}),
            ),
            m(
                "prioritization.bucket",
                json!({"name": serde_json::to_value(m("prioritization.name.bottlenecks", json!({}))).unwrap(), "items": bucket(&merged.bottlenecks)}),
            ),
            m(
                "prioritization.bucket",
                json!({"name": serde_json::to_value(m("prioritization.name.non_negotiables", json!({}))).unwrap(), "items": bucket(&merged.non_negotiables)}),
            ),
            m(
                "prioritization.bucket",
                json!({"name": serde_json::to_value(m("prioritization.name.deprioritized", json!({}))).unwrap(), "items": bucket(&merged.deprioritized)}),
            ),
            m(
                "prioritization.pending",
                json!({"items": merged.pending_review}),
            ),
        ],
    ))
}

fn setting_details_structured(
    db: &Db,
    setting: &SettingAction,
) -> AppResult<Vec<crate::i18n::LocalizedMessage>> {
    let before = read_settings(db)?;
    let value = serde_json::to_value(setting).map_err(|e| invalid(e.to_string()))?;
    let key = value["setting"].as_str().unwrap_or("active_model");
    let m = |key: &str, args: Value| {
        crate::i18n::LocalizedMessage::new(format!("backend-actions:{key}"), args)
    };
    let theme_message = |code: &str| {
        m(
            match code {
                "white" => "settings.theme.white",
                "gray" => "settings.theme.gray",
                _ => "settings.theme.unknown",
            },
            json!({"code": code}),
        )
    };
    let display = |setting_key: &str, v: &Value| match (setting_key, v) {
        ("theme", Value::String(code)) => serde_json::to_value(theme_message(code)).unwrap(),
        (_, Value::Null) => serde_json::to_value(crate::i18n::LocalizedMessage::new(
            "backend-actions:settings.value.unset",
            json!({}),
        ))
        .unwrap(),
        (_, Value::Bool(v)) => serde_json::to_value(crate::i18n::LocalizedMessage::new(
            if *v {
                "backend-actions:settings.value.on"
            } else {
                "backend-actions:settings.value.off"
            },
            json!({}),
        ))
        .unwrap(),
        (_, Value::String(s)) => Value::String(s.clone()),
        (_, value) => Value::String(value.to_string()),
    };
    let after = match setting {
        SettingAction::QuietHours { start, end } => serde_json::to_value(crate::i18n::LocalizedMessage::new(
            "backend-actions:settings.quiet_hours.value",
            json!({
                "start": start.as_deref().map(|value| Value::String(value.to_owned())).unwrap_or_else(|| serde_json::to_value(crate::i18n::LocalizedMessage::new("backend-actions:settings.value.off", json!({}))).unwrap()),
                "end": end.as_deref().map(|value| Value::String(value.to_owned())).unwrap_or_else(|| serde_json::to_value(crate::i18n::LocalizedMessage::new("backend-actions:settings.value.off", json!({}))).unwrap()),
            }),
        )).unwrap(),
        SettingAction::ActiveModel { provider_id, model_id } => Value::String(format!("{provider_id} / {model_id}")),
        _ => display(key, &value["value"]),
    };
    let label = crate::i18n::LocalizedMessage::new(
        format!("backend-actions:settings.label.{key}"),
        json!({}),
    );
    Ok(vec![crate::i18n::LocalizedMessage::new(
        "backend-actions:settings.changed",
        json!({"label": label, "before": display(key, &before[key]), "after": after}),
    )])
}

fn reminder_label(conn: &rusqlite::Connection, id: &str) -> AppResult<String> {
    let r = repo::reminders::require(conn, id)?;
    let title = if r.target_kind.as_str() == "task" {
        repo::tasks::require(conn, &r.target_id)?.title
    } else {
        repo::cycles::require(conn, &r.target_id)?.title
    };
    Ok(format!(
        "{title} · {}",
        chrono::DateTime::from_timestamp_millis(r.fire_at)
            .map(|d| d.with_timezone(&chrono::Local).to_rfc3339())
            .unwrap_or_default()
    ))
}

pub fn read_settings(db: &Db) -> AppResult<Value> {
    let conn = db.pool().get()?;
    let settings = service::settings::get(db)?;
    let reminders = service::reminders::get_settings(db)?;
    Ok(
        json!({"locale": settings.locale, "theme": settings.theme.unwrap_or("white".into()), "week_start_day": service::settings::week_start_day_or_default(&conn)?,
        "coach_idle_minutes": repo::agent::context_idle_minutes(&conn)?,
        "plan_with_ai": service::settings::get_app_flag(db, "ui.plan-with-ai".into())?.as_deref() != Some("false"),
        "daily_capacity_minutes": service::calendar::get_daily_capacity_minutes(&conn)?,
        "daily_reminder": reminders.daily_plan_time, "quiet_hours": reminders.quiet_hours,
        "log_level": repo::settings::get(&conn, repo::settings::KEY_LOG_LEVEL)?.unwrap_or("info".into()) }),
    )
}

/// Deliberate allowlist: URLs, headers and credentials never enter the LLM context.
pub fn model_choices(ai: &AiSettingsState) -> Value {
    let active = ai.store.resolve_active_model();
    json!({"active": active.map(|(p,m)| json!({"provider_id":p.id,"provider_name":p.name,"model_id":m.model_id})),
        "available": ai.store.list().into_iter().filter(|p| !p.archived && p.connection_verified_at.is_some()).flat_map(|p| p.models.into_iter().map(move |m| json!({"provider_id":p.id,"provider_name":p.name,"model_id":m.model_id,"supports_tools":m.supports_tools}))).collect::<Vec<_>>()})
}

pub fn stage(db: &Db, source_cycle_id: &str, action: Action, rationale: &str) -> AppResult<Value> {
    if rationale.trim().is_empty() || rationale.len() > 2000 {
        return Err(invalid("A short, nonempty rationale is required"));
    }
    action.validate()?;
    let (summary_key, details) = action.describe(db)?;
    let summary = crate::i18n::render_message(
        crate::i18n::Locale::En,
        &crate::i18n::LocalizedMessage::new(summary_key.clone(), json!({})),
    );
    let encoded = serde_json::to_string(&action).map_err(|e| invalid(e.to_string()))?;
    let conn = db.pool().get()?;
    // The same still-pending intent is reused if a model repeats its call.
    repo::cycles::require(&conn, source_cycle_id)?;
    use rusqlite::OptionalExtension;
    if let Some(id) = conn.query_row("SELECT id FROM agent_actions WHERE source_cycle_id=?1 AND action_json=?2 AND state='pending'", rusqlite::params![source_cycle_id, encoded], |r| r.get::<_,String>(0)).optional().map_err(db_error)? {
        return Ok(json!({"status":"proposed", "action_id":id, "summary":summary, "summary_key":summary_key, "details":details}));
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute("INSERT INTO agent_actions (id,source_cycle_id,action_json,rationale,summary,details_json,state,created_at) VALUES (?1,?2,?3,?4,?5,?6,'pending',?7)",
        rusqlite::params![id,source_cycle_id,encoded,rationale,summary_key,serde_json::to_string(&details).map_err(|e| invalid(e.to_string()))?,service::now_ms()]).map_err(db_error)?;
    Ok(
        json!({"status":"proposed", "action_id":id,"summary":summary,"summary_key":summary_key,"details":details}),
    )
}

pub fn list(db: &Db, source_cycle_id: &str) -> AppResult<Vec<PendingAction>> {
    let conn = db.pool().get()?;
    let mut stmt = conn.prepare("SELECT id,source_cycle_id,action_json,rationale,summary,details_json,state FROM agent_actions WHERE source_cycle_id=?1 AND state IN ('pending','applying') ORDER BY created_at,id").map_err(db_error)?;
    let rows = stmt
        .query_map([source_cycle_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    rows.into_iter()
        .map(
            |(id, source_cycle_id, raw, rationale, summary, details, state)| {
                let action: Action =
                    serde_json::from_str(&raw).map_err(|e| invalid(e.to_string()))?;
                let structured_summary = summary.starts_with("backend-actions:");
                let summary_key = if structured_summary {
                    summary.clone()
                } else {
                    String::new()
                };
                let summary = if structured_summary {
                    crate::i18n::render_message(
                        crate::i18n::Locale::En,
                        &crate::i18n::LocalizedMessage::new(summary_key.clone(), json!({})),
                    )
                } else {
                    summary
                };
                let (details, legacy_details) =
                    match serde_json::from_str::<Vec<crate::i18n::LocalizedMessage>>(&details) {
                        Ok(details) => (details, !structured_summary),
                        Err(_) => {
                            let legacy = serde_json::from_str::<Vec<String>>(&details)
                                .unwrap_or_else(|_| vec![details]);
                            (
                                legacy
                                    .into_iter()
                                    .map(|value| {
                                        crate::i18n::LocalizedMessage::new(
                                            "backend-actions:literal",
                                            json!({"value": value}),
                                        )
                                    })
                                    .collect(),
                                true,
                            )
                        }
                    };
                Ok(PendingAction {
                    id,
                    source_cycle_id,
                    action,
                    rationale,
                    summary,
                    summary_key,
                    details,
                    state,
                    legacy_details,
                })
            },
        )
        .collect()
}

/// Conditional claim prevents double-click execution. If the app stops during
/// a domain write, `applying` remains visible and MUST NOT be retried blindly.
pub fn claim(db: &Db, source_cycle_id: &str, id: &str, approve: bool) -> AppResult<PendingAction> {
    let item = list(db, source_cycle_id)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::not_found("action", id))?;
    if approve {
        let (fresh_summary_key, fresh_details) = item.action.describe(db)?;
        if item.legacy_details
            || fresh_summary_key != item.summary_key
            || fresh_details != item.details
        {
            db.pool()
                .get()?
                .execute(
                    "UPDATE agent_actions SET summary=?2,details_json=?3 WHERE id=?1 AND state='pending'",
                    rusqlite::params![id, fresh_summary_key, serde_json::to_string(&fresh_details).map_err(|e| invalid(e.to_string()))?],
                )
                .map_err(db_error)?;
            return Err(AppError::conflict(
                "action_changed",
                "相关内容已有变化，已刷新操作详情，请重新核对后确认。",
            ));
        }
    }
    let state = if approve { "applying" } else { "rejected" };
    let changed = db.pool().get()?.execute("UPDATE agent_actions SET state=?2 WHERE id=?1 AND (state='pending' OR (?3=0 AND state='applying'))", rusqlite::params![id,state,approve]).map_err(db_error)?;
    if changed != 1 {
        return Err(AppError::conflict("action_already_resolved", "This action has already been handled or is being applied. Inspect the plan before retrying."));
    }
    Ok(item)
}
pub fn finish(db: &Db, id: &str, success: bool) -> AppResult<()> {
    db.pool()
        .get()?
        .execute(
            "UPDATE agent_actions SET state=?2 WHERE id=?1 AND state='applying'",
            rusqlite::params![id, if success { "applied" } else { "pending" }],
        )
        .map_err(db_error)?;
    Ok(())
}

pub async fn apply(db: &Db, ai: &AiSettingsState, action: &Action) -> AppResult<()> {
    action.validate()?;
    let now = service::now_ms();
    match action {
        Action::Cycle(change) => match change {
            CycleAction::Create {
                cycle_type,
                date,
                title,
                duration_months,
                parent_id,
            } => {
                service::cycles::create_planning_cycle(
                    db,
                    &service::cycles::CreateCycleArgs {
                        cycle_type: cycle_type.clone(),
                        date: date.clone(),
                        title: title.clone(),
                        duration_months: *duration_months,
                        parent_id: parent_id.clone(),
                    },
                    crate::domain::calendar::today_local(),
                    now,
                )?;
            }
            CycleAction::Start { cycle_id } => {
                service::cycles::start_cycle(db, cycle_id, now)?;
            }
            CycleAction::Finish { cycle_id } => {
                service::cycles::finish_cycle(db, cycle_id, now)?;
            }
            CycleAction::Delete { cycle_id } => {
                service::cycles::delete_cycle(db, cycle_id)?;
            }
            CycleAction::CopyUncompleted { cycle_id } => {
                service::cycles::copy_uncompleted_from_previous(db, cycle_id, now)?;
            }
        },
        Action::Focus(change) => match change {
            FocusAction::Create {
                day_cycle_id,
                title,
                duration_minutes,
            } => {
                service::cycles::add_session(
                    db,
                    &service::cycles::AddSessionArgs {
                        day_cycle_id: day_cycle_id.clone(),
                        title: title.clone(),
                        duration_ms: Some(minutes(*duration_minutes)?),
                        position: None,
                    },
                    now,
                )?;
            }
            FocusAction::Update {
                session_id,
                title,
                duration_minutes,
            } => {
                service::cycles::update_session(
                    db,
                    session_id,
                    title.clone(),
                    Some(minutes(*duration_minutes)?),
                )?;
            }
            FocusAction::Schedule {
                session_id,
                starts_at,
            } => {
                service::calendar::set_session_schedule(
                    db,
                    session_id,
                    starts_at.as_deref().map(timestamp).transpose()?,
                    None,
                )?;
            }
            FocusAction::Reorder {
                day_cycle_id,
                session_ids,
            } => {
                service::cycles::reorder_sessions(db, day_cycle_id, session_ids)?;
            }
        },
        Action::Task(change) => match change {
            TaskAction::Move {
                task_id,
                target_cycle_id,
            } => {
                require_no_task_preview(db, task_id)?;
                service::tasks::move_task(db, task_id, target_cycle_id, None)?;
            }
            TaskAction::Link { task_id, parent_id } => {
                require_no_task_preview(db, task_id)?;
                service::tasks::set_task_parent_link(db, task_id, parent_id.as_deref())?;
            }
            TaskAction::Color { task_id, color } => {
                require_no_task_preview(db, task_id)?;
                service::tasks::set_task_root_color(db, task_id, color.as_deref())?;
            }
            TaskAction::Reorder {
                cycle_id,
                parent_id,
                task_ids,
            } => {
                for id in task_ids {
                    require_no_task_preview(db, id)?;
                }
                service::tasks::reorder_tasks(db, cycle_id, parent_id.as_deref(), task_ids)?;
            }
        },
        Action::DayMove(change) => {
            let strategy = match change.strategy.as_deref() {
                Some("merge") => Some(service::calendar::MoveStrategy::Merge),
                Some("swap") => Some(service::calendar::MoveStrategy::Swap),
                _ => None,
            };
            service::calendar::move_day_cycle(
                db,
                &change.cycle_id,
                &change.target_date,
                strategy,
                now,
            )?;
        }
        Action::Reminder(change) => match change {
            ReminderAction::Create {
                target_kind,
                target_id,
                fire_at,
                respect_quiet_hours,
            } => {
                service::reminders::set_reminder(
                    db,
                    &service::reminders::SetReminderArgs {
                        target_kind: target_kind.clone(),
                        target_id: target_id.clone(),
                        fire_at: timestamp(fire_at)?,
                        quiet_ok: *respect_quiet_hours,
                    },
                    now,
                )?;
            }
            ReminderAction::Update {
                reminder_id,
                fire_at,
                respect_quiet_hours,
            } => {
                service::reminders::update_reminder(
                    db,
                    reminder_id,
                    &service::reminders::UpdateReminderArgs {
                        fire_at: timestamp(fire_at)?,
                        quiet_ok: Some(*respect_quiet_hours),
                    },
                )?;
            }
            ReminderAction::Delete { reminder_id } => {
                service::reminders::delete_reminder(db, reminder_id)?
            }
        },
        Action::Repeat(change) => match change {
            RepeatAction::Create { session_id } => {
                service::repeats::add_repeat(
                    db,
                    &service::repeats::AddRepeatArgs {
                        session_id: session_id.clone(),
                    },
                    now,
                )?;
            }
            RepeatAction::Update {
                repeat_id,
                title,
                duration_minutes,
            } => {
                service::repeats::update_repeat(
                    db,
                    repeat_id,
                    &service::repeats::RepeatPatch {
                        title: Some(title.clone()),
                        duration: Some(minutes(*duration_minutes)?),
                        position: None,
                    },
                )?;
            }
            RepeatAction::Stop { repeat_id } => {
                service::repeats::stop_repeat(db, repeat_id)?;
            }
        },
        Action::Settings(setting) => match setting {
            SettingAction::Locale { value } => service::settings::set_locale(db, value.clone())?,
            SettingAction::Theme { value } => service::settings::set_theme(db, value.clone())?,
            SettingAction::WeekStartDay { value } => {
                service::settings::set_week_start_day(db, *value)?
            }
            SettingAction::CoachIdleMinutes { value } => service::settings::set_app_flag(
                db,
                repo::agent::CONTEXT_IDLE_SETTING.into(),
                value.to_string(),
            )?,
            SettingAction::PlanWithAi { value } => {
                service::settings::set_app_flag(db, "ui.plan-with-ai".into(), value.to_string())?
            }
            SettingAction::DailyCapacityMinutes { value } => {
                service::calendar::set_daily_capacity_minutes(db, *value)?
            }
            SettingAction::LogLevel { value } => {
                service::settings::set_log_level(db, value.clone())?
            }
            SettingAction::DailyReminder { value } => {
                let existing = service::reminders::get_settings(db)?;
                service::reminders::set_settings(db, existing.quiet_hours, value.clone())?;
            }
            SettingAction::QuietHours { start, end } => {
                let existing = service::reminders::get_settings(db)?;
                let quiet = start.as_ref().zip(end.as_ref()).map(|(start, end)| {
                    service::reminders::QuietHours {
                        start: start.clone(),
                        end: end.clone(),
                    }
                });
                service::reminders::set_settings(db, quiet, existing.daily_plan_time)?;
            }
            SettingAction::ActiveModel {
                provider_id,
                model_id,
            } => {
                let _guard = ai.mutations.lock().await;
                crate::providers::service::activate_model(ai, provider_id, model_id)?;
            }
        },
        Action::Prioritization(PrioritizationAction::Update { cycle_id, update }) => {
            let conn = db.pool().get()?;
            let tasks = repo::tasks::list_visible_by_cycle(&conn, cycle_id)?;
            let candidates: std::collections::HashSet<String> =
                tasks.iter().map(|task| task.id.clone()).collect();
            let existing = prioritization::load_for_cycle(&conn, cycle_id)?.unwrap_or_default();
            let merged = prioritization::merge(&existing, update, &candidates)?;
            prioritization::store_for_cycle(&conn, cycle_id, &merged)?;
        }
    }
    Ok(())
}
fn require_no_task_preview(db: &Db, task_id: &str) -> AppResult<()> {
    if repo::tasks::require(&*db.pool().get()?, task_id)?
        .proposal
        .is_some()
    {
        return Err(AppError::conflict(
            "preview_conflict",
            "Resolve the pending task edit first",
        ));
    }
    Ok(())
}

pub fn resolve_task_preview(
    db: &Db,
    source_cycle_id: &str,
    task_id: &str,
    approve: bool,
) -> AppResult<service::Mutation<()>> {
    let locale = crate::i18n::for_db(db)?;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let task = repo::tasks::require(&tx, task_id)?;
    if task.proposal.is_none() {
        return Err(AppError::not_found("preview", task_id));
    }
    let original = repo::proposals::get_snapshot(&tx, task_id)?;
    let summary_key = if !approve {
        "backend-actions:preview.rejected"
    } else if task.proposal == Some(crate::domain::proposal::ProposalKind::Delete) {
        "backend-actions:preview.deleted"
    } else if original.as_ref().is_some_and(|o| {
        !o.original_exists || o.title.as_deref().is_none_or(|t| t.trim().is_empty())
    }) {
        "backend-actions:preview.added"
    } else {
        "backend-actions:preview.updated"
    };
    let status_key = if approve {
        "backend-actions:receipt.applied"
    } else {
        "backend-actions:receipt.rejected"
    };
    let summary = crate::i18n::render_message(
        locale,
        &crate::i18n::LocalizedMessage::new(summary_key, json!({})),
    );
    let details = vec![crate::i18n::LocalizedMessage::new(
        "backend-actions:preview.title",
        json!({"title": task.title}),
    )];
    let text = crate::i18n::render_message(
        locale,
        &crate::i18n::LocalizedMessage::new(
            "backend-actions:receipt.task",
            json!({"status": crate::i18n::render_message(locale, &crate::i18n::LocalizedMessage::new(status_key, json!({}))), "summary": summary, "title": task.title}),
        ),
    );
    crate::ai::agent::turn::record_decision(
        &tx,
        source_cycle_id,
        json!({"text":text,"result":{"summary_key":summary_key,"details":details,"decision":if approve {"applied"} else {"rejected"},"operation":"task_preview"},"target_kind":"task","target_id":task_id,"decision":if approve {"applied"} else {"rejected"}}),
    )?;
    if approve {
        service::proposals::keep_one(&tx, task_id)?;
    } else {
        service::proposals::undo_one(&tx, task_id)?;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(service::Mutation::new(())
        .touching_tasks(task.cycle_id.clone())
        .touching_proposals(task.cycle_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{llm::types::ToolCallRecord, tool_catalog};
    fn setup() -> (tempfile::TempDir, Db, AiSettingsState, String) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_at(&dir.path().join("test.db")).unwrap();
        let ai = AiSettingsState::load(dir.path()).unwrap();
        let day = service::cycles::get_or_create_day(
            &db,
            chrono::NaiveDate::from_ymd_opt(2026, 9, 15).unwrap(),
            1,
        )
        .unwrap()
        .value;
        (dir, db, ai, day.id)
    }
    async fn approve(db: &Db, ai: &AiSettingsState, cycle: &str, action: Action) {
        let result = stage(db, cycle, action, "用户要求").unwrap();
        let id = result["action_id"].as_str().unwrap();
        let item = claim(db, cycle, id, true).unwrap();
        assert!(
            claim(db, cycle, id, true).is_err(),
            "double approval cannot execute again"
        );
        let result = apply(db, ai, &item.action).await;
        finish(db, id, result.is_ok()).unwrap();
        result.unwrap();
        assert!(
            claim(db, cycle, id, true).is_err(),
            "applied action cannot run again"
        );
    }
    #[tokio::test]
    async fn settings_are_pending_until_confirmed_and_reject_preserves_state() {
        let (_dir, db, ai, cycle) = setup();
        let action = Action::Settings(SettingAction::CoachIdleMinutes { value: 30 });
        let staged = stage(&db, &cycle, action.clone(), "调整有效期").unwrap();
        let repeated = stage(&db, &cycle, action.clone(), "同一项").unwrap();
        assert_eq!(staged["action_id"], repeated["action_id"]);
        assert_eq!(read_settings(&db).unwrap()["coach_idle_minutes"], 15);
        claim(&db, &cycle, staged["action_id"].as_str().unwrap(), false).unwrap();
        assert_eq!(read_settings(&db).unwrap()["coach_idle_minutes"], 15);
        approve(&db, &ai, &cycle, action).await;
        assert_eq!(read_settings(&db).unwrap()["coach_idle_minutes"], 30);
        assert!(list(&db, &cycle).unwrap().is_empty());
    }

    #[tokio::test]
    async fn structured_details_are_locale_independent_for_claim_freshness() {
        let (_dir, db, _ai, cycle) = setup();
        let staged = stage(
            &db,
            &cycle,
            Action::Settings(SettingAction::Theme {
                value: "gray".into(),
            }),
            "切换主题",
        )
        .unwrap();
        assert_eq!(staged["summary_key"], "backend-actions:settings");
        assert_eq!(
            staged["details"][0]["key"],
            "backend-actions:settings.changed"
        );
        let before = list(&db, &cycle).unwrap().remove(0);
        service::settings::set_locale(&db, "zh-CN".into()).unwrap();
        let after = list(&db, &cycle).unwrap().remove(0);
        assert_eq!(before.summary_key, after.summary_key);
        assert_eq!(before.details, after.details);
        assert_ne!(
            crate::i18n::render_message(crate::i18n::Locale::En, &before.details[0]),
            crate::i18n::render_message(crate::i18n::Locale::ZhCn, &before.details[0]),
        );
        let claimed = claim(&db, &cycle, staged["action_id"].as_str().unwrap(), true).unwrap();
        assert_eq!(claimed.summary_key, "backend-actions:settings");
    }

    #[tokio::test]
    async fn legacy_string_details_refresh_once_before_approval() {
        let (_dir, db, _ai, cycle) = setup();
        let staged = stage(
            &db,
            &cycle,
            Action::Settings(SettingAction::Theme {
                value: "gray".into(),
            }),
            "切换主题",
        )
        .unwrap();
        db.pool()
            .get()
            .unwrap()
            .execute(
                "UPDATE agent_actions SET summary='修改设置',details_json='[\"旧详情\"]' WHERE id=?1",
                [staged["action_id"].as_str().unwrap()],
            )
            .unwrap();
        let err = claim(&db, &cycle, staged["action_id"].as_str().unwrap(), true).unwrap_err();
        assert!(matches!(err, AppError::Conflict { code, .. } if code == "action_changed"));
        let refreshed = list(&db, &cycle).unwrap().remove(0);
        assert_eq!(refreshed.summary_key, "backend-actions:settings");
        assert!(refreshed
            .details
            .iter()
            .all(|detail| detail.key != "旧详情"));
        assert!(claim(&db, &cycle, staged["action_id"].as_str().unwrap(), true).is_ok());
    }

    #[test]
    fn pending_actions_remain_listable_and_rejectable_after_new_target_is_deleted() {
        let (_dir, db, _ai, day) = setup();
        let task = service::tasks::add_task(
            &db,
            &service::tasks::AddTaskArgs {
                cycle_id: day.clone(),
                title: "Pending target".into(),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value;
        let staged = stage(
            &db,
            &day,
            Action::Task(TaskAction::Color {
                task_id: task.id.clone(),
                color: Some("blue".into()),
            }),
            "改色",
        )
        .unwrap();
        service::tasks::delete_task(&db, &task.id).unwrap();
        let listed = list(&db, &day).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].legacy_details);
        assert!(claim(&db, &day, staged["action_id"].as_str().unwrap(), true).is_err());
        assert!(claim(&db, &day, staged["action_id"].as_str().unwrap(), false).is_ok());
        assert!(list(&db, &day).unwrap().is_empty());
    }

    #[test]
    fn legacy_pending_actions_use_persisted_fallback_after_new_target_is_deleted() {
        let (_dir, db, _ai, day) = setup();
        let task = service::tasks::add_task(
            &db,
            &service::tasks::AddTaskArgs {
                cycle_id: day.clone(),
                title: "Legacy pending target".into(),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value;
        let staged = stage(
            &db,
            &day,
            Action::Task(TaskAction::Color {
                task_id: task.id.clone(),
                color: Some("blue".into()),
            }),
            "改色",
        )
        .unwrap();
        db.pool()
            .get()
            .unwrap()
            .execute(
                "UPDATE agent_actions SET summary='修改目标颜色',details_json='[\"旧详情\"]' WHERE id=?1",
                [staged["action_id"].as_str().unwrap()],
            )
            .unwrap();
        service::tasks::delete_task(&db, &task.id).unwrap();
        let listed = list(&db, &day).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].summary_key, "");
        assert_eq!(listed[0].summary, "修改目标颜色");
        assert!(listed[0].legacy_details);
        assert_eq!(listed[0].details[0].key, "backend-actions:literal");
        assert!(claim(&db, &day, staged["action_id"].as_str().unwrap(), true).is_err());
        assert!(claim(&db, &day, staged["action_id"].as_str().unwrap(), false).is_ok());
        assert!(list(&db, &day).unwrap().is_empty());
    }
    #[tokio::test]
    async fn focus_schedule_repeat_and_reminder_share_gui_services() {
        let (_dir, db, ai, day) = setup();
        approve(
            &db,
            &ai,
            &day,
            Action::Focus(FocusAction::Create {
                day_cycle_id: day.clone(),
                title: "Write".into(),
                duration_minutes: 25,
            }),
        )
        .await;
        let session = service::cycles::list_sessions(&db, &day).unwrap().remove(0);
        assert_eq!(session.duration, Some(25 * 60_000));
        approve(
            &db,
            &ai,
            &day,
            Action::Focus(FocusAction::Schedule {
                session_id: session.id.clone(),
                starts_at: Some("2026-09-15T14:00:00+08:00".into()),
            }),
        )
        .await;
        assert_eq!(
            service::calendar::get_calendar_range(&db, "2026-09-15", "2026-09-15")
                .unwrap()
                .days
                .into_iter()
                .find(|d| d.date == "2026-09-15")
                .unwrap()
                .sessions[0]
                .schedule
                .as_ref()
                .map(|s| s.starts_at),
            Some(timestamp("2026-09-15T14:00:00+08:00").unwrap())
        );
        approve(
            &db,
            &ai,
            &day,
            Action::Focus(FocusAction::Schedule {
                session_id: session.id.clone(),
                starts_at: None,
            }),
        )
        .await;
        let slot: Option<i64> = db
            .pool()
            .get()
            .unwrap()
            .query_row(
                "SELECT scheduled_start_at FROM cycles WHERE id = ?1",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(slot, None);
        assert_eq!(
            repo::cycles::require(&db.pool().get().unwrap(), &session.id)
                .unwrap()
                .duration,
            Some(25 * 60_000)
        );
        approve(
            &db,
            &ai,
            &day,
            Action::Repeat(RepeatAction::Create {
                session_id: session.id.clone(),
            }),
        )
        .await;
        assert_eq!(
            repo::repeats::list_active(&*db.pool().get().unwrap())
                .unwrap()
                .len(),
            1
        );
        approve(
            &db,
            &ai,
            &day,
            Action::Reminder(ReminderAction::Create {
                target_kind: "session".into(),
                target_id: session.id.clone(),
                fire_at: "2026-09-15T13:55:00+08:00".into(),
                respect_quiet_hours: true,
            }),
        )
        .await;
        let reminders = service::reminders::list_reminders(
            &db,
            Some(&day),
            repo::reminders::StatusFilter::Pending,
        )
        .unwrap();
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].target_id, session.id);
        approve(
            &db,
            &ai,
            &day,
            Action::Reminder(ReminderAction::Delete {
                reminder_id: reminders[0].id.clone(),
            }),
        )
        .await;
        assert!(service::reminders::list_reminders(
            &db,
            Some(&day),
            repo::reminders::StatusFilter::All
        )
        .unwrap()
        .is_empty());
    }
    #[tokio::test]
    async fn task_move_keeps_ids_and_same_cycle_children_but_not_linked_weekly_tasks() {
        let (_dir, db, ai, day) = setup();
        let month = |months| {
            service::cycles::create_planning_cycle(
                &db,
                &service::cycles::CreateCycleArgs {
                    cycle_type: "month".into(),
                    duration_months: Some(months),
                    ..Default::default()
                },
                chrono::NaiveDate::from_ymd_opt(2026, 9, 15).unwrap(),
                1,
            )
            .unwrap()
            .value
        };
        let a = month(1);
        let b = month(3);
        let task = service::tasks::add_task(
            &db,
            &service::tasks::AddTaskArgs {
                cycle_id: a.id.clone(),
                title: "Goal".into(),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value;
        let child = service::tasks::add_task(
            &db,
            &service::tasks::AddTaskArgs {
                cycle_id: a.id.clone(),
                parent_id: Some(task.id.clone()),
                title: "Subtask".into(),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value;
        let week = repo::cycles::require(&*db.pool().get().unwrap(), &day)
            .unwrap()
            .parent_id
            .unwrap();
        let linked = service::tasks::add_task(
            &db,
            &service::tasks::AddTaskArgs {
                cycle_id: week.clone(),
                title: "Week task".into(),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value;
        service::tasks::set_task_parent_link(&db, &linked.id, Some(&task.id)).unwrap();
        approve(
            &db,
            &ai,
            &day,
            Action::Task(TaskAction::Move {
                task_id: task.id.clone(),
                target_cycle_id: b.id.clone(),
            }),
        )
        .await;
        let conn = db.pool().get().unwrap();
        assert_eq!(
            repo::tasks::require(&conn, &task.id).unwrap().cycle_id,
            b.id
        );
        assert_eq!(
            repo::tasks::require(&conn, &child.id).unwrap().cycle_id,
            b.id
        );
        assert_eq!(
            repo::tasks::require(&conn, &linked.id).unwrap().cycle_id,
            week
        );
    }
    #[test]
    fn strict_action_shapes_reject_ambiguous_nulls_secrets_and_invalid_units() {
        let (_dir, db, _ai, day) = setup();
        for (name, change) in [
            (
                "propose_settings",
                json!({"setting":"coach_idle_minutes","value":0}),
            ),
            (
                "propose_settings",
                json!({"setting":"theme","value":"white","api_key":"do-not-store"}),
            ),
            ("propose_settings", json!({"setting":"daily_reminder"})),
            (
                "propose_task_organization",
                json!({"operation":"link","task_id":"x"}),
            ),
            (
                "propose_focus_block",
                json!({"operation":"schedule","session_id":"x","starts_at":"2026-09-15 14:00"}),
            ),
            (
                "propose_focus_block",
                json!({"operation":"create","day_cycle_id":day,"title":"Work","duration_minutes":25.5}),
            ),
        ] {
            let call = ToolCallRecord {
                id: "test".into(),
                name: name.into(),
                arguments: json!({"change":change,"rationale":"user request"}),
            };
            assert!(
                tool_catalog::execute(&db, &day, &call).is_err(),
                "{name}: {}",
                call.arguments
            );
        }
        assert!(list(&db, &day).unwrap().is_empty());
    }
    #[test]
    fn changed_state_requires_a_fresh_review_and_inflight_is_not_retried() {
        let (_dir, db, _ai, day) = setup();
        let result = stage(
            &db,
            &day,
            Action::Settings(SettingAction::Theme {
                value: "gray".into(),
            }),
            "更换主题",
        )
        .unwrap();
        service::settings::set_theme(&db, "gray".into()).unwrap();
        let id = result["action_id"].as_str().unwrap();
        assert!(claim(&db, &day, id, true).is_err());
        assert!(crate::i18n::render_message(
            crate::i18n::Locale::En,
            &list(&db, &day).unwrap()[0].details[0],
        )
        .contains("gray → gray"));
        claim(&db, &day, id, true).unwrap();
        assert!(claim(&db, &day, id, true).is_err());
        claim(&db, &day, id, false).unwrap(); // GUI acknowledgement after inspecting an interrupted write.
        assert!(list(&db, &day).unwrap().is_empty());
    }
}
