//! Provider/model configuration CRUD over `providers.json` (task §1).
//!
//! One BYOK configuration model covers cloud, gateway and local endpoints
//! (design: add-ai-access-and-voice D1): [`ProviderConfig`] carries
//! non-sensitive metadata only; credentials live in the system keychain
//! ([`crate::providers::credentials`]) and never pass through this file.
//!
//! Storage (design D4): `providers.json` sits next to `planner.db` in the app
//! data directory, passed in here as a plain path so the store stays testable
//! without a Tauri handle. Every read and write is serialized by one mutex and
//! every write is atomic (temp file + rename). A corrupt file is backed up as
//! `providers.json.corrupt-{timestamp}` before the store falls back to an
//! empty configuration, so unparseable JSON never wedges the app and the
//! original bytes are never silently destroyed.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Log label shared by this module's diagnostics.
const LOG_MODULE: &str = "providers";

/// Name of the configuration file inside the data directory.
const FILE_NAME: &str = "providers.json";

/// Wire protocol of a provider endpoint; decides endpoint path, auth header
/// and SSE decoding in the sampling layer (design D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    AnthropicMessages,
    OpenaiChatCompletions,
    OpenaiResponses,
}

/// Kinds of content a model accepts. `Text` is mandatory for every model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Text,
    Image,
    Video,
    Pdf,
}

/// Kinds of content a model can emit. Text is the only variant; the field
/// exists so the shape is explicit in storage and on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Text,
}

/// A non-sensitive custom header sent with every request to this provider
/// (e.g. gateway routing hints). Never used for credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraHeader {
    pub name: String,
    pub value: String,
}

/// One model of one provider, with user-declared capability metadata
/// (design D1). Sampling needs `model_id` only; the rest informs compaction
/// decisions and explicit capability downgrades in the agent layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Identifier sent to the API (e.g. `llama3`, `claude-sonnet-4`).
    pub model_id: String,
    /// Informational: used for compaction decisions, not enforced by the
    /// sampler (grow semantics).
    pub context_window: u64,
    pub max_output_tokens: u64,
    /// Must contain [`InputType::Text`].
    pub input_types: Vec<InputType>,
    /// Can only be `[OutputType::Text]`.
    pub output_types: Vec<OutputType>,
    /// User-declared, not runtime-probed (design D1): probing costs a real
    /// billed request and can produce false negatives. Defaults to `true`.
    #[serde(default = "default_supports_tools")]
    pub supports_tools: bool,
}

fn default_supports_tools() -> bool {
    true
}

/// A BYOK provider entry (design D1). Local runtimes are ordinary providers:
/// `http://localhost…` base URLs are valid and an empty credential is simply
/// never stored (design D4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// UUID assigned by the store; stable across updates.
    pub id: String,
    pub name: String,
    /// Scheme included; may carry a path prefix (e.g. `https://host/v1`).
    pub base_url: String,
    pub api_format: ApiFormat,
    #[serde(default)]
    pub extra_headers: Vec<ExtraHeader>,
    pub models: Vec<ModelConfig>,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    /// Retained for forward compatibility only: deletion removes the entry
    /// outright, so this flag carries no behavior today.
    #[serde(default)]
    pub archived: bool,
    /// Last successful real probe of this saved configuration and credential.
    /// Only backend connection commands may establish this marker.
    #[serde(default)]
    pub connection_verified_at: Option<i64>,
}

/// On-disk shape of `providers.json`, also kept as the in-memory state: the
/// active selection lives at the top level next to the provider list (design D4).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ProviderState {
    #[serde(default)]
    active_provider_id: Option<String>,
    #[serde(default)]
    active_model_id: Option<String>,
    #[serde(default)]
    providers: Vec<ProviderConfig>,
}

