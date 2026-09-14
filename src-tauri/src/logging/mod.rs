//! Local debug logging: the app's single on-disk log channel.
//!
//! Every module writes diagnostics through this module into `<data dir>/logs`;
//! there is no other sink and no network reporting of any kind (spec:
//! `openspec/specs/local-logging/spec.md`, `docs/architecture.md` 本地调试日志).
//! Logging failures are always silent: diagnostics must never break or block
//! business flows.
//!
//! Line format: `[RFC3339 UTC] [LEVEL] [module] message`. The active file
//! rotates at [`MAX_LOG_BYTES`], keeping at most [`MAX_LOG_FILES`] archives.
//! Strings registered via [`register_secret`] are replaced with `[REDACTED]`
//! before a line reaches the disk — including inside formatted error chains,
//! so credentials hidden in a `Result` chain are masked the same way.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::Utc;

/// Size cap of the active log file before it rotates (1 MiB).
const MAX_LOG_BYTES: u64 = 1_048_576;

/// Number of rotated archives (`planner.log.1` … `planner.log.N`) kept.
const MAX_LOG_FILES: usize = 5;

/// Name of the active log file inside the log directory.
const LOG_FILE: &str = "planner.log";

/// Replacement token written instead of a registered secret.
const REDACTED: &str = "[REDACTED]";

/// Severity of a log line. Higher values are more verbose: a message is
/// written when its numeric level is at or below the configured one, so at
/// `Debug` everything is written while at `Error` only errors are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn as_u8(self) -> u8 {
        match self {
            Level::Error => 0,
            Level::Warn => 1,
            Level::Info => 2,
            Level::Debug => 3,
        }
    }

    fn from_u8(value: u8) -> Level {
        match value {
            0 => Level::Error,
            1 => Level::Warn,
            2 => Level::Info,
            _ => Level::Debug,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }

    /// Parses a persisted `app_settings.log_level` value; unknown strings
    /// yield `None` so the caller falls back to the default level.
    pub fn parse(raw: &str) -> Option<Level> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Level::Error),
            "warn" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            _ => None,
        }
    }
}

/// Global logger state. All fields are interior-mutable so [`init`] can stay
/// idempotent while the level stays adjustable at runtime.
struct Logger {
    level: AtomicU8,
    dir: Mutex<PathBuf>,
    secrets: Mutex<HashSet<String>>,
    /// Serializes rotate + append so lines never interleave.
    write_lock: Mutex<()>,
    max_bytes: AtomicU64,
    max_files: AtomicUsize,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Initializes the global logger for `dir`, creating the directory best
/// effort (an unusable directory is not an error; writes fail silently).
/// Idempotent: repeated calls neither panic nor overwrite the directory or
/// level of the already-initialized instance; `initial_level` only applies
/// on the first call.
pub fn init(dir: PathBuf, initial_level: Option<Level>) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = LOGGER.get_or_init(|| Logger {
        level: AtomicU8::new(initial_level.unwrap_or(Level::Info).as_u8()),
        dir: Mutex::new(dir),
        secrets: Mutex::new(HashSet::new()),
        write_lock: Mutex::new(()),
        max_bytes: AtomicU64::new(MAX_LOG_BYTES),
        max_files: AtomicUsize::new(MAX_LOG_FILES),
    });
}

/// Adjusts the global level at runtime; takes effect on the next write.
pub fn set_level(level: Level) {
    if let Some(logger) = LOGGER.get() {
        logger.level.store(level.as_u8(), Ordering::Relaxed);
    }
}

/// Current global level. Reports the `Info` default before [`init`].
pub fn level() -> Level {
    match LOGGER.get() {
        Some(logger) => Level::from_u8(logger.level.load(Ordering::Relaxed)),
        None => Level::Info,
    }
}

/// Adds an exact string to the redaction list: every occurrence in any future
/// log line is replaced with `[REDACTED]` before it is written. Empty strings
/// are ignored so a blank credential cannot wreck every line.
pub fn register_secret(secret: &str) {
    if secret.is_empty() {
        return;
    }
    if let Some(logger) = LOGGER.get() {
        logger
            .secrets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(secret.to_string());
    }
}

