//! Domain-sized tools, rather than an IPC mirror or an unrestricted dispatcher.
use crate::{
    ai::{
        actions::{self, Action},
        llm::types::{AgentSkill, ToolCallRecord, ToolDef},
    },
    db::Db,
    error::{AppError, AppResult},
    repository as repo, service,
};
use serde::Deserialize;
use serde_json::{json, Value};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn text() -> Value {
    json!({"type":"string","minLength":1,"maxLength":2000})
}
fn optional_text() -> Value {
    json!({"type":["string","null"],"minLength":1})
}
fn optional_date() -> Value {
    json!({"oneOf":[{"type":"string","pattern":"^\\d{4}-\\d{2}-\\d{2}$"},{"type":"null"}]})
}
fn progress_check() -> Value {
    json!({"oneOf":[
        {"type":"null"},
        {"type":"object","properties":{"kind":{"const":"once"},"date":{"type":"string","pattern":"^\\d{4}-\\d{2}-\\d{2}$"}},"required":["kind","date"],"additionalProperties":false},
        {"type":"object","properties":{"kind":{"const":"repeat"},"every_days":{"type":"integer","minimum":1}},"required":["kind","every_days"],"additionalProperties":false}
    ]})
}
fn choice(options: &[&str]) -> Value {
    json!({"type":"string","enum":options})
}
fn ids() -> Value {
    json!({"type":"array","minItems":1,"maxItems":200,"uniqueItems":true,"items":{"type":"string","minLength":1}})
}
fn duration() -> Value {
    json!({"type":"integer","minimum":1,"maximum":1440,"description":"Whole minutes, not milliseconds"})
}
fn variant(tag: &str, value: &str, mut properties: Value, required: &[&str]) -> Value {
    properties[tag] = json!({"type":"string","enum":[value]});
    let mut fields = vec![tag];
    fields.extend(required);
    object(properties, &fields)
}
fn operation(value: &str, properties: Value, required: &[&str]) -> Value {
    variant("operation", value, properties, required)
}
fn proposal(name: &str, description: &str, variants: Vec<Value>) -> ToolDef {
    ToolDef::new(name,format!("{description} Stages one reviewable action; only the user's GUI approval executes it. Return proposed, never claim applied. Read IDs first; do not repeat pending actions."),object(json!({"change":{"oneOf":variants},"rationale":text()}),&["change","rationale"]))
}

