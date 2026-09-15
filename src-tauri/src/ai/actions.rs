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
    pub details: Vec<String>,
    pub state: String,
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
                if let Some(start) = starts_at { timestamp(start)?; }
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
    fn describe(&self, db: &Db) -> AppResult<(String, Vec<String>)> {
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
        let (summary, details) = match self {
            Self::Cycle(CycleAction::Create {
                cycle_type,
                date,
                title,
                duration_months,
                parent_id,
            }) => (
                "创建计划周期",
                vec![
                    format!(
                        "类型：{cycle_type}；名称：{}",
                        title.as_deref().unwrap_or("按日期命名")
                    ),
                    format!(
                        "日期：{}；长期周期：{}",
                        date.as_deref().unwrap_or("今天"),
                        duration_months
                            .map(|n| format!("{n} × 28 天"))
                            .unwrap_or("—".into())
                    ),
                    format!(
                        "上级周期：{}",
                        parent_id
                            .as_deref()
                            .map(&cycle)
                            .transpose()?
                            .unwrap_or("无".into())
                    ),
                ],
            ),
            Self::Cycle(change) => {
                let (label, id) = match change {
                    CycleAction::Start { cycle_id } => ("开始周期 / 专注", cycle_id),
                    CycleAction::Finish { cycle_id } => ("结束周期 / 专注", cycle_id),
                    CycleAction::Delete { cycle_id } => ("删除周期及其内容", cycle_id),
                    CycleAction::CopyUncompleted { cycle_id } => {
                        ("承接上一周期未完成事务", cycle_id)
                    }
                    _ => unreachable!(),
                };
                let mut d = vec![cycle(id)?];
                if matches!(change, CycleAction::Delete { .. }) {
                    let impact = service::cycles::get_cycle_deletion_preview(db, id)?;
                    if let Some(code) = impact.guard_code {
                        return Err(AppError::validation(
                            code,
                            impact.guard_message.unwrap_or_default(),
                        ));
                    }
                    d.push(format!(
                        "将删除 {} 个下级周期、{} 项事务。",
                        impact.descendant_cycles, impact.tasks
                    ));
                }
                (label, d)
            }
            Self::Focus(FocusAction::Create {
                day_cycle_id,
                title,
                duration_minutes,
            }) => (
                "添加专注块",
                vec![
                    cycle(day_cycle_id)?,
                    format!("{title} · {duration_minutes} 分钟"),
                ],
            ),
            Self::Focus(FocusAction::Update {
                session_id,
                title,
                duration_minutes,
            }) => (
                "修改专注块",
                vec![
                    cycle(session_id)?,
                    format!("改为 {title} · {duration_minutes} 分钟"),
                ],
            ),
            Self::Focus(FocusAction::Schedule {
                session_id,
                starts_at,
            }) => (
                if starts_at.is_some() { "安排专注时间" } else { "移回未安排" },
                vec![cycle(session_id)?, starts_at.clone().unwrap_or("保留专注块及原有时长".into())],
            ),
            Self::Focus(FocusAction::Reorder {
                day_cycle_id,
                session_ids,
            }) => (
                "调整专注块顺序",
                vec![
                    cycle(day_cycle_id)?,
                    session_ids
                        .iter()
                        .map(|id| cycle(id))
                        .collect::<AppResult<Vec<_>>>()?
                        .join(" → "),
                ],
            ),
            Self::Task(TaskAction::Move {
                task_id,
                target_cycle_id,
            }) => (
                "移动事务及子任务",
                vec![task(task_id)?, format!("目标：{}", cycle(target_cycle_id)?)],
            ),
            Self::Task(TaskAction::Link { task_id, parent_id }) => (
                "调整事务归属",
                vec![
                    task(task_id)?,
                    format!(
                        "归属：{}",
                        parent_id
                            .as_deref()
                            .map(&task)
                            .transpose()?
                            .unwrap_or("独立事务".into())
                    ),
                ],
            ),
            Self::Task(TaskAction::Color { task_id, color }) => (
                "修改目标颜色",
                vec![task(task_id)?, color.clone().unwrap_or("无颜色".into())],
            ),
            Self::Task(TaskAction::Reorder {
                cycle_id, task_ids, ..
            }) => (
                "调整事务顺序",
                vec![
                    cycle(cycle_id)?,
                    task_ids
                        .iter()
                        .map(|id| task(id))
                        .collect::<AppResult<Vec<_>>>()?
                        .join(" → "),
                ],
            ),
            Self::Reminder(ReminderAction::Create {
                target_kind,
                target_id,
                fire_at,
                respect_quiet_hours,
            }) => (
                "添加提醒",
                vec![
                    if target_kind == "task" {
                        task(target_id)?
                    } else {
                        cycle(target_id)?
                    },
                    fire_at.clone(),
                    format!("遵循免打扰：{respect_quiet_hours}"),
                ],
            ),
            Self::Reminder(ReminderAction::Update {
                reminder_id,
                fire_at,
                respect_quiet_hours,
            }) => (
                "修改提醒",
                vec![
                    reminder_label(&conn, reminder_id)?,
                    fire_at.clone(),
                    format!("遵循免打扰：{respect_quiet_hours}"),
                ],
            ),
            Self::Reminder(ReminderAction::Delete { reminder_id }) => {
                ("删除提醒", vec![reminder_label(&conn, reminder_id)?])
            }
            Self::Repeat(RepeatAction::Create { session_id }) => {
                ("每日重复此专注块", vec![cycle(session_id)?])
            }
            Self::Repeat(RepeatAction::Update {
                repeat_id,
                title,
                duration_minutes,
            }) => (
                "修改重复模板",
                vec![
                    repo::repeats::require(&conn, repeat_id)?.title,
                    format!("{title} · {duration_minutes} 分钟；仅影响未来实例"),
                ],
            ),
            Self::Repeat(RepeatAction::Stop { repeat_id }) => (
                "停止每日重复",
                vec![
                    repo::repeats::require(&conn, repeat_id)?.title,
                    "已有专注块保留".into(),
                ],
            ),
            Self::DayMove(change) => (
                "移动日计划",
                vec![
                    cycle(&change.cycle_id)?,
                    format!(
                        "目标：{}；冲突处理：{}",
                        change.target_date,
                        change.strategy.as_deref().unwrap_or("遇到已有计划时停止")
                    ),
                ],
            ),
            Self::Settings(setting) => ("修改设置", setting_details(db, setting)?),
            Self::Prioritization(PrioritizationAction::Update { cycle_id, update }) => {
                describe_prioritization(db, cycle_id, update)?
            }
        };
        Ok((summary.into(), details))
    }
}

