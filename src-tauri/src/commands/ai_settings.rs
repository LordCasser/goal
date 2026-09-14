//! AI settings commands (task §4): provider CRUD, activation, keychain key
//! management and the minimal real connection test (design D7).
//!
//! Commands validate argument shapes here and delegate to
//! [`AiSettingsState`] (`providers::service`); they never touch
//! `providers.json` or SQL directly. Summaries carry provider metadata plus
//! exactly two derived booleans — `has_api_key` and `is_active` — and never
//! the key itself (task 4.3: key material crosses the IPC boundary in one
//! direction only, as the input of `save_provider_api_key`).
//!
//! `test_provider_connection` (design D7) sends one minimal real sampling
//! request through the configured provider and reports latency or the
//! classified failure. A classified failure is the command's *result*, not
//! its error: only configuration problems — unknown provider, an unusable
//! provider record or keychain — surface as `Err`.

use std::time::Instant;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::providers::config::ProviderConfig;
use crate::providers::service::{sampling_api_format, AiSettingsState};
use crate::sampling::{
    sample, ApiFormat, MessageRole, SamplingError, SamplingMessage, SamplingRequest, Timeouts,
};

/// Log label for this module's diagnostics (ids and outcomes only, never key
/// material).
const LOG_MODULE: &str = "ai_settings";

/// The fixed one-shot prompt of the connection test (design D7).
const TEST_PROMPT: &str = "Reply with the single word: ok";

/// Tight token cap so the connection test can never cost a real generation.
const TEST_MAX_TOKENS: u64 = 16;

// ---------------------------------------------------------------------------
// IPC payload shapes
// ---------------------------------------------------------------------------

/// One provider as seen by the settings page: every [`ProviderConfig`] field
/// (flattened) plus the two derived booleans. Never contains key material —
/// the key's existence is a boolean, its value never leaves the keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSummary {
    #[serde(flatten)]
    pub provider: ProviderConfig,
    /// Whether a keychain entry exists for this provider; `false` for local
    /// endpoints without credentials (a normal state, design D4).
    pub has_api_key: bool,
    pub is_active: bool,
}

/// Full settings snapshot returned by `get_ai_settings` (task 4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettingsSummary {
    /// The resolved active provider; `None` when nothing is active or the
    /// stored id dangles (resolution never substitutes another provider).
    pub active_provider: Option<ProviderSummary>,
    pub providers: Vec<ProviderSummary>,
    /// The active provider exists. A local endpoint without a key still
    /// counts as available (design D4).
    pub ai_available: bool,
}

/// Outcome of `test_provider_connection` (design D7): latency on success, or
/// the design-D3 classification of the failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_ai_settings(state: State<'_, AiSettingsState>) -> AppResult<AiSettingsSummary> {
    summarize_settings(&state)
}

#[tauri::command]
pub fn save_provider(
    state: State<'_, AiSettingsState>,
    provider: ProviderConfig,
) -> AppResult<ProviderConfig> {
    save_provider_config(&state, provider)
}

#[tauri::command]
pub fn delete_provider(state: State<'_, AiSettingsState>, provider_id: String) -> AppResult<()> {
    delete_provider_entry(&state, &provider_id)
}

#[tauri::command]
pub fn set_active_provider(
    state: State<'_, AiSettingsState>,
    provider_id: String,
) -> AppResult<()> {
    state.store.set_active(&provider_id)
}

#[tauri::command]
pub fn save_provider_api_key(
    state: State<'_, AiSettingsState>,
    provider_id: String,
    api_key: String,
) -> AppResult<()> {
    store_api_key(&state, &provider_id, &api_key)
}

#[tauri::command]
pub fn remove_provider_api_key(
    state: State<'_, AiSettingsState>,
    provider_id: String,
) -> AppResult<()> {
    drop_api_key(&state, &provider_id)
}

/// Sends one minimal real request through the provider (design D7) and
/// returns its outcome. Classified request failures are part of the result;
/// only configuration errors are command errors.
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AiSettingsState>,
    provider_id: String,
) -> AppResult<ConnectionTestResult> {
    test_connection(&state, &provider_id).await
}

