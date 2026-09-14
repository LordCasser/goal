//! Telemetry boundary.
//!
//! Behaviour contract: `openspec/specs/telemetry/spec.md`. Non-negotiables:
//!
//! * the switch is off when unset — a missing setting means "not enabled";
//! * events carry only enum names, counters and durations — never free text,
//!   so goal or task titles cannot leak into a payload even by accident;
//! * reporting never sits on a business critical path: recording is a
//!   non-blocking enqueue that drops silently when the queue is full;
//! * the anonymous id is a random uuid, independent of device fingerprint and
//!   of any account or credential identifier;
//! * with telemetry off and AI unused, the app makes no network requests.
//!
//! The wire endpoint is a product decision that ships with the AI change;
//! until an endpoint is configured the default sink drops batches, which keeps
//! the channel exercised end to end without inventing a destination.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::db::Db;
use crate::error::AppResult;
use crate::repository::settings as repo;

/// Fixed event vocabulary. Never add a `String` payload field here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventName {
    AppLaunched,
    LongTermCycleCreated,
    SessionFinished,
    BackupExported,
    PreviewKept,
    PreviewReverted,
}

impl EventName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppLaunched => "app_launched",
            Self::LongTermCycleCreated => "long_term_cycle_created",
            Self::SessionFinished => "session_finished",
            Self::BackupExported => "backup_exported",
            Self::PreviewKept => "preview_kept",
            Self::PreviewReverted => "preview_reverted",
        }
    }
}

/// One measurable behaviour occurrence: a named enum value, a counter and an
/// optional duration. Serialization is the contract — no free-text field
/// exists to fill by mistake.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryEvent {
    pub name: &'static str,
    pub count: u32,
    pub duration_ms: Option<u64>,
}

impl TelemetryEvent {
    pub fn new(name: EventName) -> Self {
        Self { name: name.as_str(), count: 1, duration_ms: None }
    }

    pub fn with_duration(name: EventName, duration_ms: u64) -> Self {
        Self { name: name.as_str(), count: 1, duration_ms: Some(duration_ms) }
    }
}

/// Where batches go. The production sink is `UnconfiguredSink` until an
/// endpoint ships; tests provide recording/failing/slow sinks.
pub trait Sink: Send + Sync {
    fn send(&self, batch: &[TelemetryEvent]) -> Result<(), String>;
}

/// No destination configured: every batch is dropped, silently.
pub struct UnconfiguredSink;

impl Sink for UnconfiguredSink {
    fn send(&self, _batch: &[TelemetryEvent]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TelemetrySettings {
    /// `None` = the user never decided; treated as disabled everywhere.
    pub enabled: Option<bool>,
}

pub fn get_settings(db: &Db) -> AppResult<TelemetrySettings> {
    let conn = db.pool().get()?;
    let enabled = repo::get(&conn, repo::KEY_TELEMETRY_ENABLED)?
        .and_then(|v| v.parse::<bool>().ok());
    Ok(TelemetrySettings { enabled })
}

pub fn set_enabled(db: &Db, enabled: bool) -> AppResult<()> {
    let conn = db.pool().get()?;
    repo::set(&conn, repo::KEY_TELEMETRY_ENABLED, &enabled.to_string())
}

/// Whether the switch is on. Missing setting -> disabled.
pub fn is_enabled(db: &Db) -> AppResult<bool> {
    Ok(get_settings(db)?.enabled.unwrap_or(false))
}

/// Random, stored once, never derived from hardware or account data.
pub fn anonymous_id(db: &Db) -> AppResult<String> {
    let conn = db.pool().get()?;
    if let Some(existing) = repo::get(&conn, repo::KEY_TELEMETRY_ANONYMOUS_ID)? {
        return Ok(existing);
    }
    let id = uuid::Uuid::new_v4().to_string();
    repo::set_if_absent(&conn, repo::KEY_TELEMETRY_ANONYMOUS_ID, &id)?;
    Ok(repo::get(&conn, repo::KEY_TELEMETRY_ANONYMOUS_ID)?.unwrap_or(id))
}

/// In-memory error-category counter for diagnostics. Codes only — never
/// messages that could embed user content.
#[derive(Default)]
pub struct ErrorCategories {
    counts: Mutex<HashMap<String, u64>>,
    enabled: AtomicBool,
}

impl ErrorCategories {
    pub fn record(&self, code: &str) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut map) = self.counts.lock() {
            if map.contains_key(code) || map.len() < 64 {
                *map.entry(code.to_string()).or_insert(0) += 1;
            }
        }
    }

