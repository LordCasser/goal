//! Task and goal domain.
//!
//! Goals, weekly items, daily tasks and subtasks are all rows in `tasks`;
//! their role comes from the owning cycle's type plus `parent_id`.
//! Behaviour contract: `openspec/specs/task-graph/spec.md`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::proposal::ProposalKind;

/// Clarity flags are tri-state: `None` means "never evaluated".
/// `Some(false)` means "evaluated and fine" — the two states mean different
/// things and must not collapse (spec: 清晰度标记三态语义).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarityFlags {
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
}

/// Palette key used to group goals inside a long-term cycle.
pub const ROOT_COLOR_KEYS: [&str; 8] = [
    "red", "amber", "gold", "green", "teal", "blue", "indigo", "plum",
];

pub fn is_valid_root_color_key(key: &str) -> bool {
    ROOT_COLOR_KEYS.contains(&key)
}

/// A nested checklist entry stored in `tasks.subtasks` as JSON.
/// The flat form `[{"title","completed"}]` from `docs/architecture.md` stays
/// valid; `children` is omitted while empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subtask {
    pub title: String,
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Subtask>,
}

impl Subtask {
    pub fn new(title: impl Into<String>, completed: bool) -> Self {
        Self {
            title: title.into(),
            completed,
            children: Vec::new(),
        }
    }
}

/// One row of the unified task graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub cycle_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub subtasks: Vec<Subtask>,
    pub position: i64,
    pub completed: bool,
    pub goal_breakdown: Option<serde_json::Value>,
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
    pub root_color_key: Option<String>,
    pub copied_from_task_id: Option<String>,
    /// `Some` marks a pending agent proposal; committed data is always `None`.
    pub proposal: Option<ProposalKind>,
    pub created_at: i64,
}

/// A task with its same-cycle children, ordered by `position` (stable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskNode {
    #[serde(flatten)]
    pub task: Task,
    pub children: Vec<TaskNode>,
    /// `subtasks` rendered as Markdown for AI context and sessions.
    pub subtasks_markdown: String,
}

/// Builds the forest for one cycle. Tasks whose `parent_id` is absent from the
/// set (e.g. a weekly item linked to a long-term goal) become roots; sibling
/// order is a stable sort by `position`. Guards against parent cycles that the
/// schema's plain FK cannot rule out.
pub fn build_tree(tasks: Vec<Task>) -> Vec<TaskNode> {
    let mut sorted = tasks;
    sorted.sort_by_key(|t| t.position);

    let ids: HashSet<&str> = sorted.iter().map(|t| t.id.as_str()).collect();
    let mut children_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for (idx, task) in sorted.iter().enumerate() {
        if let Some(parent) = task.parent_id.as_deref() {
            children_of.entry(parent).or_default().push(idx);
        }
    }

    fn attach(
        idx: usize,
        sorted: &[Task],
        children_of: &HashMap<&str, Vec<usize>>,
        visited: &mut HashSet<usize>,
    ) -> TaskNode {
        let task = sorted[idx].clone();
        let markdown = render_subtasks_markdown(&task.subtasks);
        let mut node = TaskNode {
            task,
            children: Vec::new(),
            subtasks_markdown: markdown,
        };
        visited.insert(idx);
        if let Some(kids) = children_of.get(sorted[idx].id.as_str()) {
            for &kid in kids {
                if !visited.contains(&kid) {
                    node.children
                        .push(attach(kid, sorted, children_of, visited));
                }
            }
        }
        node
    }

    let mut roots = Vec::new();
    let mut visited = HashSet::new();
    for (idx, task) in sorted.iter().enumerate() {
        let is_root = match task.parent_id.as_deref() {
            None => true,
            Some(parent) => !ids.contains(parent),
        };
        if is_root && !visited.contains(&idx) {
            roots.push(attach(idx, &sorted, &children_of, &mut visited));
        }
    }
    // Tasks caught in a parent cycle never became roots; surface them as roots.
    for idx in 0..sorted.len() {
        if !visited.contains(&idx) {
            roots.push(attach(idx, &sorted, &children_of, &mut visited));
        }
    }
    roots
}

/// Whether the row is an "empty input row": blank title, no checklist content,
/// not completed. Agent goal creation replaces such a trailing row instead of
/// appending (spec: 新建目标复用空行).
pub fn is_empty_input_row(task: &Task) -> bool {
    task.title.trim().is_empty()
        && !task.completed
        && task
            .subtasks
            .iter()
            .all(|s| s.title.trim().is_empty() && s.children.is_empty())
}