/// Mutex-guarded owner of `providers.json`.
///
/// All mutating methods persist atomically before committing to the in-memory
/// state, so memory never runs ahead of the file. `ProviderStore` is
/// `Send + Sync` and meant to live in Tauri managed state.
pub struct ProviderStore {
    path: PathBuf,
    state: Mutex<ProviderState>,
}

impl ProviderStore {
    /// Opens the store for `dir` (the app data directory that also holds
    /// `planner.db`). A missing file is an empty configuration; a corrupt file
    /// is backed up beside the original and then treated the same way.
    pub fn load(dir: impl AsRef<Path>) -> AppResult<Self> {
        let path = dir.as_ref().join(FILE_NAME);
        let state = read_state(&path)?;
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    /// Adds a provider, assigning its id (`uuid v4`), `created_at` and
    /// `archived = false`; any caller-supplied values for those fields are
    /// ignored. Returns the stored record.
    pub fn add(&self, provider: ProviderConfig) -> AppResult<ProviderConfig> {
        validate(&provider)?;
        let mut stored = provider;
        stored.id = uuid::Uuid::new_v4().to_string();
        stored.created_at = now_ms();
        stored.archived = false;

        let mut state = self.lock();
        let mut next = state.clone();
        next.providers.push(stored.clone());
        self.commit(&mut state, next)?;
        crate::logging::debug(
            LOG_MODULE,
            &format!("added provider {} ({})", stored.name, stored.id),
        );
        Ok(stored)
    }

    /// Replaces the provider with the same `id`. The id itself and the
    /// original `created_at` are preserved; every other field is overwritten.
    pub fn update(&self, provider: ProviderConfig) -> AppResult<ProviderConfig> {
        validate(&provider)?;
        let mut state = self.lock();
        let created_at = state
            .providers
            .iter()
            .find(|p| p.id == provider.id)
            .map(|p| p.created_at)
            .ok_or_else(|| AppError::not_found("provider", provider.id.clone()))?;

        let mut stored = provider;
        stored.created_at = created_at;
        let mut next = state.clone();
        let slot = next
            .providers
            .iter_mut()
            .find(|p| p.id == stored.id)
            .expect("existence checked above");
        *slot = stored.clone();
        if next.active_provider_id.as_deref() == Some(&stored.id)
            && !stored
                .models
                .iter()
                .any(|m| Some(&m.model_id) == next.active_model_id.as_ref())
        {
            next.active_provider_id = None;
            next.active_model_id = None;
        }
        self.commit(&mut state, next)?;
        crate::logging::debug(
            LOG_MODULE,
            &format!("updated provider {} ({})", stored.name, stored.id),
        );
        Ok(stored)
    }

    /// Removes the provider outright (no archive semantics). Deleting the
    /// active provider also clears the selection so resolution never dangles.
    /// Cascade-deleting its keychain entry is the caller's job (task 2.2).
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let mut state = self.lock();
        if !state.providers.iter().any(|p| p.id == id) {
            return Err(AppError::not_found("provider", id));
        }
        let mut next = state.clone();
        next.providers.retain(|p| p.id != id);
        if next.active_provider_id.as_deref() == Some(id) {
            next.active_provider_id = None;
            next.active_model_id = None;
        }
        self.commit(&mut state, next)?;
        crate::logging::debug(LOG_MODULE, &format!("deleted provider {id}"));
        Ok(())
    }

    /// All providers, in insertion order.
    pub fn list(&self) -> Vec<ProviderConfig> {
        self.lock().providers.clone()
    }

    pub fn get(&self, id: &str) -> Option<ProviderConfig> {
        self.lock().providers.iter().find(|p| p.id == id).cloned()
    }

    /// The stored active selection without an existence check; prefer
    /// [`Self::resolve_active`] when making decisions.
    pub fn active_provider_id(&self) -> Option<String> {
        self.lock().active_provider_id.clone()
    }

    pub fn active_model_id(&self) -> Option<String> {
        self.lock().active_model_id.clone()
    }

