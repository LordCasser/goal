//! Resolving the active BYOK provider into everything a request needs
//! (task §1.3).
//!
//! [`resolve`] is the only path from AI settings to a usable provider: no
//! active selection means the AI features are off with a stable error code
//! the frontend can map to the settings route; a configured-but-unusable
//! keychain is an `internal` failure, never silently "no key". `Ok(None)`
//! from the keychain is a normal state — local runtimes and keyless gateways
//! never store a credential (spec: 供应商切换与解析, 凭据安全存储).

use crate::providers::config::{ModelConfig, ProviderConfig};
use crate::providers::service::AiSettingsState;

use super::AgentError;

/// The active provider, the model to call and the facts the request builder
/// needs. `tools_supported` reflects the chosen model's user-declared
/// capability; the agent layer degrades to conversation mode when it is
/// `false` (design D1, task §1.3).
#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub config: ProviderConfig,
    pub model: ModelConfig,
    pub api_key: Option<String>,
    pub tools_supported: bool,
}

/// Resolves the active provider from the managed AI settings state.
///
/// Errors: [`AgentError::NoActiveProvider`] when nothing is active (or the
/// stored selection dangles — the store never substitutes another provider),
/// [`AgentError::Internal`] when the keychain cannot be read (as opposed to
/// holding no entry, which is fine).
pub fn resolve(ai: &AiSettingsState) -> Result<ResolvedProvider, AgentError> {
    let provider = ai
        .store
        .resolve_active()
        .ok_or(AgentError::NoActiveProvider)?;
    let api_key = ai
        .credentials
        .load(&provider.id)
        .map_err(|e| AgentError::Internal(format!("cannot read provider credentials: {e}")))?;
    // Ok(None): no credential stored — allowed for local endpoints and
    // keyless gateways, so absence is deliberately not an error here.
    resolve_provider(provider, api_key)
}

/// Pure core of the resolution: picks the model and derives the tool flag.
/// Separated from [`resolve`] so model selection is testable without a
/// keychain.
fn resolve_provider(
    provider: ProviderConfig,
    api_key: Option<String>,
) -> Result<ResolvedProvider, AgentError> {
    // Prefer the first tool-capable model; fall back to the first model and
    // report `tools_supported = false` so the caller can degrade instead of
    // sending a request the provider would reject.
    let model = provider
        .models
        .iter()
        .find(|m| m.supports_tools)
        .cloned()
        .or_else(|| provider.models.first().cloned())
        .ok_or_else(|| {
            AgentError::Internal(format!("active provider '{}' has no models", provider.name))
        })?;
    let tools_supported = model.supports_tools;
    Ok(ResolvedProvider {
        config: provider,
        model,
        api_key,
        tools_supported,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// Writes `providers.json` by hand into `dir` — the documented on-disk
    /// shape — with one provider of `models` and an optional active marker.
    fn write_providers_json(dir: &Path, active: bool, models: serde_json::Value) {
        let doc = serde_json::json!({
            "active_provider_id": if active { Some("p1") } else { None },
            "providers": [{
                "id": "p1",
                "name": "Test gateway",
                "base_url": "http://127.0.0.1:9/v1",
                "api_format": "openai_chat_completions",
                "extra_headers": [],
                "models": models,
                "created_at": 0,
                "archived": false,
            }],
        });
        std::fs::write(
            dir.join("providers.json"),
            serde_json::to_string_pretty(&doc).unwrap(),
        )
        .unwrap();
    }

    fn model_json(id: &str, supports_tools: bool) -> serde_json::Value {
        serde_json::json!({
            "model_id": id,
            "context_window": 8192,
            "max_output_tokens": 2048,
            "input_types": ["text"],
            "output_types": ["text"],
            "supports_tools": supports_tools,
        })
    }

    fn model(id: &str, supports_tools: bool) -> ModelConfig {
        serde_json::from_value(model_json(id, supports_tools)).unwrap()
    }

    #[test]
    fn resolve_without_active_provider_is_no_active_provider() {
        let dir = tempfile::tempdir().unwrap();
        write_providers_json(dir.path(), false, vec![model_json("m", true)].into());
        let ai = AiSettingsState::load(dir.path()).unwrap();

        let error = resolve(&ai).unwrap_err();
        assert_eq!(error, AgentError::NoActiveProvider);
        assert_eq!(error.code(), "no_active_provider");
    }

    #[test]
    fn resolve_without_any_provider_is_no_active_provider() {
        let dir = tempfile::tempdir().unwrap();
        let ai = AiSettingsState::load(dir.path()).unwrap();
        assert_eq!(resolve(&ai).unwrap_err(), AgentError::NoActiveProvider);
    }

    #[test]
    fn resolve_prefers_the_first_tool_capable_model() {
        let dir = tempfile::tempdir().unwrap();
        write_providers_json(
            dir.path(),
            true,
            vec![model_json("small", false), model_json("big", true)].into(),
        );
        let ai = AiSettingsState::load(dir.path()).unwrap();

        let resolved = resolve(&ai).unwrap();
        assert_eq!(resolved.config.id, "p1");
        assert_eq!(resolved.model.model_id, "big");
        assert!(resolved.tools_supported);
        assert_eq!(
            resolved.api_key, None,
            "no keychain entry was ever stored for p1"
        );
    }

    #[test]
    fn selection_falls_back_to_the_first_model_when_none_supports_tools() {
        let provider = ProviderConfig {
            id: "p1".into(),
            name: "Local".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            api_format: crate::providers::config::ApiFormat::AnthropicMessages,
            extra_headers: vec![],
            models: vec![model("a", false), model("b", false)],
            created_at: 0,
            archived: false,
        };
        let resolved = resolve_provider(provider, None).unwrap();
        assert_eq!(resolved.model.model_id, "a");
        assert!(!resolved.tools_supported, "the caller must degrade");
    }

    #[test]
    fn selection_prefers_tools_even_when_a_later_model_is_bigger() {
        let provider = ProviderConfig {
            id: "p1".into(),
            name: "Local".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            api_format: crate::providers::config::ApiFormat::AnthropicMessages,
            extra_headers: vec![],
            models: vec![model("capable", true), model("first", false)],
            created_at: 0,
            archived: false,
        };
        let resolved = resolve_provider(provider, Some("sk-x".into())).unwrap();
        assert_eq!(resolved.model.model_id, "capable");
        assert!(resolved.tools_supported);
        assert_eq!(resolved.api_key.as_deref(), Some("sk-x"));
    }

    #[test]
    fn selection_rejects_a_model_less_provider_defensively() {
        let provider = ProviderConfig {
            id: "p1".into(),
            name: "Empty".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            api_format: crate::providers::config::ApiFormat::AnthropicMessages,
            extra_headers: vec![],
            models: vec![],
            created_at: 0,
            archived: false,
        };
        // The store rejects this shape on write; resolution still answers
        // with a typed error instead of panicking.
        let error = resolve_provider(provider, None).unwrap_err();
        assert!(matches!(error, AgentError::Internal(_)));
        assert_eq!(error.code(), "internal");
    }
}