pub fn definitions(skill: AgentSkill) -> Vec<ToolDef> {
    let mut defs = vec![
        ToolDef::new("list_cycles","Find existing cycle IDs before reading or writing. No side effects. Without dates returns active cycles; a date range includes finished overlapping cycles. At most 100 results; narrow a truncated range.",object(json!({"start_date":optional_text(),"end_date":optional_text(),"cycle_type":json!({"enum":["month","week","day",null]})}),&[])),
        ToolDef::new("get_pending_changes","Read pending task previews and app actions for this conversation. No approval/accept tool is available to the model.",object(json!({}),&[])),
    ];
    if skill == AgentSkill::None {
        defs.push(ToolDef::new("get_settings","Read user preferences and saved, verified provider/model choices. Credentials, endpoints and headers are excluded. To change a setting, propose_settings then wait for GUI approval.",object(json!({}),&[])));
        defs.push(proposal("propose_settings","Change one preference or activate one previously verified provider/model pair. Null explicitly disables the nullable setting; never enter API keys here.",vec![
            variant("setting","locale",json!({"value":choice(&["en","zh-CN"])}),&["value"]),
            variant("setting","theme",json!({"value":choice(&["white","gray"])}),&["value"]),
            variant("setting","week_start_day",json!({"value":{"type":"integer","minimum":1,"maximum":7,"description":"1=Monday … 7=Sunday"}}),&["value"]),
            variant("setting","coach_idle_minutes",json!({"value":duration()}),&["value"]),
            variant("setting","plan_with_ai",json!({"value":{"type":"boolean"}}),&["value"]),
            variant("setting","daily_capacity_minutes",json!({"value":{"type":["integer","null"],"minimum":1,"maximum":1440}}),&["value"]),
            variant("setting","daily_reminder",json!({"value":{"type":["string","null"],"description":"Local HH:MM or null to disable"}}),&["value"]),
            variant("setting","quiet_hours",json!({"start":optional_text(),"end":optional_text()}),&["start","end"]),
            variant("setting","log_level",json!({"value":choice(&["error","warn","info","debug"])}),&["value"]),
            variant("setting","active_model",json!({"provider_id":text(),"model_id":text()}),&["provider_id","model_id"]),
        ]));
        return defs;
    }
    if skill == AgentSkill::PlanningIssues {
        return defs;
    }
    defs.push(ToolDef::new("get_calendar","Read day plans, task content, focus schedules and capacity in an exact local date range (maximum 62 days). End date is inclusive; calendar may include padding days.",object(json!({"start_date":text(),"end_date":text()}),&["start_date","end_date"])));
    if skill == AgentSkill::PeriodAnalysis {
        return defs;
    }
    defs.extend([
        ToolDef::new("list_reminders","List reminders, optionally scoped to a cycle subtree. Returns at most 100; narrow the cycle if truncated.",object(json!({"cycle_id":optional_text(),"status":choice(&["pending","fired","all"])}),&["status"])),
        ToolDef::new("list_repeats","List active daily focus-block templates and IDs. Templates affect future instances.",object(json!({}),&[])),
        proposal("propose_cycle","Create a planning container, start/finish/delete a cycle or focus block, or carry unfinished tasks forward. A weekly container holds multiple weekly tasks. Long-term cycles support 1/3/6 product months or explicit starts_on/ends_on bounds, plus an optional once or repeat progress_check. Plan durations are fixed after creation. Deletion includes descendants; inspect the impact shown in the approval card.",vec![
            operation("create",json!({"cycle_type":choice(&["month","week","day"]),"date":optional_text(),"title":optional_text(),"duration_months":{"enum":[1,3,6,null]},"parent_id":optional_text(),"starts_on":optional_date(),"ends_on":optional_date(),"progress_check":progress_check()}),&["cycle_type"]),
            operation("start",json!({"cycle_id":text()}),&["cycle_id"]),
            operation("finish",json!({"cycle_id":text()}),&["cycle_id"]),
            operation("delete",json!({"cycle_id":text()}),&["cycle_id"]),
            operation("copy_uncompleted",json!({"cycle_id":text()}),&["cycle_id"]),
        ]),
        proposal("propose_focus_block","Create, edit, schedule or reorder focus blocks inside a day. Use get_calendar for day/session IDs. starts_at requires RFC3339 with timezone; null removes the time slot and keeps the block and its duration. Lifecycle/deletion use propose_cycle.",vec![
            operation("create",json!({"day_cycle_id":text(),"title":text(),"duration_minutes":duration()}),&["day_cycle_id","title","duration_minutes"]),
            operation("update",json!({"session_id":text(),"title":text(),"duration_minutes":duration()}),&["session_id","title","duration_minutes"]),
            operation("schedule",json!({"session_id":text(),"starts_at":optional_text()}),&["session_id","starts_at"]),
            operation("reorder",json!({"day_cycle_id":text(),"session_ids":ids()}),&["day_cycle_id","session_ids"]),
        ]),
        proposal("propose_task_organization","Move a task including its children, set/clear its parent, assign a long-term root color, or reorder siblings. Later is a normal target from list_cycles. Parent links can be same-cycle nesting or weekly→long-term / daily→weekly. For link, null parent means independent. For reorder, null parent selects visible roots including tasks linked to goals in other cycles; a non-null parent must be in the same cycle. Reordering preserves links. Null color clears. Resolve existing task previews first.",vec![
            operation("move",json!({"task_id":text(),"target_cycle_id":text()}),&["task_id","target_cycle_id"]),
            operation("link",json!({"task_id":text(),"parent_id":optional_text()}),&["task_id","parent_id"]),
            operation("color",json!({"task_id":text(),"color":{"enum":["red","amber","gold","green","teal","blue","indigo","plum",null]}}),&["task_id","color"]),
            operation("reorder",json!({"cycle_id":text(),"parent_id":optional_text(),"task_ids":ids()}),&["cycle_id","parent_id","task_ids"]),
        ]),
        ToolDef::new("propose_day_move","Propose moving a day plan, its tasks and focus blocks to YYYY-MM-DD. strategy null refuses an occupied date; merge or swap require an explicit user intent. No writes until GUI approval.",object(json!({"change":object(json!({"cycle_id":text(),"target_date":text(),"strategy":{"enum":["merge","swap",null]}}),&["cycle_id","target_date","strategy"]),"rationale":text()}),&["change","rationale"])),
        proposal("propose_reminder","Create, edit or delete a reminder. Use RFC3339 with timezone for fire_at. respect_quiet_hours=true allows muting during quiet hours. OS notification permission is managed in GUI.",vec![
            operation("create",json!({"target_kind":choice(&["task","session","day","cycle"]),"target_id":text(),"fire_at":text(),"respect_quiet_hours":{"type":"boolean"}}),&["target_kind","target_id","fire_at","respect_quiet_hours"]),
            operation("update",json!({"reminder_id":text(),"fire_at":text(),"respect_quiet_hours":{"type":"boolean"}}),&["reminder_id","fire_at","respect_quiet_hours"]),
            operation("delete",json!({"reminder_id":text()}),&["reminder_id"]),
        ]),
        proposal("propose_repeat","Save a focus block as a daily repeat, edit a future-instance template, or stop repeating while keeping existing sessions.",vec![
            operation("create",json!({"session_id":text()}),&["session_id"]),
            operation("update",json!({"repeat_id":text(),"title":text(),"duration_minutes":duration()}),&["repeat_id","title","duration_minutes"]),
            operation("stop",json!({"repeat_id":text()}),&["repeat_id"]),
        ]),
    ]);
    defs
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalArgs<T> {
    change: T,
    rationale: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CycleQuery {
    start_date: Option<String>,
    end_date: Option<String>,
    cycle_type: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarQuery {
    start_date: String,
    end_date: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReminderQuery {
    cycle_id: Option<String>,
    status: String,
}
fn parse<T: serde::de::DeserializeOwned>(call: &ToolCallRecord) -> AppResult<T> {
    serde_json::from_value(call.arguments.clone())
        .map_err(|e| AppError::validation("invalid_arguments", e.to_string()))
}
fn encode(value: impl serde::Serialize) -> AppResult<Value> {
    serde_json::to_value(value).map_err(|e| AppError::Internal(e.to_string()))
}

pub fn execute(db: &Db, cycle_id: &str, call: &ToolCallRecord) -> AppResult<Value> {
    macro_rules! stage {
        ($kind:ident) => {{
            let args: ProposalArgs<_> = parse(call)?;
            actions::stage(db, cycle_id, Action::$kind(args.change), &args.rationale)
        }};
    }
    match call.name.as_str() {
        "propose_cycle" => stage!(Cycle),
        "propose_focus_block" => stage!(Focus),
        "propose_task_organization" => stage!(Task),
        "propose_day_move" => stage!(DayMove),
        "propose_reminder" => stage!(Reminder),
        "propose_repeat" => stage!(Repeat),
        "propose_settings" => stage!(Settings),
        "get_settings" => {
            let _: Empty = parse(call)?;
            actions::read_settings(db)
        }
        "get_pending_changes" => {
            let _: Empty = parse(call)?;
            Ok(
                json!({"actions":actions::list(db,cycle_id)?,"tasks":service::proposals::get_preview_summary(db,cycle_id)?}),
            )
        }
        "list_cycles" => {
            let args: CycleQuery = parse(call)?;
            let conn = db.pool().get()?;
            let cycles = match (&args.start_date, &args.end_date) {
                (Some(a), Some(b)) => {
                    validate_range(a, b, 3660)?;
                    repo::cycles::list_planning_cycles_overlapping(&conn, a, b)?
                }
                (None, None) => repo::cycles::list_planner_cycles(&conn)?
                    .into_iter()
                    .filter(|c| !c.finished)
                    .collect(),
                _ => {
                    return Err(AppError::validation(
                        "invalid_arguments",
                        "Provide both date boundaries or neither",
                    ))
                }
            };
            if args
                .cycle_type
                .as_deref()
                .is_some_and(|t| !["month", "week", "day"].contains(&t))
            {
                return Err(AppError::validation(
                    "invalid_arguments",
                    "Unknown cycle type",
                ));
            }
            let cycles: Vec<_> = cycles
                .into_iter()
                .filter(|c| {
                    args.cycle_type
                        .as_deref()
                        .is_none_or(|t| c.cycle_type.as_str() == t)
                })
                .collect();
            Ok(
                json!({"total":cycles.len(),"truncated":cycles.len()>100,"cycles":cycles.into_iter().take(100).collect::<Vec<_>>()}),
            )
        }
        "get_calendar" => {
            let args: CalendarQuery = parse(call)?;
            validate_range(&args.start_date, &args.end_date, 62)?;
            let range =
                service::calendar::get_calendar_range(db, &args.start_date, &args.end_date)?;
            let mut result = encode(&range)?;
            let conn = db.pool().get()?;
            for (i, day) in range.days.iter().enumerate() {
                if let Some(cycle) = &day.day_cycle {
                    let tasks = repo::tasks::list_with_proposals_by_cycle(&conn, &cycle.id)?;
                    result["days"][i]["tasks"] = json!({"total":tasks.len(),"truncated":tasks.len()>100,"items":tasks.into_iter().take(100).collect::<Vec<_>>()});
                }
                result["days"][i]["budget"] =
                    encode(service::calendar::get_time_budget(db, &day.date)?)?;
            }
            Ok(result)
        }
        "list_reminders" => {
            let args: ReminderQuery = parse(call)?;
            let status = match args.status.as_str() {
                "pending" => repo::reminders::StatusFilter::Pending,
                "fired" => repo::reminders::StatusFilter::Fired,
                "all" => repo::reminders::StatusFilter::All,
                _ => {
                    return Err(AppError::validation(
                        "invalid_arguments",
                        "Unknown reminder status",
                    ))
                }
            };
            let rows = service::reminders::list_reminders(db, args.cycle_id.as_deref(), status)?;
            Ok(
                json!({"total":rows.len(),"truncated":rows.len()>100,"reminders":rows.into_iter().take(100).collect::<Vec<_>>()}),
            )
        }
        "list_repeats" => {
            let _: Empty = parse(call)?;
            let rows = repo::repeats::list_active(&*db.pool().get()?)?;
            Ok(
                json!({"total":rows.len(),"truncated":rows.len()>100,"repeats":rows.into_iter().take(100).collect::<Vec<_>>()}),
            )
        }
        _ => Err(AppError::validation(
            "unknown_tool",
            format!("Unknown tool {}", call.name),
        )),
    }
}
fn validate_range(a: &str, b: &str, max_days: i64) -> AppResult<()> {
    let parse = |v: &str| {
        chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| AppError::validation("invalid_arguments", "Date must be YYYY-MM-DD"))
    };
    let days = (parse(b)? - parse(a)?).num_days();
    if days < 0 || days >= max_days {
        return Err(AppError::validation(
            "invalid_arguments",
            format!("Range must contain 1–{max_days} days"),
        ));
    }
    Ok(())
}