    #[cfg(test)]
    pub fn set_active(&self, id: &str) -> AppResult<()> {
        let provider = self
            .get(id)
            .ok_or_else(|| AppError::not_found("provider", id))?;
        self.set_active_model(id, &provider.models[0].model_id)
    }

    /// Persist the exact pair atomically. Browsing/editing never changes it.
    pub fn set_active_model(&self, id: &str, model_id: &str) -> AppResult<()> {
        let mut state = self.lock();
        if !state.providers.iter().any(|p| p.id == id) {
            return Err(AppError::not_found("provider", id));
        }
        if !state
            .providers
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .models
            .iter()
            .any(|m| m.model_id == model_id)
        {
            return Err(AppError::not_found("model", model_id));
        }
        let mut next = state.clone();
        next.active_model_id = Some(model_id.to_string());
        next.active_provider_id = Some(id.to_string());
        self.commit(&mut state, next)?;
        crate::logging::debug(LOG_MODULE, &format!("active provider set to {id}"));
        Ok(())
    }

    /// Clears the active selection. Idempotent.
    pub fn clear_active(&self) -> AppResult<()> {
        let mut state = self.lock();
        if state.active_provider_id.is_none() {
            return Ok(());
        }
        let mut next = state.clone();
        next.active_provider_id = None;
        next.active_model_id = None;
        self.commit(&mut state, next)?;
        crate::logging::debug(LOG_MODULE, "active provider cleared");
        Ok(())
    }

    /// Deterministic resolution (spec: 供应商切换与解析): the active provider
    /// when it still exists, `None` when nothing is active or the stored id
    /// dangles. Never substitutes another provider.
    pub fn resolve_active(&self) -> Option<ProviderConfig> {
        let state = self.lock();
        let id = state.active_provider_id.as_deref()?;
        state.providers.iter().find(|p| p.id == id).cloned()
    }

    pub fn resolve_active_model(&self) -> Option<(ProviderConfig, ModelConfig)> {
        let state = self.lock();
        let provider = state
            .providers
            .iter()
            .find(|p| Some(&p.id) == state.active_provider_id.as_ref())?;
        let model = provider
            .models
            .iter()
            .find(|m| Some(&m.model_id) == state.active_model_id.as_ref())?;
        Some((provider.clone(), model.clone()))
    }

    fn lock(&self) -> MutexGuard<'_, ProviderState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Persists `next` atomically and installs it into the already-locked
    /// state only after the write succeeded.
    fn commit(
        &self,
        state: &mut MutexGuard<'_, ProviderState>,
        next: ProviderState,
    ) -> AppResult<()> {
        let raw = serde_json::to_string_pretty(&next)
            .map_err(|e| AppError::Internal(format!("cannot serialize providers config: {e}")))?;
        // Same directory, fixed name: writes are serialized by the state
        // mutex, so a leftover temp file is simply overwritten next save.
        let tmp = self.path.with_file_name(format!("{FILE_NAME}.tmp"));
        std::fs::write(&tmp, raw)
            .map_err(|e| AppError::Internal(format!("cannot write providers config: {e}")))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| AppError::Internal(format!("cannot replace providers config: {e}")))?;
        **state = next;
        Ok(())
    }
}

/// Reads the file at `path`: a missing file and a corrupt file both yield the
/// empty state; only genuine I/O failures are errors. A corrupt file is
/// renamed to `providers.json.corrupt-{timestamp}` first so nothing is lost.
fn read_state(path: &Path) -> AppResult<ProviderState> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ProviderState::default()),
        Err(e) => {
            return Err(AppError::Internal(format!(
                "cannot read providers config: {e}"
            )))
        }
    };
    match serde_json::from_str(&raw) {
        Ok(state) => Ok(state),
        Err(e) => {
            let backup = path.with_file_name(format!(
                "{FILE_NAME}.corrupt-{}",
                chrono::Utc::now().timestamp_millis()
            ));
            match std::fs::rename(path, &backup) {
                Ok(()) => crate::logging::warn(
                    LOG_MODULE,
                    &format!(
                        "providers config is corrupt ({e}); backed up to {} and starting empty",
                        backup.display()
                    ),
                ),
                Err(rename_err) => crate::logging::warn(
                    LOG_MODULE,
                    &format!(
                        "providers config is corrupt ({e}) and could not be backed up: {rename_err}"
                    ),
                ),
            }
            Ok(ProviderState::default())
        }
    }
}

