//! Service glue between provider configuration, credentials and the IPC
//! layer (task §4).
//!
//! [`AiSettingsState`] bundles the two halves of BYOK state (design D4):
//! provider metadata in [`ProviderStore`] (`providers.json` next to
//! `planner.db`) and API keys in the system keychain via [`Credentials`]. It
//! is created once in the `lib.rs` setup hook and lives in Tauri managed
//! state; the commands in `commands::ai_settings` operate on it. Key material
//! never appears in this module's fields or log lines.
//!
//! The two `ApiFormat` enums — `providers::config` (persisted, serde wire
//! shape) and `sampling` (request building) — are deliberately independent;
//! [`sampling_api_format`] is the only bridge between them.

use std::path::Path;

use crate::error::AppResult;
use crate::providers::config::{ApiFormat as ConfigApiFormat, ProviderStore};
use crate::providers::credentials::Credentials;
use crate::sampling::ApiFormat as SamplingApiFormat;

/// Managed state for the AI settings commands (task 4.1).
///
/// Both halves are `Send + Sync` and cheap to share: the store serializes
/// access through its internal mutex, the credentials wrapper is stateless.
pub struct AiSettingsState {
    pub store: ProviderStore,
    pub credentials: Credentials,
    /// Serialize probe/commit and credential changes so stale tests cannot
    /// certify a configuration changed by another settings command.
    pub mutations: futures_util::lock::Mutex<()>,
}

impl AiSettingsState {
    /// Loads the provider store from `dir` (the app data directory that also
    /// holds `planner.db`; a missing or corrupt `providers.json` yields the
    /// empty configuration) and pairs it with the keychain wrapper.
    pub fn load(dir: impl AsRef<Path>) -> AppResult<Self> {
        Ok(Self {
            store: ProviderStore::load(dir)?,
            credentials: Credentials::new(),
            mutations: futures_util::lock::Mutex::new(()),
        })
    }
}

/// Shared by the settings selector and a human-approved Coach action.
/// Callers hold `mutations` while validating and activating the exact pair.
pub fn activate_model(ai: &AiSettingsState, provider_id: &str, model_id: &str) -> AppResult<()> {
    let provider = ai.store.get(provider_id).ok_or_else(|| crate::error::AppError::not_found("provider", provider_id))?;
    if provider.connection_verified_at.is_none() {
        return Err(crate::error::AppError::validation("provider_not_verified", "Test the provider connection before activating it."));
    }
    ai.store.set_active_model(provider_id, model_id)
}

/// Converts the persisted wire-format enum onto the sampling layer's enum.
/// The variants match one to one; this function exists so neither module has
/// to depend on the other (task §4 note: two independent enums).
pub fn sampling_api_format(format: ConfigApiFormat) -> SamplingApiFormat {
    match format {
        ConfigApiFormat::AnthropicMessages => SamplingApiFormat::AnthropicMessages,
        ConfigApiFormat::OpenaiChatCompletions => SamplingApiFormat::OpenaiChatCompletions,
        ConfigApiFormat::OpenaiResponses => SamplingApiFormat::OpenaiResponses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_format_conversion_covers_every_variant() {
        let cases = [
            (ConfigApiFormat::AnthropicMessages, "anthropic_messages"),
            (
                ConfigApiFormat::OpenaiChatCompletions,
                "openai_chat_completions",
            ),
            (ConfigApiFormat::OpenaiResponses, "openai_responses"),
        ];
        for (config_format, label) in cases {
            let converted = sampling_api_format(config_format);
            assert_eq!(converted.label(), label);
            // The wire labels of both enums agree, so provider configuration
            // round-trips the format through sampling unchanged.
            assert_eq!(
                serde_json::to_value(config_format).unwrap(),
                serde_json::to_value(converted).unwrap()
            );
        }
    }

    #[test]
    fn state_load_uses_the_given_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AiSettingsState::load(dir.path()).expect("load");
        assert!(state.store.list().is_empty());

        let provider = crate::providers::config::ProviderConfig {
            id: String::new(),
            name: "State test".into(),
            base_url: "http://127.0.0.1:1234".into(),
            api_format: ConfigApiFormat::OpenaiChatCompletions,
            extra_headers: vec![],
            models: vec![crate::providers::config::ModelConfig {
                model_id: "m".into(),
                context_window: 8,
                max_output_tokens: 8,
                input_types: vec![crate::providers::config::InputType::Text],
                output_types: vec![crate::providers::config::OutputType::Text],
                supports_tools: true,
            }],
            created_at: 0,
            archived: false,
            connection_verified_at: None,
        };
        state.store.add(provider).expect("add");
        assert!(
            dir.path().join("providers.json").exists(),
            "the store writes into the directory the state was loaded with"
        );
    }
}
