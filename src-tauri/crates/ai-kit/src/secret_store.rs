//! Secret storage backed by the OS keyring.
//!
//! Abstracted behind a trait so tests can inject an in-memory backend and
//! the Tauri command layer can swap in a noop store during headless runs.

use std::sync::Mutex;

use creator_core::{AiProviderError, AiProviderType};

/// Service name used for all keys in the OS keyring.
pub const KEYRING_SERVICE: &str = "tech.lighton.media.MyMediaKit";

pub trait SecretStore: Send + Sync {
    fn get(&self, provider: AiProviderType) -> Result<Option<String>, AiProviderError>;
    fn set(&self, provider: AiProviderType, value: &str) -> Result<(), AiProviderError>;
    fn delete(&self, provider: AiProviderType) -> Result<(), AiProviderError>;
}

/// In-memory store — handy for tests and for CI runs where no keyring is
/// available (e.g. Linux headless). Callers can instantiate directly.
#[derive(Default)]
pub struct InMemorySecretStore {
    inner: Mutex<std::collections::HashMap<AiProviderType, String>>,
}

impl SecretStore for InMemorySecretStore {
    fn get(&self, provider: AiProviderType) -> Result<Option<String>, AiProviderError> {
        Ok(self.inner.lock().unwrap().get(&provider).cloned())
    }

    fn set(&self, provider: AiProviderType, value: &str) -> Result<(), AiProviderError> {
        self.inner
            .lock()
            .unwrap()
            .insert(provider, value.to_string());
        Ok(())
    }

    fn delete(&self, provider: AiProviderType) -> Result<(), AiProviderError> {
        self.inner.lock().unwrap().remove(&provider);
        Ok(())
    }
}

/// OS keyring backend. Uses the `keyring` crate which picks the right
/// backend per platform (macOS: Keychain, Windows: Credential Manager,
/// Linux: Secret Service).
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self
    }

    fn account_for(provider: AiProviderType) -> &'static str {
        match provider {
            AiProviderType::Claude => "ai.provider.claude.apiKey",
            AiProviderType::OpenAi => "ai.provider.openai.apiKey",
            AiProviderType::Gemini => "ai.provider.gemini.apiKey",
            AiProviderType::Ollama => "ai.provider.ollama.host",
            AiProviderType::OpenRouter => "ai.provider.openrouter.apiKey",
            AiProviderType::Mlx => "ai.provider.mlx.modelPath",
            AiProviderType::AppleIntelligence => "ai.provider.appleIntelligence.token",
        }
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, provider: AiProviderType) -> Result<Option<String>, AiProviderError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, Self::account_for(provider))
            .map_err(|e| AiProviderError::Rejected(format!("keyring open: {e}")))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AiProviderError::Rejected(format!("keyring read: {e}"))),
        }
    }

    fn set(&self, provider: AiProviderType, value: &str) -> Result<(), AiProviderError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, Self::account_for(provider))
            .map_err(|e| AiProviderError::Rejected(format!("keyring open: {e}")))?;
        entry
            .set_password(value)
            .map_err(|e| AiProviderError::Rejected(format!("keyring write: {e}")))
    }

    fn delete(&self, provider: AiProviderType) -> Result<(), AiProviderError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, Self::account_for(provider))
            .map_err(|e| AiProviderError::Rejected(format!("keyring open: {e}")))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AiProviderError::Rejected(format!("keyring delete: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_round_trip() {
        let s = InMemorySecretStore::default();
        assert!(s.get(AiProviderType::Claude).unwrap().is_none());
        s.set(AiProviderType::Claude, "sk-ant-123").unwrap();
        assert_eq!(s.get(AiProviderType::Claude).unwrap().as_deref(), Some("sk-ant-123"));
        s.delete(AiProviderType::Claude).unwrap();
        assert!(s.get(AiProviderType::Claude).unwrap().is_none());
    }

    #[test]
    fn accounts_distinct_per_provider() {
        use std::collections::HashSet;
        let accounts: HashSet<&str> = [
            AiProviderType::Claude,
            AiProviderType::OpenAi,
            AiProviderType::Gemini,
            AiProviderType::Ollama,
            AiProviderType::OpenRouter,
            AiProviderType::Mlx,
            AiProviderType::AppleIntelligence,
        ]
        .into_iter()
        .map(KeyringSecretStore::account_for)
        .collect();
        assert_eq!(accounts.len(), 7);
    }
}