/// Checks every invariant a provider must hold before it can be stored
/// (task 1.3/1.4). Each rule maps to one stable validation code.
pub(crate) fn validate(provider: &ProviderConfig) -> AppResult<()> {
    if provider.name.trim().is_empty() {
        return Err(AppError::validation(
            "invalid_provider_name",
            "provider name must not be empty",
        ));
    }
    if !is_http_base_url(&provider.base_url) {
        return Err(AppError::validation(
            "invalid_base_url",
            "base_url must be an http(s) URL; local http endpoints are allowed",
        ));
    }
    if provider.models.is_empty() {
        return Err(AppError::validation(
            "provider_needs_model",
            "a provider needs at least one model",
        ));
    }
    let mut model_ids = std::collections::HashSet::new();
    for model in &provider.models {
        if !model_ids.insert(model.model_id.as_str()) {
            return Err(AppError::validation(
                "duplicate_model_id",
                "Model IDs must be unique within a provider.",
            ));
        }
        if model.model_id.trim().is_empty() {
            return Err(AppError::validation(
                "invalid_model_id",
                "model_id must not be empty",
            ));
        }
        if model.context_window == 0 || model.max_output_tokens == 0 {
            return Err(AppError::validation(
                "invalid_model_limits",
                "context_window and max_output_tokens must be positive",
            ));
        }
        if !model.input_types.contains(&InputType::Text) {
            return Err(AppError::validation(
                "invalid_input_types",
                "input_types must include text",
            ));
        }
        if model.output_types.is_empty()
            || model.output_types.iter().any(|t| *t != OutputType::Text)
        {
            return Err(AppError::validation(
                "invalid_output_types",
                "output_types can only be [\"text\"]",
            ));
        }
    }
    for header in &provider.extra_headers {
        if !is_valid_header_name(&header.name) {
            return Err(AppError::validation(
                "invalid_header_name",
                "extra header names must be valid HTTP header names",
            ));
        }
    }
    Ok(())
}

