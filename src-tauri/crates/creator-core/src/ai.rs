//! AI provider identity + error envelope. Keeps the type surface minimal;
//! the actual provider trait and impls live in ai-kit.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Set of providers the UI knows about. Availability is determined at runtime
/// by ai-kit's `Registry`, which consults platform cfg + reachability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiProviderType {
    Claude,
    OpenAi,
    Gemini,
    Ollama,
    /// OpenRouter — routes to 300+ models via OpenAI-compatible API.
    OpenRouter,
    /// MLX (Apple Silicon only). Gated at runtime by ai-kit.
    Mlx,
    /// Apple Intelligence (macOS 26+ Silicon only). Gated at runtime.
    AppleIntelligence,
}

impl AiProviderType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude (Anthropic)",
            Self::OpenAi => "OpenAI",
            Self::Gemini => "Gemini (Google)",
            Self::Ollama => "Ollama (local)",
            Self::OpenRouter => "OpenRouter",
            Self::Mlx => "MLX (local, Apple Silicon)",
            Self::AppleIntelligence => "Apple Intelligence (macOS 26+)",
        }
    }

    /// True if this provider stores a secret key in the OS keyring. Local
    /// providers skip the keyring entirely.
    pub fn uses_api_key(&self) -> bool {
        matches!(self, Self::Claude | Self::OpenAi | Self::Gemini | Self::OpenRouter)
    }
}

#[derive(Debug, Error)]
pub enum AiProviderError {
    #[error("provider {0:?} is not available on this platform")]
    NotAvailable(AiProviderType),
    #[error("provider {0:?} missing API key")]
    MissingApiKey(AiProviderType),
    #[error("network error: {0}")]
    Network(String),
    #[error("provider returned malformed response: {0}")]
    Malformed(String),
    #[error("provider exceeded context window (needed {needed}, max {max})")]
    ContextOverflow { needed: usize, max: usize },
    #[error("provider rejected request: {0}")]
    Rejected(String),
    #[error("request cancelled")]
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_flags_only_cloud_providers() {
        assert!(AiProviderType::Claude.uses_api_key());
        assert!(AiProviderType::OpenAi.uses_api_key());
        assert!(AiProviderType::Gemini.uses_api_key());
        assert!(AiProviderType::OpenRouter.uses_api_key());
        assert!(!AiProviderType::Ollama.uses_api_key());
        assert!(!AiProviderType::Mlx.uses_api_key());
        assert!(!AiProviderType::AppleIntelligence.uses_api_key());
    }

    #[test]
    fn openrouter_serializes_as_camel_case() {
        let s = serde_json::to_string(&AiProviderType::OpenRouter).unwrap();
        assert_eq!(s, "\"openRouter\"");
    }

    #[test]
    fn display_names_non_empty() {
        for p in [
            AiProviderType::Claude,
            AiProviderType::OpenAi,
            AiProviderType::Gemini,
            AiProviderType::Ollama,
            AiProviderType::OpenRouter,
            AiProviderType::Mlx,
            AiProviderType::AppleIntelligence,
        ] {
            assert!(!p.display_name().is_empty());
        }
    }
}