// ---------------------------------------------------------------------------
// Command logic (plain functions over `&AiSettingsState`, so tests run
// without a Tauri application)
// ---------------------------------------------------------------------------

/// Builds the settings summary (task 4.1). `has_api_key` is the keychain
/// entry's existence; a keychain that cannot be read reports `false` plus a
/// warning so the page still renders.
fn summarize_settings(settings: &AiSettingsState) -> AppResult<AiSettingsSummary> {
    // `resolve_active` (not the raw stored id) decides availability: a
    // dangling active id means "not configured", never a substitute.
    let active_id = settings.store.resolve_active().map(|p| p.id);
    let providers = settings
        .store
        .list()
        .into_iter()
        .map(|provider| {
            let is_active = active_id.as_deref() == Some(provider.id.as_str());
            let has_api_key = has_api_key(settings, &provider.id);
            ProviderSummary {
                provider,
                has_api_key,
                is_active,
            }
        })
        .collect::<Vec<_>>();
    let active_provider = providers.iter().find(|s| s.is_active).cloned();
    let ai_available = active_provider.is_some();
    Ok(AiSettingsSummary {
        active_provider,
        providers,
        ai_available,
    })
}

/// `save_provider`: an empty id adds (the store assigns id/created_at),
/// anything else updates by id. Validation codes come from the config layer
/// unchanged (task 1.3).
fn save_provider_config(
    settings: &AiSettingsState,
    provider: ProviderConfig,
) -> AppResult<ProviderConfig> {
    if provider.id.trim().is_empty() {
        settings.store.add(provider)
    } else {
        settings.store.update(provider)
    }
}

/// `delete_provider`: config first (which also clears an active selection),
/// then the keychain entry (task 2.2 cascade — no orphan credentials). A
/// `NotFound` from the config step means an earlier attempt already removed
/// the config (e.g. the keychain step failed afterwards); falling through
/// makes the whole command idempotent on retry. A keychain failure returns
/// an error so the frontend can prompt — the config stays deleted either
/// way, and re-deleting completes the cascade.
fn delete_provider_entry(settings: &AiSettingsState, provider_id: &str) -> AppResult<()> {
    match settings.store.delete(provider_id) {
        Ok(()) | Err(AppError::NotFound { .. }) => {}
        Err(error) => return Err(error),
    }
    settings.credentials.delete(provider_id)
}

/// `save_provider_api_key`: an empty key is rejected — "no credential" is
/// expressed by removing the entry, never by storing a blank string (design
/// D4). The provider must exist so the command cannot create an orphan
/// keychain entry.
fn store_api_key(settings: &AiSettingsState, provider_id: &str, api_key: &str) -> AppResult<()> {
    if api_key.trim().is_empty() {
        return Err(AppError::validation(
            "invalid_api_key",
            "an api key must not be empty; remove the key instead of storing an empty one",
        ));
    }
    if settings.store.get(provider_id).is_none() {
        return Err(AppError::not_found("provider", provider_id));
    }
    settings.credentials.save(provider_id, api_key)
}

/// `remove_provider_api_key`: idempotent keychain cleanup for an existing
/// provider.
fn drop_api_key(settings: &AiSettingsState, provider_id: &str) -> AppResult<()> {
    if settings.store.get(provider_id).is_none() {
        return Err(AppError::not_found("provider", provider_id));
    }
    settings.credentials.delete(provider_id)
}

/// `test_provider_connection` core: resolves the provider, loads its
/// credential (a missing key is legal — local endpoints; only an unusable
/// keychain fails) and probes the endpoint (design D7).
async fn test_connection(
    settings: &AiSettingsState,
    provider_id: &str,
) -> AppResult<ConnectionTestResult> {
    let provider = settings
        .store
        .get(provider_id)
        .ok_or_else(|| AppError::not_found("provider", provider_id))?;
    // Defensive: the config layer rejects model-less providers, but a
    // hand-edited providers.json can still contain one.
    let Some(model) = provider.models.first() else {
        return Err(AppError::validation(
            "provider_needs_model",
            "a provider needs at least one model",
        ));
    };
    let api_key = settings.credentials.load(provider_id)?;
    Ok(probe_provider(
        &provider.base_url,
        sampling_api_format(provider.api_format),
        &model.model_id,
        api_key,
    )
    .await)
}