// ---------------------------------------------------------------------------
// Subtask <-> Markdown rendering
//
// Structured storage is authoritative; the Markdown form is for AI context and
// must carry the same levels and order (spec: 子任务的结构化存储与文本渲染).
// Titles are escaped so list/checkbox/heading meta characters cannot change
// the structure, and parsing the rendered text reproduces the input exactly.
// ---------------------------------------------------------------------------

/// Characters that would be re-read as structure when a title starts with them.
fn starts_with_meta(chars: &[char]) -> bool {
    match chars.first() {
        Some('-' | '+' | '*' | '#' | '>' | '[') => true,
        Some(c) if c.is_ascii_digit() => {
            let mut i = 1;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            i < chars.len() && (chars[i] == '.' || chars[i] == ')')
        }
        _ => false,
    }
}

fn escape_title(title: &str) -> String {
    let cleaned = title.replace(['\r', '\n'], " ");
    let mut out = String::with_capacity(cleaned.len() + 1);
    let chars: Vec<char> = cleaned.chars().collect();
    if starts_with_meta(&chars) {
        out.push('\\');
    }
    for c in chars {
        if c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

const ESCAPABLE: &str = "\\-+*#>[";

fn unescape_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&next) if ESCAPABLE.contains(next) || next.is_ascii_digit() => {
                    out.push(next);
                    chars.next();
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Renders nested subtasks as a Markdown checklist. An empty list renders as
/// the empty string — no placeholder, no empty heading.
pub fn render_subtasks_markdown(subtasks: &[Subtask]) -> String {
    fn walk(out: &mut String, items: &[Subtask], depth: usize) {
        for item in items {
            let indent = "  ".repeat(depth);
            let box_ = if item.completed { "[x]" } else { "[ ]" };
            out.push_str(&indent);
            out.push_str("- ");
            out.push_str(box_);
            out.push(' ');
            out.push_str(&escape_title(&item.title));
            out.push('\n');
            walk(out, &item.children, depth + 1);
        }
    }
    let mut out = String::new();
    walk(&mut out, subtasks, 0);
    out
}

/// Parses a Markdown checklist back into structured subtasks. Inverse of
/// [`render_subtasks_markdown`] for any input that function produced.
pub fn parse_subtasks_markdown(markdown: &str) -> Vec<Subtask> {
    // Invariant while building: stack[d] holds the items collected at depth d.
    let mut stack: Vec<Vec<Subtask>> = vec![Vec::new()];

    fn fold_up(stack: &mut Vec<Vec<Subtask>>) {
        while stack.len() > 1 {
            let finished = stack.pop().expect("stack non-empty");
            let top = stack.last_mut().expect("stack non-empty");
            match top.last_mut() {
                Some(parent) => parent.children = finished,
                // Malformed indent (no parent to attach to): keep the items at
                // the shallower level instead of dropping them.
                None => top.extend(finished),
            }
        }
    }

    for line in markdown.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        let depth = indent / 2;
        let mut rest = &line[indent..];

        if let Some(after) = rest.strip_prefix(['-', '+', '*']) {
            rest = after.strip_prefix(' ').unwrap_or(after);
        }
        let mut completed = false;
        if let Some(after) = rest.strip_prefix("[ ] ") {
            rest = after;
        } else if let Some(after) = rest
            .strip_prefix("[x] ")
            .or_else(|| rest.strip_prefix("[X] "))
        {
            completed = true;
            rest = after;
        } else if rest == "[ ]" {
            rest = "";
        } else if rest == "[x]" || rest == "[X]" {
            completed = true;
            rest = "";
        }

        let item = Subtask {
            title: unescape_title(rest),
            completed,
            children: Vec::new(),
        };

        if stack.len() > depth + 1 {
            fold_up(&mut stack);
        }
        while stack.len() < depth + 1 {
            stack.push(Vec::new());
        }
        stack.last_mut().expect("stack non-empty").push(item);
    }
    fold_up(&mut stack);
    stack.pop().expect("root list")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, parent: Option<&str>, position: i64) -> Task {
        Task {
            id: id.into(),
            cycle_id: "c1".into(),
            parent_id: parent.map(Into::into),
            title: format!("task {id}"),
            subtasks: vec![],
            position,
            completed: false,
            goal_breakdown: None,
            needs_refinement: None,
            needs_breakdown: None,
            root_color_key: None,
            copied_from_task_id: None,
            proposal: None,
            created_at: 0,
        }
    }

    fn sub(title: &str, completed: bool, children: Vec<Subtask>) -> Subtask {
        Subtask {
            title: title.into(),
            completed,
            children,
        }
    }

    #[test]
    fn tree_groups_by_parent_and_stable_sorts_by_position() {
        let tasks = vec![
            task("b", None, 1),
            task("a", None, 0),
            task("late-a2", None, 0), // ties keep insertion order (stable)
            task("b2", Some("b"), 5),
            task("b1", Some("b"), 1),
        ];
        let tree = build_tree(tasks);
        let root_titles: Vec<&str> = tree.iter().map(|n| n.task.id.as_str()).collect();
        assert_eq!(root_titles, vec!["a", "late-a2", "b"]);
        let kids: Vec<&str> = tree[2]
            .children
            .iter()
            .map(|n| n.task.id.as_str())
            .collect();
        assert_eq!(kids, vec!["b1", "b2"]);
    }

    #[test]
    fn tree_treats_cross_cycle_parent_as_root() {
        let tasks = vec![task("w1", Some("long-term-goal"), 0)];
        let tree = build_tree(tasks);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].task.parent_id.as_deref(), Some("long-term-goal"));
    }

    #[test]
    fn tree_survives_parent_cycle() {
        // A schema-level FK cannot rule out a↔b; build_tree must terminate and
        // keep every row renderable (one becomes the root, the other nests).
        let tasks = vec![task("a", Some("b"), 0), task("b", Some("a"), 1)];
        let tree = build_tree(tasks);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].task.id, "a");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].task.id, "b");
    }

    #[test]
    fn empty_subtasks_render_to_empty_string() {
        assert_eq!(render_subtasks_markdown(&[]), "");
        let empty: Vec<Subtask> = parse_subtasks_markdown("");
        assert!(empty.is_empty());
    }

    #[test]
    fn markdown_renders_levels_and_order() {
        let items = vec![
            sub(
                "first",
                false,
                vec![sub("child", true, vec![]), sub("child2", false, vec![])],
            ),
            sub("second", true, vec![]),
        ];
        let md = render_subtasks_markdown(&items);
        assert_eq!(
            md,
            "- [ ] first\n  - [x] child\n  - [ ] child2\n- [x] second\n"
        );
        let parsed = parse_subtasks_markdown(&md);
        assert_eq!(parsed, items);
    }

    #[test]
    fn markdown_round_trips_nested_levels() {
        let items = vec![sub(
            "a",
            false,
            vec![sub("a1", false, vec![sub("deep", true, vec![])])],
        )];
        let parsed = parse_subtasks_markdown(&render_subtasks_markdown(&items));
        assert_eq!(parsed, items);
    }

    #[test]
    fn markdown_escapes_meta_characters() {
        let tricky = [
            "- looks like a bullet",
            "* star",
            "+ plus",
            "# heading",
            "1. ordered",
            "12) paren",
            "[ ] fake checkbox",
            "[x] done-looking",
            "back\\slash",
            "> quote",
            "normal title",
            "",
        ];
        let items: Vec<Subtask> = tricky.iter().map(|t| Subtask::new(*t, false)).collect();
        let parsed = parse_subtasks_markdown(&render_subtasks_markdown(&items));
        assert_eq!(
            parsed, items,
            "round trip must preserve meta-character titles"
        );
    }

    #[test]
    fn markdown_sanitizes_newlines_instead_of_breaking_structure() {
        let items = vec![sub("line one\nline two", false, vec![])];
        let md = render_subtasks_markdown(&items);
        assert_eq!(
            md.lines().count(),
            1,
            "a title newline must not become a new list item"
        );
        let parsed = parse_subtasks_markdown(&md);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "line one line two");
    }

    #[test]
    fn empty_input_row_detection() {
        let mut t = task("x", None, 0);
        t.title = "  ".into();
        assert!(is_empty_input_row(&t));
        t.title = "hello".into();
        assert!(!is_empty_input_row(&t));
        t.title = String::new();
        t.completed = true;
        assert!(!is_empty_input_row(&t));
        t.completed = false;
        t.subtasks = vec![Subtask::new("leftover", false)];
        assert!(!is_empty_input_row(&t));
    }

    #[test]
    fn subtask_json_is_backward_compatible_flat_form() {
        let flat = r#"[{"title":"a","completed":false},{"title":"b","completed":true}]"#;
        let parsed: Vec<Subtask> = serde_json::from_str(flat).expect("flat form parses");
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].children.is_empty());
        assert_eq!(parsed[1].completed, true);
    }

    #[test]
    fn color_key_validation() {
        assert!(is_valid_root_color_key("red"));
        assert!(is_valid_root_color_key("plum"));
        assert!(!is_valid_root_color_key("hot-pink"));
        assert!(!is_valid_root_color_key(""));
    }
}
