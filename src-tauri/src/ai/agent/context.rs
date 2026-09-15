//! Context injection (add-ai-planning-core §4): the machine-readable
//! `<context>` block appended to the system instruction each turn.
//!
//! Layered by importance (design D4) so trimming is a pure function over a
//! built payload: essential blocks never drop, important block drops second,
//! optional blocks drop first. Rendering itself is deterministic text.

use std::collections::HashMap;

use chrono::Local;
use rusqlite::Connection;

use crate::domain::calendar;
use crate::domain::cycle::Cycle;
use crate::domain::cycle::CycleType;
use crate::domain::task::Task;
use crate::error::AppResult;
use crate::repository::cycles as cycles_repo;
use crate::repository::tasks as tasks_repo;

use super::prompt_xml::escape_xml;

/// Injection priorities; lower drops first. Design D4's table, in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Parent/child cycle keys and history extras — dropped first.
    Optional,
    /// The cycle's task list (titles + flags).
    Important,
    /// Cycle metadata, the focused task snapshot, missing fields.
    Essential,
}

/// One built block, ready to render or drop by budget.
#[derive(Debug, Clone)]
pub struct ContextBlock {
    pub priority: Priority,
    pub xml: String,
}

/// Everything the builder needs, already loaded — keeps `render` pure and
/// testable without a database. Tests build it with `serde_json` helpers.
#[derive(Debug)]
pub struct CycleContext {
    pub cycle: Cycle,
    pub parent: Option<Cycle>,
    /// Direct child planning cycles (weeks under the month, days under the
    /// week), each with a lifecycle word (ended/current/future).
    pub children: Vec<(Cycle, &'static str)>,
    /// Visible tasks of the cycle, position-ordered.
    pub tasks: Vec<Task>,
    /// Titles for cross-references (e.g. link targets), id → title.
    pub referenced_titles: HashMap<String, String>,
    /// When the focused task is a single-goal clarification, its snapshot.
    pub focused_task: Option<Task>,
}

/// Budget in approximate characters for the whole context block; `None` =
/// unbounded (tests and small cycles). Trimming drops whole blocks by
/// priority, never mid-XML — a truncated tag would confuse the model more
/// than a missing section.
pub fn render(context: &CycleContext, budget: Option<usize>) -> String {
    let mut blocks = vec![
        ContextBlock {
            priority: Priority::Essential,
            xml: cycle_block(&context.cycle, context.parent.as_ref()),
        },
        ContextBlock {
            priority: Priority::Essential,
            xml: time_block(),
        },
    ];
    if let Some(task) = &context.focused_task {
        blocks.push(ContextBlock {
            priority: Priority::Essential,
            xml: task_snapshot_block(task),
        });
    }
    if !context.tasks.is_empty() {
        blocks.push(ContextBlock {
            priority: Priority::Important,
            xml: tasks_block(&context.tasks, &context.referenced_titles),
        });
    }
    if !context.children.is_empty() {
        blocks.push(ContextBlock {
            priority: Priority::Optional,
            xml: children_block(&context.children),
        });
    }

    // Budget trim: drop whole blocks, lowest priority first — a truncated
    // tag would confuse the model more than a missing section.
    if let Some(limit) = budget {
        for drop_priority in [Priority::Optional, Priority::Important] {
            if blocks.iter().map(|b| b.xml.len()).sum::<usize>() <= limit {
                break;
            }
            blocks.retain(|b| b.priority != drop_priority);
        }
    }

    let mut out = String::from("<context>\n");
    for block in blocks {
        out.push_str(&block.xml);
        out.push('\n');
    }
    out.push_str("</context>");
    out
}

/// Loads everything the renderer needs for one cycle from the database.
pub fn load_context(conn: &Connection, cycle_id: &str) -> AppResult<CycleContext> {
    let cycle = cycles_repo::require(conn, cycle_id)?;
    let parent = match &cycle.parent_id {
        Some(id) => cycles_repo::get(conn, id)?,
        None => None,
    };
    let children = match cycle.cycle_type {
        CycleType::Month => cycles_repo::list_children(conn, cycle_id)?,
        CycleType::Week => cycles_repo::list_children(conn, cycle_id)?,
        _ => Vec::new(),
    };
    let today = calendar::today_local();
    let children = children
        .into_iter()
        .map(|child| {
            let word = lifecycle_word(&child, today);
            (child, word)
        })
        .collect();
    let tasks = tasks_repo::list_visible_by_cycle(conn, cycle_id)?;
    Ok(CycleContext {
        cycle,
        parent,
        children,
        tasks,
        referenced_titles: HashMap::new(),
        focused_task: None,
    })
}

fn lifecycle_word(cycle: &Cycle, today: chrono::NaiveDate) -> &'static str {
    if cycle.finished {
        "ended"
    } else if cycle
        .starts_on
        .as_deref()
        .and_then(calendar::parse_date)
        .is_some_and(|start| start > today)
    {
        "future"
    } else {
        "current"
    }
}