/// Sends the minimal request and consumes the stream until its first event
/// (design D7): any decoded event proves the base URL, key and api format
/// work together; the first classified error becomes the result.
///
/// `base_url` is a parameter (not read from a config) so tests can point the
/// probe at a local fake provider instead of a real endpoint.
async fn probe_provider(
    base_url: &str,
    api_format: ApiFormat,
    model: &str,
    api_key: Option<String>,
) -> ConnectionTestResult {
    let request = SamplingRequest {
        base_url: base_url.to_string(),
        api_format,
        model: model.to_string(),
        api_key,
        messages: vec![SamplingMessage {
            role: MessageRole::User,
            content: TEST_PROMPT.to_string(),
        }],
        tools: Vec::new(),
        max_tokens: Some(TEST_MAX_TOKENS),
    };

    let started = Instant::now();
    match sample(request, Timeouts::default()).await {
        Err(error) => classified_failure(&error),
        Ok(mut stream) => match stream.next().await {
            Some(Ok(_)) => ConnectionTestResult {
                ok: true,
                latency_ms: Some(started.elapsed().as_millis() as u64),
                error_code: None,
                error_message: None,
            },
            Some(Err(error)) => classified_failure(&error),
            None => ConnectionTestResult {
                ok: false,
                latency_ms: None,
                error_code: Some("protocol_error".to_string()),
                error_message: Some("the stream ended without any event".to_string()),
            },
        },
    }
}

/// Maps a classified sampling failure onto the test result via
/// [`SamplingError::to_app_error`], so `error_code`/`error_message` carry
/// exactly the pair an `Err` would have serialized (design D3).
fn classified_failure(error: &SamplingError) -> ConnectionTestResult {
    let AppError::Validation { code, message } = error.to_app_error() else {
        unreachable!("SamplingError::to_app_error always maps to Validation");
    };
    ConnectionTestResult {
        ok: false,
        latency_ms: None,
        error_code: Some(code),
        error_message: Some(message),
    }
}