/// Writes one line through the unified channel after level filtering and
/// redaction. Never panics and never reports failure to the caller.
pub fn log(module: &str, level: Level, message: &str) {
    let Some(logger) = LOGGER.get() else { return };
    if level.as_u8() > logger.level.load(Ordering::Relaxed) {
        return;
    }
    let line = format!(
        "[{}] [{}] [{}] {}\n",
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        level.label(),
        module,
        redact(logger, message),
    );
    let _guard = logger.write_lock.lock().unwrap_or_else(|p| p.into_inner());
    if let Err(e) = append_line(logger, &line) {
        eprintln!("planner log write failed: {e}");
    }
}

/// Convenience wrappers for a single severity.
pub fn error(module: &str, message: &str) {
    log(module, Level::Error, message);
}

pub fn warn(module: &str, message: &str) {
    log(module, Level::Warn, message);
}

pub fn info(module: &str, message: &str) {
    log(module, Level::Info, message);
}

pub fn debug(module: &str, message: &str) {
    log(module, Level::Debug, message);
}

/// Replaces every registered secret occurring in `message`. Callers pass
/// formatted error chains through [`log`] too, so credentials inside a
/// `format!("{e:?}")` chain are masked here just the same.
fn redact(logger: &Logger, message: &str) -> String {
    let secrets = logger.secrets.lock().unwrap_or_else(|p| p.into_inner());
    if secrets.is_empty() {
        return message.to_string();
    }
    let mut redacted = message.to_string();
    for secret in secrets.iter() {
        redacted = redacted.replace(secret.as_str(), REDACTED);
    }
    redacted
}

/// Appends one line to the active file, rotating first when the file would
/// exceed the size cap. Must run while holding `write_lock`.
fn append_line(logger: &Logger, line: &str) -> std::io::Result<()> {
    let dir = logger.dir.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let path = dir.join(LOG_FILE);
    let current = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let max_bytes = logger.max_bytes.load(Ordering::Relaxed);
    if current + line.len() as u64 > max_bytes {
        rotate(&dir, logger.max_files.load(Ordering::Relaxed));
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())
}

/// Shifts archives up (`planner.log.{N-1}` -> `planner.log.{N}` …
/// `planner.log` -> `planner.log.1`) and drops the archive that falls beyond
/// `max_files`. All errors are swallowed: a failed rotation must not block
/// the write attempt.
fn rotate(dir: &Path, max_files: usize) {
    let _ = std::fs::remove_file(dir.join(format!("{LOG_FILE}.{max_files}")));
    for index in (1..max_files).rev() {
        let _ = std::fs::rename(
            dir.join(format!("{LOG_FILE}.{index}")),
            dir.join(format!("{LOG_FILE}.{}", index + 1)),
        );
    }
    let _ = std::fs::rename(dir.join(LOG_FILE), dir.join(format!("{LOG_FILE}.1")));
}

/// Test-only override of the rotation limits. Changes the logger's atomics
/// only; the production constants keep their meaning.
#[cfg(test)]
pub(crate) fn set_limits_for_test(max_bytes: u64, max_files: usize) {
    if let Some(logger) = LOGGER.get() {
        logger.max_bytes.store(max_bytes, Ordering::Relaxed);
        logger.max_files.store(max_files, Ordering::Relaxed);
    }
}