/// The product-facing type words (spec: 数据库类型与模型词汇是两套).
fn product_type_name(cycle_type: CycleType) -> &'static str {
    match cycle_type {
        CycleType::Month => "long_term",
        CycleType::Week => "week",
        CycleType::Day => "day",
        CycleType::Session => "session",
    }
}

fn cycle_key(cycle: &Cycle) -> String {
    match cycle.cycle_type {
        CycleType::Month => match (&cycle.starts_on, &cycle.ends_on) {
            (Some(start), Some(end)) => calendar::long_term_key(
                calendar::parse_date(start).unwrap_or_default(),
                calendar::parse_date(end).unwrap_or_default(),
            ),
            _ => cycle.id.clone(),
        },
        CycleType::Week => match &cycle.starts_on {
            Some(start) => calendar::week_key(calendar::parse_date(start).unwrap_or_default()),
            None => cycle.id.clone(),
        },
        CycleType::Day => match &cycle.starts_on {
            Some(start) => calendar::day_key(calendar::parse_date(start).unwrap_or_default()),
            None => cycle.id.clone(),
        },
        CycleType::Session => format!("session:{}", cycle.id),
    }
}

/// Human label like "3 months" / "1 week" / "1 day" (spec: cycle_length).
fn cycle_length_label(cycle: &Cycle) -> String {
    const DAY_MS: i64 = 86_400_000;
    match cycle.duration {
        Some(ms) if ms > 0 => {
            let months = ms / (28 * DAY_MS);
            if months > 0 && ms % (28 * DAY_MS) == 0 {
                format!("{months} months")
            } else {
                let weeks = ms / (7 * DAY_MS);
                if weeks > 0 && ms % (7 * DAY_MS) == 0 {
                    format!("{weeks} weeks")
                } else {
                    format!("{} days", ms / DAY_MS)
                }
            }
        }
        _ => "not set".to_string(),
    }
}

fn cycle_block(cycle: &Cycle, parent: Option<&Cycle>) -> String {
    // parent_cycle_key is the literal `null` when absent, and the system
    // prompt for that case (goalless long-term) tells the model not to fetch
    // a parent (spec: 父周期不存在).
    let parent_key = match parent {
        Some(p) => escape_xml(&cycle_key(p)),
        None => "null".to_string(),
    };
    format!(
        "<cycle>\n    <cycle_key>{}</cycle_key>\n    <parent_cycle_key>{}</parent_cycle_key>\n    <cycle_type>{}</cycle_type>\n    <cycle_length>{}</cycle_length>\n    <starts_on>{}</starts_on>\n    <ends_on>{}</ends_on>\n  </cycle>",
        escape_xml(&cycle_key(cycle)),
        parent_key,
        product_type_name(cycle.cycle_type),
        escape_xml(&cycle_length_label(cycle)),
        cycle.starts_on.as_deref().unwrap_or("null"),
        cycle.ends_on.as_deref().unwrap_or("null"),
    )
}

fn time_block() -> String {
    format!(
        "  <current_date_and_time>{}</current_date_and_time>",
        Local::now().to_rfc3339()
    )
}

/// Task list with the clarity flags the specs make visible to the model.
fn tasks_block(tasks: &[Task], titles: &HashMap<String, String>) -> String {
    let mut out = String::from("  <tasks>\n");
    for task in tasks {
        let title = titles
            .get(&task.id)
            .cloned()
            .unwrap_or_else(|| task.title.clone());
        let mut attrs = format!("id=\"{}\"", escape_xml(&task.id));
        if task.completed {
            attrs.push_str(" completed=\"true\"");
        }
        if task.needs_refinement == Some(true) {
            attrs.push_str(" needs_refinement=\"true\"");
        }
        if task.needs_breakdown == Some(true) {
            attrs.push_str(" needs_breakdown=\"true\"");
        }
        if let Some(parent) = &task.parent_id {
            attrs.push_str(&format!(" parent_id=\"{}\"", escape_xml(parent)));
        }
        out.push_str(&format!(
            "    <task {}>{}</task>\n",
            attrs,
            escape_xml(&title)
        ));
    }
    out.push_str("  </tasks>");
    out
}