/// Whether a keychain entry exists for the provider. A keychain read failure
/// is reported as `false` plus a warning (task 2.4 distinguishes the two,
/// but the settings summary must still render); the error text never
/// contains key material.
fn has_api_key(settings: &AiSettingsState, provider_id: &str) -> bool {
    match settings.credentials.load(provider_id) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(error) => {
            crate::logging::warn(
                LOG_MODULE,
                &format!("cannot read keychain state for provider {provider_id}: {error}"),
            );
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::Response;
    use axum::routing::post;
    use axum::Router;

    use super::*;
    use crate::providers::config::{
        ApiFormat as ConfigApiFormat, ExtraHeader, InputType, ModelConfig, OutputType,
    };

    // -- fixtures -----------------------------------------------------------

    fn sample_model() -> ModelConfig {
        ModelConfig {
            model_id: "test-model".into(),
            context_window: 8192,
            max_output_tokens: 2048,
            input_types: vec![InputType::Text],
            output_types: vec![OutputType::Text],
            supports_tools: true,
        }
    }

    fn sample_provider(name: &str, base_url: &str) -> ProviderConfig {
        ProviderConfig {
            id: String::new(),
            name: name.into(),
            base_url: base_url.into(),
            api_format: ConfigApiFormat::OpenaiChatCompletions,
            extra_headers: vec![],
            models: vec![sample_model()],
            created_at: 0,
            archived: false,
        }
    }

    fn settings_in(dir: &std::path::Path) -> AiSettingsState {
        AiSettingsState::load(dir).expect("load ai settings state")
    }

    fn assert_validation<T: std::fmt::Debug>(result: AppResult<T>, code: &str) {
        match result {
            Err(AppError::Validation { code: c, .. }) => assert_eq!(c, code),
            other => panic!("expected validation '{code}', got {other:?}"),
        }
    }

    /// Holds the logging test lock for tests that can reach an Info+ log
    /// write into the process-global logger — the keychain-read warn inside
    /// `has_api_key`/`test_connection` fires only on a locked or unusable
    /// keychain, but when it does it must not rotate a concurrently running
    /// logging test's files.
    fn logging_quiet() -> std::sync::MutexGuard<'static, ()> {
        crate::logging::TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    // -- get_ai_settings (task 4.1) -----------------------------------------

    #[test]
    fn get_ai_settings_empty_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let summary = summarize_settings(&settings).unwrap();
        assert_eq!(summary.providers, vec![]);
        assert_eq!(summary.active_provider, None);
        assert!(!summary.ai_available);
    }

    #[test]
    fn get_ai_settings_lists_providers_and_marks_the_active_one() {
        let _logging = logging_quiet();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let first = save_provider_config(
            &settings,
            sample_provider("Local", "http://127.0.0.1:11434/v1"),
        )
        .unwrap();
        let second = save_provider_config(
            &settings,
            sample_provider("Cloud", "https://api.example.com/v1"),
        )
        .unwrap();
        settings.store.set_active(&second.id).unwrap();

        let summary = summarize_settings(&settings).unwrap();
        assert_eq!(summary.providers.len(), 2);
        assert_eq!(summary.providers[0].provider.id, first.id);
        assert!(!summary.providers[0].is_active);
        assert!(summary.providers[1].is_active);
        assert_eq!(
            summary.active_provider.as_ref().unwrap().provider.id,
            second.id
        );
        assert!(summary.ai_available);
        // No key was ever stored: both report the no-credential state.
        assert!(!summary.providers[0].has_api_key);
        assert!(!summary.providers[1].has_api_key);
    }

    #[test]
    fn summary_serialization_carries_no_key_material() {
        let _logging = logging_quiet();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let added = save_provider_config(
            &settings,
            sample_provider("Keyed name", "https://api.example.com/v1"),
        )
        .unwrap();
        settings.store.set_active(&added.id).unwrap();

        let summary = summarize_settings(&settings).unwrap();
        let json = serde_json::to_string(&summary).unwrap();
        // The only api-key-related key is the boolean state flag: the exact
        // JSON key `"api_key"` (or any value) never appears (task 4.3).
        assert!(!json.contains("\"api_key\""), "leaked key field: {json}");
        assert!(json.contains("\"has_api_key\""));
        assert!(json.contains("\"is_active\""));
        // Round-trips so the frontend can consume the same shape.
        let parsed: AiSettingsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, summary);
    }

    #[test]
    fn provider_summary_flattens_every_config_field() {
        let mut provider = sample_provider("Full", "https://api.example.com/v1");
        provider.extra_headers = vec![ExtraHeader {
            name: "X-Source".into(),
            value: "planner".into(),
        }];
        let summary = ProviderSummary {
            provider,
            has_api_key: true,
            is_active: false,
        };
        let value = serde_json::to_value(&summary).unwrap();
        for key in [
            "id",
            "name",
            "base_url",
            "api_format",
            "extra_headers",
            "models",
            "created_at",
            "archived",
            "has_api_key",
            "is_active",
        ] {
            assert!(value.get(key).is_some(), "missing '{key}' in {value}");
        }
        assert_eq!(value["api_format"], "openai_chat_completions");
        let parsed: ProviderSummary = serde_json::from_value(value).unwrap();
        assert_eq!(parsed, summary);
    }

    // -- save_provider -------------------------------------------------------

    #[test]
    fn save_provider_adds_then_updates() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());

        let added =
            save_provider_config(&settings, sample_provider("A", "http://127.0.0.1:1/v1")).unwrap();
        assert!(!added.id.is_empty(), "the store assigns the id");

        let mut edited = added.clone();
        edited.name = "Renamed".into();
        edited.base_url = "https://api.example.com/v1".into();
        let updated = save_provider_config(&settings, edited).unwrap();
        assert_eq!(updated.id, added.id);
        assert_eq!(updated.name, "Renamed");

        let list = settings.store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].base_url, "https://api.example.com/v1");
    }

    #[test]
    fn save_provider_passes_config_validation_codes_through() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let provider = sample_provider("Bad", "ftp://example.com");
        assert_validation(
            save_provider_config(&settings, provider),
            "invalid_base_url",
        );

        let nameless = sample_provider("", "https://api.example.com");
        assert_validation(
            save_provider_config(&settings, nameless),
            "invalid_provider_name",
        );
        let mut modelless = sample_provider("No models", "https://api.example.com");
        modelless.models = vec![];
        assert_validation(
            save_provider_config(&settings, modelless),
            "provider_needs_model",
        );
    }

    #[test]
    fn save_provider_update_of_missing_id_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let mut ghost = sample_provider("Ghost", "https://api.example.com");
        ghost.id = "no-such-id".into();
        assert!(matches!(
            save_provider_config(&settings, ghost),
            Err(AppError::NotFound { .. })
        ));
    }

    // -- delete_provider -----------------------------------------------------

    #[test]
    fn delete_provider_removes_config_and_clears_activation() {
        let _logging = logging_quiet();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let first = save_provider_config(&settings, sample_provider("First", "http://127.0.0.1:1"))
            .unwrap();
        let second =
            save_provider_config(&settings, sample_provider("Second", "http://127.0.0.1:2"))
                .unwrap();
        settings.store.set_active(&first.id).unwrap();

        delete_provider_entry(&settings, &first.id).unwrap();

        assert_eq!(
            settings
                .store
                .list()
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            vec![second.id.clone()],
            "the provider is gone"
        );
        assert_eq!(settings.store.active_provider_id(), None);
        assert_eq!(
            settings.store.resolve_active(),
            None,
            "must not fall back to another provider (task 1.5)"
        );
        // The summary agrees: not available, nothing active.
        let summary = summarize_settings(&settings).unwrap();
        assert!(!summary.ai_available);
        assert_eq!(summary.active_provider, None);
        assert!(!summary.providers[0].is_active);
    }

    #[test]
    fn delete_provider_is_idempotent_on_retry() {
        // The cascade is config-delete-then-keychain-delete; when the
        // keychain step fails the config is already gone, and retrying the
        // whole command must succeed (task 2.2).
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let added =
            save_provider_config(&settings, sample_provider("Once", "http://127.0.0.1:3")).unwrap();
        delete_provider_entry(&settings, &added.id).unwrap();
        delete_provider_entry(&settings, &added.id).unwrap();
        assert!(settings.store.list().is_empty());
    }

    // -- api key commands (parameter validation; keychain paths are #[ignore])

    #[test]
    fn save_provider_api_key_rejects_empty_strings() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let added = save_provider_config(
            &settings,
            sample_provider("Keyed", "https://api.example.com/v1"),
        )
        .unwrap();
        for key in ["", "   "] {
            assert_validation(store_api_key(&settings, &added.id, key), "invalid_api_key");
        }
    }

    #[test]
    fn api_key_commands_require_an_existing_provider() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        assert!(matches!(
            store_api_key(&settings, "missing", "sk-x"),
            Err(AppError::NotFound { .. })
        ));
        assert!(matches!(
            drop_api_key(&settings, "missing"),
            Err(AppError::NotFound { .. })
        ));
    }

    #[test]
    #[ignore = "requires a usable system keychain (cargo test -- --ignored)"]
    fn api_key_roundtrip_through_commands() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let added = save_provider_config(
            &settings,
            sample_provider("Keyed", "https://api.example.com/v1"),
        )
        .unwrap();

        store_api_key(&settings, &added.id, "sk-command-roundtrip").unwrap();
        assert_eq!(
            settings.credentials.load(&added.id).unwrap().as_deref(),
            Some("sk-command-roundtrip")
        );
        assert!(summarize_settings(&settings).unwrap().providers[0].has_api_key);

        drop_api_key(&settings, &added.id).unwrap();
        assert_eq!(settings.credentials.load(&added.id).unwrap(), None);
        assert!(!summarize_settings(&settings).unwrap().providers[0].has_api_key);

        // Removing again is still success (idempotent).
        drop_api_key(&settings, &added.id).unwrap();
    }

    // -- test_provider_connection (task 4.2) ----------------------------------

    #[tokio::test]
    async fn test_connection_unknown_provider_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        assert!(matches!(
            test_connection(&settings, "no-such-id").await,
            Err(AppError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn test_connection_without_models_reports_validation() {
        // The config layer never stores a model-less provider; a hand-edited
        // providers.json can still contain one, so the command must fail
        // safely instead of panicking on `models[0]`.
        let dir = tempfile::tempdir().unwrap();
        let mut bare = sample_provider("Bare", "http://127.0.0.1:1");
        bare.models = vec![];
        let raw = format!(
            "{{\"providers\":[{}]}}",
            serde_json::to_string(&bare).unwrap()
        );
        std::fs::write(dir.path().join("providers.json"), raw).unwrap();

        let settings = settings_in(dir.path());
        let id = settings.store.list()[0].id.clone();
        assert_validation(
            test_connection(&settings, &id).await,
            "provider_needs_model",
        );
    }

    #[test]
    fn connection_test_result_serialization_shape() {
        let ok = ConnectionTestResult {
            ok: true,
            latency_ms: Some(12),
            error_code: None,
            error_message: None,
        };
        let value = serde_json::to_value(&ok).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "ok": true,
                "latency_ms": 12,
                "error_code": null,
                "error_message": null,
            })
        );
        assert_eq!(
            serde_json::from_value::<ConnectionTestResult>(value).unwrap(),
            ok
        );

        let failed = ConnectionTestResult {
            ok: false,
            latency_ms: None,
            error_code: Some("auth_failed".into()),
            error_message: Some("authentication failed".into()),
        };
        assert_eq!(
            serde_json::to_value(&failed).unwrap(),
            serde_json::json!({
                "ok": false,
                "latency_ms": null,
                "error_code": "auth_failed",
                "error_message": "authentication failed",
            })
        );
    }

    // -- local fake provider (same pattern as sampling::tests) ---------------

    type Captured = Arc<Mutex<Vec<serde_json::Value>>>;

    #[derive(Clone)]
    enum Scenario {
        /// Reply 200 with a fixed SSE payload.
        Sse(&'static str),
        /// Reply with this HTTP status and JSON error body.
        Error(StatusCode, String),
    }

    #[derive(Clone)]
    struct Ctx {
        scenario: Scenario,
        captured: Captured,
    }

    async fn handle(State(ctx): State<Ctx>, body: axum::body::Bytes) -> Response {
        ctx.captured
            .lock()
            .unwrap()
            .push(serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null));
        match ctx.scenario {
            Scenario::Sse(payload) => Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(Body::from(payload))
                .unwrap(),
            Scenario::Error(status, body) => Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        }
    }

    /// Boots the fake provider on a random local port; it serves all three
    /// protocol endpoints, so any api_format can point at one base URL.
    async fn spawn(scenario: Scenario) -> (String, Captured) {
        let captured: Captured = Arc::new(Mutex::new(Vec::new()));
        let ctx = Ctx {
            scenario,
            captured: captured.clone(),
        };
        let app = Router::new()
            .route("/messages", post(handle))
            .route("/chat/completions", post(handle))
            .route("/responses", post(handle))
            .with_state(ctx);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), captured)
    }

    /// One clean minimal chat-completions stream: a single text delta, stop.
    const CHAT_OK: &str = concat!(
        "data: {\"id\":\"t1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n",
        "\n",
        "data: {\"id\":\"t1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
        "\n",
        "data: [DONE]\n",
        "\n",
    );

    #[tokio::test]
    async fn test_connection_succeeds_against_local_fake_provider() {
        let _logging = logging_quiet();
        let (base, captured) = spawn(Scenario::Sse(CHAT_OK)).await;
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let added = save_provider_config(&settings, sample_provider("Fake", &base)).unwrap();

        let result = test_connection(&settings, &added.id).await.unwrap();
        assert!(result.ok, "expected success, got {result:?}");
        assert!(result.latency_ms.is_some());
        assert_eq!(result.error_code, None);
        assert_eq!(result.error_message, None);

        // The request really was the minimal one (design D7): fixed prompt,
        // tight token cap, no tools, the provider's first model.
        let requests = captured.lock().unwrap();
        let body = requests.last().expect("exactly one request arrived");
        assert_eq!(body["model"], serde_json::json!("test-model"));
        assert_eq!(body["max_tokens"], serde_json::json!(16));
        assert_eq!(body["messages"][0]["role"], serde_json::json!("user"));
        assert_eq!(
            body["messages"][0]["content"],
            serde_json::json!("Reply with the single word: ok")
        );
        assert!(body.get("tools").is_none());
        // Local endpoint: no key was stored, so no credential crossed the wire.
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn test_connection_maps_classified_failures_into_the_result() {
        let _logging = logging_quiet();
        let cases = [
            (
                StatusCode::UNAUTHORIZED,
                r#"{"error":{"message":"bad key"}}"#.to_string(),
                "auth_failed",
                "authentication failed",
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                r#"{"error":{"message":"slow down"}}"#.to_string(),
                "rate_limited",
                "rate limited",
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"wrong model"}}"#.to_string(),
                "invalid_request",
                "invalid request: wrong model",
            ),
        ];
        for (status, body, code, message) in cases {
            let (base, _) = spawn(Scenario::Error(status, body)).await;
            let dir = tempfile::tempdir().unwrap();
            let settings = settings_in(dir.path());
            let added = save_provider_config(&settings, sample_provider("Fake", &base)).unwrap();

            // Classified failures are results, not command errors (task 4.2).
            let result = test_connection(&settings, &added.id).await.unwrap();
            assert!(!result.ok, "{status}");
            assert_eq!(result.latency_ms, None);
            assert_eq!(result.error_code.as_deref(), Some(code), "{status}");
            assert_eq!(result.error_message.as_deref(), Some(message), "{status}");
        }
    }

    #[tokio::test]
    async fn test_connection_unreachable_endpoint_is_a_classified_result() {
        let _logging = logging_quiet();
        // Bind then drop a listener so the port is (almost certainly) unused.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let dir = tempfile::tempdir().unwrap();
        let settings = settings_in(dir.path());
        let added = save_provider_config(
            &settings,
            sample_provider("Dead", &format!("http://127.0.0.1:{port}")),
        )
        .unwrap();

        let result = test_connection(&settings, &added.id).await.unwrap();
        assert!(!result.ok);
        assert_eq!(result.error_code.as_deref(), Some("provider_unreachable"));
        assert!(result.error_message.is_some());
    }

    /// Manual probe against a real provider (task 6.2). No assertion network
    /// runs this in CI; run it by hand with:
    /// `PLANNER_TEST_BASE_URL=https://…/v1 PLANNER_TEST_MODEL=… \
    ///  PLANNER_TEST_API_KEY=… cargo test test_connection_real_provider -- --ignored`
    #[tokio::test]
    #[ignore = "needs a real provider; set PLANNER_TEST_BASE_URL, PLANNER_TEST_MODEL and PLANNER_TEST_API_KEY"]
    async fn test_connection_real_provider() {
        let base = std::env::var("PLANNER_TEST_BASE_URL").expect("PLANNER_TEST_BASE_URL");
        let model = std::env::var("PLANNER_TEST_MODEL").expect("PLANNER_TEST_MODEL");
        let api_key = std::env::var("PLANNER_TEST_API_KEY").ok();
        let result = probe_provider(
            &base,
            sampling_api_format(ConfigApiFormat::OpenaiChatCompletions),
            &model,
            api_key,
        )
        .await;
        assert!(result.ok, "real provider test failed: {result:?}");
    }
}