/// Test-only: re-points the global logger at a fresh directory and resets
/// level, secrets and rotation limits to a known state.
#[cfg(test)]
pub(crate) fn reset_for_test(dir: PathBuf, level: Level) {
    init(dir.clone(), Some(level));
    let logger = LOGGER.get().expect("logger initialized");
    *logger.dir.lock().unwrap_or_else(|p| p.into_inner()) = dir;
    logger
        .secrets
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clear();
    logger.level.store(level.as_u8(), Ordering::Relaxed);
    logger.max_bytes.store(MAX_LOG_BYTES, Ordering::Relaxed);
    logger.max_files.store(MAX_LOG_FILES, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The logger is global, so these tests interfere with each other; this
    /// lock serializes them (each one re-points the logger at its own temp
    /// directory via `reset_for_test`).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn read_log(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(LOG_FILE)).unwrap_or_default()
    }

    #[test]
    fn level_filtering_respects_threshold() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let logs = tmp.path().join("logs");
        reset_for_test(logs.clone(), Level::Info);

        assert_eq!(level(), Level::Info);
        debug("test", "debug detail");
        assert!(!read_log(&logs).contains("debug detail"));

        info("test", "info line");
        assert!(read_log(&logs).contains("info line"));

        set_level(Level::Error);
        assert_eq!(level(), Level::Error);
        warn("test", "warn line");
        assert!(!read_log(&logs).contains("warn line"));

        error("test", "error line");
        assert!(read_log(&logs).contains("error line"));
    }

    #[test]
    fn lines_carry_timestamp_module_and_level() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let logs = tmp.path().join("logs");
        reset_for_test(logs.clone(), Level::Debug);

        error("db", "boom");
        info("app", "ready");

        let contents = read_log(&logs);
        assert!(contents.lines().any(|l| l.ends_with("[ERROR] [db] boom")));
        assert!(contents.lines().any(|l| l.ends_with("[INFO] [app] ready")));
        // Every line opens with an RFC3339 UTC timestamp, e.g.
        // `[2026-09-15T08:15:30.123Z] [LEVEL] [module] message`.
        for line in contents.lines() {
            assert!(line.starts_with("[2"), "missing timestamp: {line}");
            assert!(line.contains("Z] ["), "malformed prefix: {line}");
        }
    }

    #[test]
    fn rotation_evicts_oldest_files() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let logs = tmp.path().join("logs");
        reset_for_test(logs.clone(), Level::Info);
        // Each rendered line is ~50 bytes, so a 48-byte cap rotates per write.
        set_limits_for_test(48, 2);

        for marker in ["marker-a", "marker-b", "marker-c", "marker-d"] {
            info("test", marker);
        }

        let archive = |n: usize| {
            std::fs::read_to_string(logs.join(format!("{LOG_FILE}.{n}"))).unwrap_or_default()
        };
        assert!(read_log(&logs).contains("marker-d"));
        assert!(archive(1).contains("marker-c"));
        assert!(archive(2).contains("marker-b"));
        // The oldest marker was evicted and no third archive exists.
        assert!(!read_log(&logs).contains("marker-a"));
        assert!(!archive(1).contains("marker-a"));
        assert!(!archive(2).contains("marker-a"));
        assert!(!logs.join(format!("{LOG_FILE}.3")).exists());
    }

    #[test]
    fn write_failures_are_silent() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        // A regular file blocks directory creation beneath it.
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("blocker");
        reset_for_test(blocker.join("logs"), Level::Debug);

        log("test", Level::Error, "error line");
        warn("test", "warn line");
        info("test", "info line");
        debug("test", "debug line");
        // Reaching this point means no panic; failures stay unreported.
        assert!(!blocker.join("logs").is_dir());
    }

    #[test]
    fn secrets_are_redacted_including_error_chains() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let logs = tmp.path().join("logs");
        reset_for_test(logs.clone(), Level::Info);

        register_secret("sk-test-abcdef123456");
        // An empty "credential" must not wreck every line.
        register_secret("");

        info("ai", "request failed with key sk-test-abcdef123456");
        let chain = std::io::Error::other("connect rejected for sk-test-abcdef123456");
        error("ai", &format!("{chain:?}"));
        info("ai", "plain line stays intact");

        let contents = read_log(&logs);
        assert!(!contents.contains("sk-test-abcdef123456"));
        assert!(contents.contains("[REDACTED]"));
        assert!(contents.contains("plain line stays intact"));
    }

    #[test]
    fn init_is_idempotent_and_keeps_first_directory() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        reset_for_test(first.path().join("logs"), Level::Info);

        init(second.path().join("logs"), Some(Level::Error));
        info("test", "after re-init");

        assert!(read_log(&first.path().join("logs")).contains("after re-init"));
        assert!(!second.path().join("logs").join(LOG_FILE).exists());
        // Repeated init must not change the level either.
        assert_eq!(level(), Level::Info);
    }
}
