//! Personal voice is separate from workflow skills and invariant tool policy.
use crate::error::{AppError, AppResult};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
const DEFAULT: &str = include_str!("persona.md");

pub fn install_missing(goal_dir: &Path) -> AppResult<()> {
    fs::create_dir_all(goal_dir).map_err(error)?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(goal_dir.join("persona.md"))
    {
        Ok(mut file) => file.write_all(DEFAULT.as_bytes()).map_err(error),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(error(e)),
    }
}
fn error(e: impl std::fmt::Display) -> AppError {
    AppError::validation(
        "persona_unavailable",
        format!("Cannot read ~/.goal/persona.md: {e}"),
    )
}
pub fn load_from(goal_dir: &Path) -> AppResult<String> {
    let mut text = String::new();
    fs::File::open(goal_dir.join("persona.md"))
        .map_err(error)?
        .take(16_385)
        .read_to_string(&mut text)
        .map_err(error)?;
    if text.trim().is_empty() || text.len() > 16_384 {
        return Err(error("Use 1–16384 bytes of UTF-8 text"));
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
        install_missing(dir.path()).unwrap();
        assert!(load_from(dir.path()).unwrap().contains("秘书"));
        fs::write(dir.path().join("persona.md"), "只用简短中文回复").unwrap();
        install_missing(dir.path()).unwrap();
        assert_eq!(load_from(dir.path()).unwrap(), "只用简短中文回复");
        fs::write(dir.path().join("persona.md"), " ").unwrap();
        assert!(load_from(dir.path()).is_err());
        fs::write(dir.path().join("persona.md"), "x".repeat(16_385)).unwrap();
        assert!(load_from(dir.path()).is_err());
    }
}