fn children_block(children: &[(Cycle, &'static str)]) -> String {
    let mut out = String::from("  <child_cycles>\n");
    for (cycle, word) in children {
        out.push_str(&format!(
            "    <cycle key=\"{}\" state=\"{}\" type=\"{}\"/>\n",
            escape_xml(&cycle_key(cycle)),
            word,
            product_type_name(cycle.cycle_type),
        ));
    }
    out.push_str("  </child_cycles>");
    out
}

/// Single-goal clarification snapshot (spec: 单任务澄清时的上下文): breakdown
/// plus the derived missing fields and flags.
fn task_snapshot_block(task: &Task) -> String {
    let breakdown = task
        .goal_breakdown
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    format!(
        "  <focused_task id=\"{}\" needs_refinement=\"{}\" needs_breakdown=\"{}\">\n    <goal_breakdown>{}</goal_breakdown>\n  </focused_task>",
        escape_xml(&task.id),
        task.needs_refinement == Some(true),
        task.needs_breakdown == Some(true),
        escape_xml(&breakdown),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cycle::Cycle;

    fn empty_ctx(cycle: Cycle) -> CycleContext {
        CycleContext {
            cycle,
            parent: None,
            children: Vec::new(),
            tasks: Vec::new(),
            referenced_titles: HashMap::new(),
            focused_task: None,
        }
    }

    fn cycle_of(type_: CycleType) -> Cycle {
        serde_json::from_value(serde_json::json!({
            "id": "c1", "title": "T", "type": match type_ { CycleType::Month => "month", CycleType::Week => "week", CycleType::Day => "day", CycleType::Session => "session" },
            "parent_id": null, "position": 0, "archived": false, "started": false,
            "finished": false, "started_at": null, "finished_at": null, "duration": 7257600000 as i64,
            "focused_time": 0, "starts_on": "2026-09-14", "ends_on": "2026-12-07",
            "calendar_key": null, "repeat_id": null, "created_at": 0
        }))
        .unwrap()
    }

    #[test]
    fn renders_cycle_metadata_with_product_words() {
        let ctx = empty_ctx(cycle_of(CycleType::Month));
        let xml = render(&ctx, None);
        assert!(xml.contains("<cycle_type>long_term</cycle_type>"));
        assert!(xml.contains("<cycle_length>3 months</cycle_length>"));
        assert!(xml.contains("<parent_cycle_key>null</parent_cycle_key>"));
        assert!(xml.contains("<current_date_and_time>"));
    }

    #[test]
    fn parent_key_uses_the_parent_cycle_key() {
        let mut parent = cycle_of(CycleType::Month);
        parent.id = "p1".into();
        let mut ctx = empty_ctx(cycle_of(CycleType::Week));
        ctx.parent = Some(parent);
        ctx.cycle.parent_id = Some("p1".into());
        let xml = render(&ctx, None);
        assert!(xml.contains(&format!(
            "parent_cycle_key>{}",
            calendar::long_term_key(
                calendar::parse_date("2026-09-14").unwrap(),
                calendar::parse_date("2026-12-07").unwrap()
            )
        )));
    }

    #[test]
    fn tasks_are_escaped_and_flagged() {
        let mut task: Task = serde_json::from_value(serde_json::json!({
            "id": "t1", "cycle_id": "c1", "parent_id": null, "title": "A <b> & bold",
            "subtasks": [], "position": 0, "completed": false,
            "needs_refinement": true, "needs_breakdown": null,
            "goal_breakdown": null, "copied_from_task_id": null,
            "proposal": null, "created_at": 0
        }))
        .unwrap();
        task.needs_refinement = Some(true);
        let ctx = empty_ctx(cycle_of(CycleType::Month));
        let mut ctx = CycleContext {
            tasks: vec![task],
            ..empty_ctx(cycle_of(CycleType::Month))
        };
        let xml = render(&ctx, None);
        assert!(xml.contains("A &lt;b&gt; &amp; bold"));
        assert!(xml.contains("needs_refinement=\"true\""));
        assert!(!xml.contains("needs_breakdown=\"true\""));
    }

    #[test]
    fn budget_drops_optional_blocks_first_then_important() {
        let mut child = cycle_of(CycleType::Week);
        child.id = "w1".into();
        let task: Task = serde_json::from_value(serde_json::json!({
            "id": "t1", "cycle_id": "c1", "parent_id": null, "title": "task",
            "subtasks": [], "position": 0, "completed": false,
            "needs_refinement": null, "needs_breakdown": null,
            "goal_breakdown": null, "copied_from_task_id": null,
            "proposal": null, "created_at": 0
        }))
        .unwrap();
        let mut ctx = empty_ctx(cycle_of(CycleType::Month));
        ctx.children = vec![(child, "current")];
        ctx.tasks = vec![task];
        let full = render(&ctx, None);
        // A limit just under the full size forces exactly one block to drop:
        // the optional one, never the important task list.
        let trimmed = render(&ctx, Some(full.len() - 40));
        assert!(!trimmed.contains("<child_cycles>"), "optional drops first");
        assert!(trimmed.contains("<tasks>"), "important survives first pass");
        // A limit no essential block fits drops everything droppable but
        // keeps the cycle metadata itself.
        let bare = render(&ctx, Some(1));
        assert!(bare.contains("<cycle>"), "essential never drops");
        assert!(!bare.contains("<tasks>"));
    }
}
