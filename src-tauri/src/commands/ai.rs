//! AI provider commands. Provides:
//!   - `ai_provider_status`  : list available providers for the settings UI
//!   - `ai_set_api_key`      : store a key in the OS keyring
//!   - `ai_delete_api_key`   : remove a stored key
//!   - `ai_has_api_key`      : check whether a key exists (without reading it)
//!   - `ai_ping`             : cheap availability ping for a provider
//!
//! Actual AI calls (summary, chapters, filler detection) live behind
//! dedicated feature commands later; this module only handles identity +
//! configuration.

use std::sync::Arc;

use serde::Serialize;
use tauri::command;

use ai_kit::{
    ClaudeProvider, GeminiProvider, KeyringSecretStore, OllamaProvider,
    OpenAiProvider, OpenRouterProvider, Provider, ProviderRegistry, ProviderStatus, SecretStore,
};
use creator_core::AiProviderType;

#[derive(Debug, Serialize)]
pub struct KeyStatus {
    pub provider: AiProviderType,
    pub has_key: bool,
}

#[command]
pub async fn ai_provider_status() -> Vec<ProviderStatus> {
    let registry = build_default_registry();
    registry.status_report().await
}

#[command]
pub fn ai_has_api_key(provider: AiProviderType) -> Result<KeyStatus, String> {
    let store = KeyringSecretStore::new();
    let has = store
        .get(provider)
        .map_err(|e: creator_core::AiProviderError| e.to_string())?
        .is_some();
    Ok(KeyStatus {
        provider,
        has_key: has,
    })
}

#[command]
pub fn ai_set_api_key(provider: AiProviderType, value: String) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("empty api key".into());
    }
    let store = KeyringSecretStore::new();
    store.set(provider, &value).map_err(|e: creator_core::AiProviderError| e.to_string())
}

#[command]
pub fn ai_delete_api_key(provider: AiProviderType) -> Result<(), String> {
    let store = KeyringSecretStore::new();
    store.delete(provider).map_err(|e: creator_core::AiProviderError| e.to_string())
}

#[command]
pub async fn ai_ping(provider: AiProviderType) -> Result<bool, String> {
    let registry = build_default_registry();
    match registry.get(provider) {
        Some(p) => Ok(p.is_available().await),
        None => Err("provider not registered".into()),
    }
}

fn build_default_registry() -> ProviderRegistry {
    let store = KeyringSecretStore::new();
    let mut registry = ProviderRegistry::new();

    if let Some(key) = store.get(AiProviderType::Claude).unwrap_or(None) {
        let p: Arc<dyn Provider> = Arc::new(ClaudeProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::OpenAi).unwrap_or(None) {
        let p: Arc<dyn Provider> = Arc::new(OpenAiProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::Gemini).unwrap_or(None) {
        let p: Arc<dyn Provider> = Arc::new(GeminiProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::OpenRouter).unwrap_or(None) {
        let p: Arc<dyn Provider> = Arc::new(OpenRouterProvider::new(key));
        registry.register(p);
    }
    // Ollama host comes from keyring too (treated as a "secret" for
    // consistency even though it's usually http://localhost:11434).
    let host = store
        .get(AiProviderType::Ollama)
        .unwrap_or(None)
        .unwrap_or_else(|| ai_kit::providers::ollama::DEFAULT_HOST.to_string());
    let p: Arc<dyn Provider> = Arc::new(OllamaProvider::new(host));
    registry.register(p);

    registry
}