/// `scheme://host[/prefix]` with an http(s) scheme and a whitespace-free
/// remainder. Plain `http` stays legal on purpose: local runtimes
/// (Ollama, LM Studio, llama.cpp, vLLM) are ordinary providers.
fn is_http_base_url(url: &str) -> bool {
    if url.chars().any(char::is_whitespace) {
        return false;
    }
    match url.split_once("://") {
        Some((scheme, rest)) => {
            (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
                && !rest.is_empty()
        }
        None => false,
    }
}

/// RFC 7230 `token` (a non-empty `tchar` run) — enough to reject header
/// injection through names containing spaces, colons or non-ASCII bytes.
fn is_valid_header_name(name: &str) -> bool {
    let is_tchar = |b: u8| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            )
    };
    !name.is_empty() && name.bytes().all(is_tchar)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_model_survives_reload_and_removal_clears_selection() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProviderStore::load(dir.path()).unwrap();
        let mut config = sample_provider();
        let mut second = sample_model();
        second.model_id = "second".into();
        config.models.push(second);
        let saved = store.add(config).unwrap();
        store.set_active_model(&saved.id, "second").unwrap();
        assert!(store.set_active_model(&saved.id, "missing").is_err());
        assert_eq!(store.resolve_active_model().unwrap().1.model_id, "second");
        let reopened = ProviderStore::load(dir.path()).unwrap();
        assert_eq!(
            reopened.resolve_active_model().unwrap().1.model_id,
            "second"
        );
        let mut changed = saved;
        changed.models.pop();
        store.update(changed).unwrap();
        assert!(store.resolve_active_model().is_none());
        assert_eq!(store.active_provider_id(), None);
        assert_eq!(store.active_model_id(), None);
    }

    fn sample_model() -> ModelConfig {
        ModelConfig {
            model_id: "llama3".into(),
            context_window: 8192,
            max_output_tokens: 2048,
            input_types: vec![InputType::Text],
            output_types: vec![OutputType::Text],
            supports_tools: true,
        }
    }

    fn sample_provider() -> ProviderConfig {
        ProviderConfig {
            id: String::new(),
            name: "Local runtime".into(),
            base_url: "http://localhost:11434/v1".into(),
            api_format: ApiFormat::OpenaiChatCompletions,
            extra_headers: vec![],
            models: vec![sample_model()],
            created_at: 0,
            archived: false,
            connection_verified_at: None,
        }
    }

    fn assert_rejected(result: AppResult<ProviderConfig>, code: &str) {
        match result {
            Err(AppError::Validation { code: c, .. }) => assert_eq!(c, code),
            other => panic!("expected validation '{code}', got {other:?}"),
        }
    }

    #[test]
    fn crud_roundtrip_survives_reload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_dir = dir.path().join("用户 providers with spaces");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        let store = ProviderStore::load(&config_dir).expect("load empty");

        assert!(store.list().is_empty());
        let added = store.add(sample_provider()).expect("add");
        assert!(!added.id.is_empty(), "store assigns the id");
        assert_ne!(added.created_at, 0, "store assigns created_at");

        // A fresh store over the same directory sees the write.
        let reopened = ProviderStore::load(&config_dir).expect("reload");
        assert_eq!(reopened.list(), vec![added.clone()]);
        assert_eq!(reopened.get(&added.id), Some(added.clone()));
        assert_eq!(reopened.get("missing"), None);

        let mut edited = added.clone();
        edited.name = "Renamed".into();
        edited.base_url = "https://api.example.com/v1".into();
        let updated = reopened.update(edited).expect("update");
        assert_eq!(updated.name, "Renamed");

        let after_update = ProviderStore::load(&config_dir).expect("reload");
        assert_eq!(after_update.list().len(), 1);
        assert_eq!(after_update.list()[0].name, "Renamed");

        after_update.delete(&updated.id).expect("delete");
        assert!(ProviderStore::load(&config_dir)
            .expect("reload")
            .list()
            .is_empty());
    }

    #[test]
    fn update_keeps_id_and_created_at() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let added = store.add(sample_provider()).expect("add");

        let mut forged = added.clone();
        forged.id = "forged".into();
        // Updating by a foreign id is a not-found, not a move.
        assert!(matches!(
            store.update(forged),
            Err(AppError::NotFound { .. })
        ));

        let mut edited = added.clone();
        edited.created_at = 42;
        let updated = store.update(edited).expect("update");
        assert_eq!(updated.id, added.id);
        assert_eq!(updated.created_at, added.created_at);
    }

    #[test]
    fn update_missing_provider_is_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut provider = sample_provider();
        provider.id = "no-such-id".into();
        assert!(matches!(
            store.update(provider),
            Err(AppError::NotFound { .. })
        ));
    }

    #[test]
    fn accepts_local_http_and_https_base_urls() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut local = sample_provider();
        local.base_url = "http://127.0.0.1:1234".into();
        store.add(local).expect("local http is valid");
        let mut remote = sample_provider();
        remote.base_url = "https://api.example.com/v1".into();
        store.add(remote).expect("https is valid");
    }

    #[test]
    fn rejects_empty_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut provider = sample_provider();
        provider.name = "   ".into();
        assert_rejected(store.add(provider), "invalid_provider_name");
    }

    #[test]
    fn rejects_non_http_base_url() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        for url in ["ftp://example.com", "api.example.com/v1", "https://"] {
            let mut provider = sample_provider();
            provider.base_url = url.into();
            assert_rejected(store.add(provider), "invalid_base_url");
        }
    }

    #[test]
    fn rejects_provider_without_models() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut provider = sample_provider();
        provider.models = vec![];
        assert_rejected(store.add(provider), "provider_needs_model");
    }

    #[test]
    fn rejects_empty_model_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut provider = sample_provider();
        provider.models[0].model_id = " ".into();
        assert_rejected(store.add(provider), "invalid_model_id");
    }

    #[test]
    fn rejects_non_positive_model_limits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut zero_context = sample_provider();
        zero_context.models[0].context_window = 0;
        assert_rejected(store.add(zero_context), "invalid_model_limits");

        let mut zero_output = sample_provider();
        zero_output.models[0].max_output_tokens = 0;
        assert_rejected(store.add(zero_output), "invalid_model_limits");
    }

    #[test]
    fn rejects_input_types_without_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut provider = sample_provider();
        provider.models[0].input_types = vec![InputType::Image];
        assert_rejected(store.add(provider), "invalid_input_types");
    }

    #[test]
    fn rejects_output_types_without_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let mut provider = sample_provider();
        provider.models[0].output_types = vec![];
        assert_rejected(store.add(provider), "invalid_output_types");
    }

    #[test]
    fn rejects_invalid_header_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        for name in ["", "Bad Header", "X-Header:", "høsted"] {
            let mut provider = sample_provider();
            provider.extra_headers = vec![ExtraHeader {
                name: name.into(),
                value: "1".into(),
            }];
            assert_rejected(store.add(provider), "invalid_header_name");
        }
        // A legal token name still passes.
        let mut provider = sample_provider();
        provider.extra_headers = vec![ExtraHeader {
            name: "X-Request-Source".into(),
            value: "planner".into(),
        }];
        store.add(provider).expect("valid header name");
    }

    #[test]
    fn update_runs_the_same_validation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let added = store.add(sample_provider()).expect("add");
        let mut edited = added;
        edited.models = vec![];
        assert_rejected(store.update(edited), "provider_needs_model");
    }

    #[test]
    fn corrupt_file_is_backed_up_and_recoverable() {
        // The recovery path emits a warn into the process-global logger;
        // hold the logging test lock so that line cannot land in (and rotate)
        // a concurrently running logging test's files.
        let _logging = crate::logging::TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(FILE_NAME);
        std::fs::write(&file, "{ this is not json").expect("corrupt file");

        let store = ProviderStore::load(dir.path()).expect("load from corrupt file");
        assert!(store.list().is_empty(), "corrupt file reads as empty");

        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&format!("{FILE_NAME}.corrupt-")))
            .collect();
        assert_eq!(
            backups.len(),
            1,
            "exactly one corrupt backup expected, got {backups:?}"
        );
        assert!(!file.exists(), "original corrupt file was moved away");

        // The store is writable: saving replaces the missing file cleanly.
        let added = store.add(sample_provider()).expect("add after corruption");
        let raw = std::fs::read_to_string(&file).expect("rewritten file");
        assert!(raw.contains(&added.id));
        assert_eq!(
            ProviderStore::load(dir.path()).expect("reload").list(),
            vec![added]
        );
    }

    #[test]
    fn missing_file_is_an_empty_configuration() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        assert!(store.list().is_empty());
        assert_eq!(store.active_provider_id(), None);
        assert_eq!(store.resolve_active(), None);
    }

    #[test]
    fn deleting_active_provider_clears_selection_without_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let first = store.add(sample_provider()).expect("add first");
        let mut second = sample_provider();
        second.name = "Other".into();
        let second = store.add(second).expect("add second");

        store.set_active(&first.id).expect("set active");
        assert_eq!(store.resolve_active(), Some(first.clone()));

        store.delete(&first.id).expect("delete active");
        assert_eq!(store.active_provider_id(), None);
        assert_eq!(
            store.resolve_active(),
            None,
            "must not fall back to another provider"
        );
        assert_eq!(store.list(), vec![second]);

        // The cleared selection is persisted, not just in-memory.
        let reopened = ProviderStore::load(dir.path()).expect("reload");
        assert_eq!(reopened.resolve_active(), None);
    }

    #[test]
    fn dangling_active_id_resolves_to_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(FILE_NAME);
        std::fs::write(
            &file,
            serde_json::to_string_pretty(&serde_json::json!({
                "active_provider_id": "gone-away",
                "providers": [],
            }))
            .expect("json"),
        )
        .expect("write");

        let store = ProviderStore::load(dir.path()).expect("load");
        assert_eq!(store.active_provider_id(), Some("gone-away".into()));
        assert_eq!(store.resolve_active(), None);
    }

    #[test]
    fn set_active_requires_an_existing_provider() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        assert!(matches!(
            store.set_active("missing"),
            Err(AppError::NotFound { .. })
        ));
    }

    #[test]
    fn clear_active_is_idempotent_and_persisted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        let added = store.add(sample_provider()).expect("add");
        store.set_active(&added.id).expect("set active");

        store.clear_active().expect("clear");
        store.clear_active().expect("clear again is fine");
        assert_eq!(store.active_provider_id(), None);
        assert_eq!(
            ProviderStore::load(dir.path())
                .expect("reload")
                .active_provider_id(),
            None
        );
    }

    #[test]
    fn save_writes_complete_json_atomically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ProviderStore::load(dir.path()).expect("load");
        for index in 0..50 {
            let mut provider = sample_provider();
            provider.name = format!("provider-{index}");
            store.add(provider).expect("add");
        }
        let active_id = store.list()[7].id.clone();
        store.set_active(&active_id).expect("set active");

        let raw = std::fs::read_to_string(dir.path().join(FILE_NAME)).expect("read");
        // The full document parses and carries everything that was written.
        let parsed: ProviderState = serde_json::from_str(&raw).expect("complete JSON");
        assert_eq!(parsed.providers.len(), 50);
        assert_eq!(parsed.active_provider_id, Some(active_id));
        // Atomic write leaves no temp file behind.
        assert!(!dir.path().join(format!("{FILE_NAME}.tmp")).exists());
        // Round-trip through serde preserves the values.
        assert_eq!(
            ProviderStore::load(dir.path())
                .expect("reload")
                .list()
                .len(),
            50
        );
    }

    #[test]
    fn serde_uses_the_spec_wire_names() {
        let provider = sample_provider();
        let value = serde_json::to_value(&provider).expect("json");
        assert_eq!(value["api_format"], "openai_chat_completions");
        assert_eq!(value["models"][0]["input_types"][0], "text");
        assert_eq!(value["models"][0]["output_types"][0], "text");

        let mut anthropic = sample_provider();
        anthropic.api_format = ApiFormat::AnthropicMessages;
        anthropic.models[0].input_types = vec![InputType::Text, InputType::Image, InputType::Pdf];
        let value = serde_json::to_value(&anthropic).expect("json");
        assert_eq!(value["api_format"], "anthropic_messages");
        assert_eq!(
            value["models"][0]["input_types"],
            serde_json::json!(["text", "image", "pdf"])
        );

        let mut responses = sample_provider();
        responses.api_format = ApiFormat::OpenaiResponses;
        assert_eq!(
            serde_json::to_value(&responses).expect("json")["api_format"],
            "openai_responses"
        );
    }

    #[test]
    fn supports_tools_defaults_to_true_when_absent() {
        let model: ModelConfig = serde_json::from_str(
            "{\"model_id\":\"m\",\"context_window\":1,\"max_output_tokens\":1,\
             \"input_types\":[\"text\"],\"output_types\":[\"text\"]}",
        )
        .expect("deserialize");
        assert!(model.supports_tools);
    }
}