    pub fn snapshot(&self) -> Vec<(String, u64)> {
        let mut list = self
            .counts
            .lock()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>())
            .unwrap_or_default();
        list.sort();
        list
    }

    /// Diagnostics recording is itself telemetry-adjacent; it runs only when
    /// the user opted in.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorCategoryCount {
    pub code: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub app_version: String,
    pub schema_version: i64,
    /// Aggregate error codes only — never plan content, never raw messages.
    pub error_categories: Vec<ErrorCategoryCount>,
}

/// Diagnostics export: version, migration version, error category counts.
/// Free of any planning content by construction (spec: 诊断信息导出).
pub fn export_diagnostics(db: &Db, errors: &ErrorCategories) -> AppResult<Diagnostics> {
    Ok(Diagnostics {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: db.schema_version()?,
        error_categories: errors
            .snapshot()
            .into_iter()
            .map(|(code, count)| ErrorCategoryCount { code, count })
            .collect(),
    })
}

/// Non-blocking ingress + background batching worker.
///
/// * `record` never waits: a full queue drops the event ("不与业务请求争抢资源").
/// * The worker flushes every `flush_interval` or when `batch_limit` events
///   pile up; sink failures are swallowed ("端点不可达 → 静默丢弃").
pub struct Reporter {
    /// `SyncSender` is `Send` but not `Sync`; the mutex only makes the
    /// handle shareable (required for Tauri managed state).
    tx: Mutex<mpsc::SyncSender<TelemetryEvent>>,
    enabled: Arc<AtomicBool>,
}

impl Reporter {
    pub fn spawn(
        sink: Arc<dyn Sink>,
        queue_capacity: usize,
        batch_limit: usize,
        flush_interval: Duration,
    ) -> Self {
        let (tx, rx) = mpsc::sync_channel(queue_capacity.max(1));
        let enabled = Arc::new(AtomicBool::new(false));
        std::thread::Builder::new()
            .name("telemetry".into())
            .spawn(move || {
                let mut batch: Vec<TelemetryEvent> = Vec::with_capacity(batch_limit);
                loop {
                    match rx.recv_timeout(flush_interval) {
                        Ok(event) => {
                            batch.push(event);
                            if batch.len() >= batch_limit {
                                flush(&sink, &mut batch);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => flush(&sink, &mut batch),
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            flush(&sink, &mut batch);
                            break;
                        }
                    }
                }
            })
            .expect("spawn telemetry worker");
        Self { tx: Mutex::new(tx), enabled }
    }

    /// Gate the channel after reading the persisted switch. Recording while
    /// gated is a no-op — zero serialization, zero sends.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Fire-and-forget. Never blocks, never panics, never logs.
    pub fn record(&self, event: TelemetryEvent) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.try_send(event);
        }
    }
}