fn describe_prioritization(
    db: &Db,
    cycle_id: &str,
    update: &prioritization::PrioritizationBreakdownUpdate,
) -> AppResult<(&'static str, Vec<String>)> {
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
    let bucket = |name: &str, items: &[prioritization::BucketItem]| {
        if items.is_empty() {
            return format!("{name}：无");
        }
        let values = items
            .iter()
            .map(|item| {
                format!(
                    "{} · {}：{}",
                    item.task_id,
                    titles.get(&item.task_id).map(String::as_str).unwrap_or("未知事务"),
                    item.reason
                )
            })
            .collect::<Vec<_>>()
            .join("；");
        format!("{name}：{values}")
    };
    Ok((
        "更新优先级排序",
        vec![
            format!("计划：{}", cycle.title),
            bucket("大胜", &merged.big_wins),
            bucket("瓶颈", &merged.bottlenecks),
            bucket("必须做", &merged.non_negotiables),
            bucket("暂缓", &merged.deprioritized),
            format!("待排序：{}", merged.pending_review.join("、")),
        ],
    ))
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

fn setting_details(db: &Db, setting: &SettingAction) -> AppResult<Vec<String>> {
    let before = read_settings(db)?;
    let value = serde_json::to_value(setting).map_err(|e| invalid(e.to_string()))?;
    let key = value["setting"].as_str().unwrap();
    let label = match key {
        "theme" => "主题",
        "week_start_day" => "每周起始日",
        "coach_idle_minutes" => "Coach 上下文有效期（分钟）",
        "plan_with_ai" => "Plan with AI 入口",
        "daily_capacity_minutes" => "每天可用时间（分钟）",
        "daily_reminder" => "每日计划提醒",
        "quiet_hours" => "免打扰时段",
        "log_level" => "日志级别",
        _ => "当前 AI 模型",
    };
    let display = |v: &Value| match v {
        Value::Null => "关闭 / 未设置".to_string(),
        Value::Bool(v) => if *v { "开启" } else { "关闭" }.into(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    };
    let after = match setting {
        SettingAction::QuietHours { start, end } => format!(
            "{} – {}",
            start.as_deref().unwrap_or("关闭"),
            end.as_deref().unwrap_or("关闭")
        ),
        SettingAction::ActiveModel {
            provider_id,
            model_id,
        } => format!("{provider_id} / {model_id}"),
        _ => display(&value["value"]),
    };
    Ok(vec![
        label.into(),
        format!("{} → {after}", display(&before[key])),
    ])
}

pub fn read_settings(db: &Db) -> AppResult<Value> {
    let conn = db.pool().get()?;
    let settings = service::settings::get(db)?;
    let reminders = service::reminders::get_settings(db)?;
    Ok(
        json!({"theme": settings.theme.unwrap_or("white".into()), "week_start_day": service::settings::week_start_day_or_default(&conn)?,
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
    let (summary, details) = action.describe(db)?;
    let encoded = serde_json::to_string(&action).map_err(|e| invalid(e.to_string()))?;
    let conn = db.pool().get()?;
    // The same still-pending intent is reused if a model repeats its call.
    repo::cycles::require(&conn, source_cycle_id)?;
    use rusqlite::OptionalExtension;
    if let Some(id) = conn.query_row("SELECT id FROM agent_actions WHERE source_cycle_id=?1 AND action_json=?2 AND state='pending'", rusqlite::params![source_cycle_id, encoded], |r| r.get::<_,String>(0)).optional().map_err(db_error)? {
        return Ok(json!({"status":"proposed", "action_id":id, "summary":summary}));
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute("INSERT INTO agent_actions (id,source_cycle_id,action_json,rationale,summary,details_json,state,created_at) VALUES (?1,?2,?3,?4,?5,?6,'pending',?7)",
        rusqlite::params![id,source_cycle_id,encoded,rationale,summary,serde_json::to_string(&details).unwrap(),service::now_ms()]).map_err(db_error)?;
    Ok(json!({"status":"proposed", "action_id":id,"summary":summary}))
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
                Ok(PendingAction {
                    id,
                    source_cycle_id,
                    action: serde_json::from_str(&raw).map_err(|e| invalid(e.to_string()))?,
                    rationale,
                    summary,
                    details: serde_json::from_str(&details).map_err(|e| invalid(e.to_string()))?,
                    state,
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
        let (_, fresh_details) = item.action.describe(db)?;
        if fresh_details != item.details {
            db.pool()
                .get()?
                .execute(
                    "UPDATE agent_actions SET details_json=?2 WHERE id=?1 AND state='pending'",
                    rusqlite::params![id, serde_json::to_string(&fresh_details).unwrap()],
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
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let task = repo::tasks::require(&tx, task_id)?;
    if task.proposal.is_none() {
        return Err(AppError::not_found("preview", task_id));
    }
    let original=repo::proposals::get_snapshot(&tx,task_id)?;
    let prefix=if !approve { "已放弃改动" } else if task.proposal==Some(crate::domain::proposal::ProposalKind::Delete) { "已删除任务" } else if original.as_ref().is_some_and(|o| !o.original_exists || o.title.as_deref().is_none_or(|t|t.trim().is_empty())) { "已添加任务" } else { "已更新任务" };
    let text=format!("{prefix}「{}」。",task.title);
    crate::ai::agent::turn::record_decision(&tx,source_cycle_id,json!({"text":text,"target_kind":"task","target_id":task_id,"decision":if approve {"applied"} else {"rejected"}}))?;
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
            &db, &ai, &day,
            Action::Focus(FocusAction::Schedule { session_id: session.id.clone(), starts_at: None }),
        ).await;
        let slot: Option<i64> = db.pool().get().unwrap().query_row(
            "SELECT scheduled_start_at FROM cycles WHERE id = ?1", [&session.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(slot, None);
        assert_eq!(repo::cycles::require(&db.pool().get().unwrap(), &session.id).unwrap().duration, Some(25 * 60_000));
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
        assert!(list(&db, &day).unwrap()[0].details[1].contains("gray → gray"));
        claim(&db, &day, id, true).unwrap();
        assert!(claim(&db, &day, id, true).is_err());
        claim(&db, &day, id, false).unwrap(); // GUI acknowledgement after inspecting an interrupted write.
        assert!(list(&db, &day).unwrap().is_empty());
    }
}
