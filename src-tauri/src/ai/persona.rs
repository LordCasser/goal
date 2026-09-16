//! Personal voice is separate from workflow skills and invariant tool policy.
use crate::error::{AppError, AppResult};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
const DEFAULT: &str = include_str!("persona.md");

pub fn install_missing(goal_dir: &Path) -> AppResult<()> {
    fs::create_dir_all(goal_dir).map_err(|e| error(goal_dir, "create", e))?;
    let path = goal_dir.join("persona.md");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(DEFAULT.as_bytes())
            .map_err(|e| error(&path, "write", e)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(error(&path, "write", e)),
    }
}
fn error(path: &Path, operation: &str, e: impl std::fmt::Display) -> AppError {
    AppError::validation(
        "persona_unavailable",
        format!("Cannot {operation} {}: {e}", path.display()),
    )
}
pub fn load_from(goal_dir: &Path) -> AppResult<String> {
    let path = goal_dir.join("persona.md");
    let mut text = String::new();
    fs::File::open(&path)
        .map_err(|e| error(&path, "read", e))?
        .take(16_385)
        .read_to_string(&mut text)
        .map_err(|e| error(&path, "read", e))?;
    if text.trim().is_empty() || text.len() > 16_384 {
        return Err(error(&path, "read", "Use 1–16384 bytes of UTF-8 text"));
    }
    Ok(text)
}
pub fn load() -> AppResult<String> {
    #[cfg(test)]
    {
        Ok(DEFAULT.into())
    }
    #[cfg(not(test))]
    {
        let skills = crate::ai::skills::directory()?;
        load_from(skills.parent().expect("skills has a parent"))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_install_preserves_edits_and_runtime_rereads() {
        let dir = tempfile::tempdir().unwrap();
        let goal_dir = dir.path().join("用户 profile");
        install_missing(&goal_dir).unwrap();
        assert!(load_from(&goal_dir)
            .unwrap()
            .contains("secretary or assistant"));
        fs::write(goal_dir.join("persona.md"), "只用简短中文回复").unwrap();
        install_missing(&goal_dir).unwrap();
        assert_eq!(load_from(&goal_dir).unwrap(), "只用简短中文回复");
        fs::write(goal_dir.join("persona.md"), " ").unwrap();
        assert!(load_from(&goal_dir).is_err());
        fs::write(goal_dir.join("persona.md"), "x".repeat(16_385)).unwrap();
        assert!(load_from(&goal_dir).is_err());
    }

    #[test]
    fn errors_name_the_actual_install_and_read_paths() {
        let dir = tempfile::tempdir().unwrap();
        let goal_dir = dir.path().join("用户 profile");
        let missing = goal_dir.join("missing");
        let read_error = load_from(&missing).unwrap_err().to_string();
        assert!(read_error.contains(&missing.join("persona.md").display().to_string()));
        assert!(read_error.contains("Cannot read"));

        let blocker = dir.path().join("安装 blocker");
        fs::write(&blocker, "not a directory").unwrap();
        let install_error = install_missing(&blocker).unwrap_err().to_string();
        assert!(install_error.contains(&blocker.display().to_string()));
        assert!(install_error.contains("Cannot create"));
    }
}