fn flush(sink: &Arc<dyn Sink>, batch: &mut Vec<TelemetryEvent>) {
    if batch.is_empty() {
        return;
    }
    // Failures are silently dropped: telemetry must never surface an error.
    let _ = sink.send(batch);
    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver, Sender};

    struct RecordingSink(Sender<Vec<TelemetryEvent>>);
    impl Sink for RecordingSink {
        fn send(&self, batch: &[TelemetryEvent]) -> Result<(), String> {
            self.0.send(batch.to_vec()).map_err(|e| e.to_string())
        }
    }

    fn drain(rx: &Receiver<Vec<TelemetryEvent>>) -> Option<Vec<TelemetryEvent>> {
        rx.recv_timeout(Duration::from_millis(300)).ok()
    }

    struct FailingSink;
    impl Sink for FailingSink {
        fn send(&self, _batch: &[TelemetryEvent]) -> Result<(), String> {
            Err("endpoint unreachable".into())
        }
    }

    fn event_with(name: &'static str) -> TelemetryEvent {
        TelemetryEvent { name, count: 1, duration_ms: None }
    }

    #[test]
    fn disabled_reporter_never_sends() {
        let (tx, rx) = channel();
        let reporter = Reporter::spawn(Arc::new(RecordingSink(tx)), 16, 4, Duration::from_millis(10));
        reporter.set_enabled(false);
        reporter.record(event_with("app_launched"));
        assert!(drain(&rx).is_none(), "gated events must not reach the sink");
    }

    #[test]
    fn enabled_reporter_batches_to_sink() {
        let (tx, rx) = channel();
        let reporter = Reporter::spawn(Arc::new(RecordingSink(tx)), 16, 2, Duration::from_millis(10));
        reporter.set_enabled(true);
        reporter.record(event_with("app_launched"));
        let batch = drain(&rx).expect("batch arrives");
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].name, "app_launched");
    }

    #[test]
    fn event_payload_cannot_carry_user_text() {
        // A sensitive title exists in the app; the serialized event must never
        // contain it because no field could hold it.
        let secret = "Salary negotiation with Alice";
        let event = TelemetryEvent::new(EventName::LongTermCycleCreated);
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains(secret));
        assert_eq!(json, r#"{"name":"long_term_cycle_created","count":1,"duration_ms":null}"#);
    }

    #[test]
    fn failing_sink_is_silent() {
        let reporter = Reporter::spawn(Arc::new(FailingSink), 16, 1, Duration::from_millis(10));
        reporter.set_enabled(true);
        for _ in 0..8 {
            reporter.record(event_with("backup_exported"));
        }
        // Nothing to assert beyond "no panic": sink errors are swallowed.
    }

    #[test]
    fn full_queue_drops_instead_of_blocking() {
        let reporter = Reporter::spawn(Arc::new(FailingSink), 1, 10_000, Duration::from_secs(60));
        reporter.set_enabled(true);
        let started = std::time::Instant::now();
        for _ in 0..64 {
            reporter.record(event_with("app_launched"));
        }
        assert!(started.elapsed() < Duration::from_secs(1), "record must never block on a stuck queue");
    }

    #[test]
    fn anonymous_id_is_stable_and_random_looking() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_at(&dir.path().join("t.db")).unwrap();
        let first = anonymous_id(&db).unwrap();
        let second = anonymous_id(&db).unwrap();
        assert_eq!(first, second);
        assert!(uuid::Uuid::parse_str(&first).is_ok(), "a uuid, not a device fingerprint");
    }

    #[test]
    fn missing_switch_means_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_at(&dir.path().join("t.db")).unwrap();
        let settings = get_settings(&db).unwrap();
        assert_eq!(settings.enabled, None);
        assert!(!is_enabled(&db).unwrap());
        set_enabled(&db, true).unwrap();
        assert!(is_enabled(&db).unwrap());
    }

    #[test]
    fn diagnostics_carry_no_content() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_at(&dir.path().join("t.db")).unwrap();
        let errors = ErrorCategories::default();
        errors.set_enabled(true);
        errors.record("validation");
        let diagnostics = export_diagnostics(&db, &errors).unwrap();
        assert_eq!(diagnostics.schema_version, 4);
        assert_eq!(diagnostics.error_categories.len(), 1);
        assert_eq!(diagnostics.error_categories[0].code, "validation");
        let json = serde_json::to_string(&diagnostics).unwrap();
        assert!(!json.contains("title"));
    }
}
