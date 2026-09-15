//! User-editable workflows, independent of providers, conversations and tool permissions.
use std::{fs, io::{Read, Write}, path::{Path, PathBuf}};
use crate::error::{AppError, AppResult};

#[derive(Clone, Copy, Debug)]
pub enum Skill {
    Coach, GoalClarification, LongTermPlanning, WeeklyPlanning, DailyPlanning,
    Prioritization, CycleReview, PeriodAnalysis, PlanningIssues,
}

impl Skill {
    pub const ALL: [Self; 9] = [Self::Coach, Self::GoalClarification, Self::LongTermPlanning,
        Self::WeeklyPlanning, Self::DailyPlanning, Self::Prioritization, Self::CycleReview,
        Self::PeriodAnalysis, Self::PlanningIssues];
    pub fn name(self) -> &'static str {
        match self {
            Self::Coach => "coach", Self::GoalClarification => "goal-clarification",
            Self::LongTermPlanning => "long-term-planning", Self::WeeklyPlanning => "weekly-planning",
            Self::DailyPlanning => "daily-planning", Self::Prioritization => "prioritization",
            Self::CycleReview => "cycle-review", Self::PeriodAnalysis => "period-analysis",
            Self::PlanningIssues => "planning-issues",
        }
    }
    pub fn parse(name: &str) -> Option<Self> { Self::ALL.into_iter().find(|s| s.name() == name) }
    pub fn agent_skill(self) -> crate::ai::llm::types::AgentSkill {
        use crate::ai::llm::types::AgentSkill as A;
        match self {
            Self::Coach => A::None, Self::GoalClarification => A::GoalSetting,
            Self::LongTermPlanning => A::LongTermPlanning, Self::WeeklyPlanning => A::WeeklyPlanning,
            Self::DailyPlanning => A::DailyPlanning, Self::Prioritization => A::Prioritization,
            Self::CycleReview => A::Review, Self::PeriodAnalysis => A::PeriodAnalysis,
            Self::PlanningIssues => A::PlanningIssues,
        }
    }
    pub fn builtin(self) -> &'static str {
        match self {
            Self::Coach => include_str!("skills/coach/SKILL.md"),
            Self::GoalClarification => include_str!("skills/goal-clarification/SKILL.md"),
            Self::LongTermPlanning => include_str!("skills/long-term-planning/SKILL.md"),
            Self::WeeklyPlanning => include_str!("skills/weekly-planning/SKILL.md"),
            Self::DailyPlanning => include_str!("skills/daily-planning/SKILL.md"),
            Self::Prioritization => include_str!("skills/prioritization/SKILL.md"),
            Self::CycleReview => include_str!("skills/cycle-review/SKILL.md"),
            Self::PeriodAnalysis => include_str!("skills/period-analysis/SKILL.md"),
            Self::PlanningIssues => include_str!("skills/planning-issues/SKILL.md"),
        }
    }
}

pub fn directory() -> AppResult<PathBuf> {
    // Tauri's home resolution is used at setup. This helper is also callable
    // from the provider-independent service layer without an AppHandle.
    #[allow(deprecated)]
    std::env::home_dir().map(|p| p.join(".goal/skills"))
        .ok_or_else(|| AppError::Internal("cannot resolve the user home directory".into()))
}

/// Never overwrite an existing workflow, including an invalid user edit.
pub fn install_missing(root: &Path) -> AppResult<()> {
    for skill in Skill::ALL {
        let dir = root.join(skill.name());
        fs::create_dir_all(&dir).map_err(|e| skill_error(&dir, e))?;
        let path = dir.join("SKILL.md");
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => file.write_all(skill.builtin().as_bytes()).map_err(|e| skill_error(&path, e))?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(e) => return Err(skill_error(&path, e)),
        }
    }
    Ok(())
}

fn skill_error(path: &Path, error: impl std::fmt::Display) -> AppError {
    AppError::validation("skill_unavailable", format!("{}: {error}", path.display()))
}

pub fn load_from(root: &Path, skill: Skill) -> AppResult<String> {
    let path = root.join(skill.name()).join("SKILL.md");
    let mut text = String::new();
    fs::File::open(&path).map_err(|e| skill_error(&path, e))?.take(65_537)
        .read_to_string(&mut text).map_err(|e| skill_error(&path, e))?;
    if text.trim().is_empty() || text.len() > 65_536 {
        return Err(skill_error(&path, "SKILL.md must contain 1–65536 bytes of UTF-8 instructions"));
    }
    Ok(text)
}

pub fn load(skill: Skill) -> AppResult<String> {
    // Unit tests never read or create personal configuration; filesystem
    // behavior is tested explicitly below against isolated temporary roots.
    #[cfg(test)]
    { Ok(skill.builtin().to_string()) }
    #[cfg(not(test))]
    { load_from(&directory()?, skill) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_preserves_user_edits_and_load_observes_changes() {
        let dir = tempfile::tempdir().unwrap();
        install_missing(dir.path()).unwrap();
        for skill in Skill::ALL { assert!(!load_from(dir.path(), skill).unwrap().is_empty()); }
        let path = dir.path().join("daily-planning/SKILL.md");
        fs::write(&path, "My daily workflow").unwrap();
        install_missing(dir.path()).unwrap();
        assert_eq!(load_from(dir.path(), Skill::DailyPlanning).unwrap(), "My daily workflow");
        fs::write(&path, "Second revision").unwrap();
        assert_eq!(load_from(dir.path(), Skill::DailyPlanning).unwrap(), "Second revision");
        fs::write(&path, " ").unwrap();
        assert!(load_from(dir.path(), Skill::DailyPlanning).is_err());
        fs::write(&path, "x".repeat(65_537)).unwrap();
        assert!(load_from(dir.path(), Skill::DailyPlanning).is_err());
    }
}
