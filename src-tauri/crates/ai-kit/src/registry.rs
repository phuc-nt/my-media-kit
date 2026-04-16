//! Provider registry + availability gating.
//!
//! The registry owns concrete `Provider` implementations and answers the
//! "which providers can this user actually call?" question the UI needs to
//! show/hide options.
//!
//! Local providers (MLX, Apple Intelligence) are only registered when the
//! build target is macOS aarch64. On Windows / Linux / Intel Mac the UI
//! will simply not see them.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use creator_core::AiProviderType;

use crate::Provider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: AiProviderType,
    pub display_name: &'static str,
    pub available: bool,
    pub reason: Option<String>,
}

pub struct ProviderRegistry {
    providers: HashMap<AiProviderType, Arc<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        self.providers.insert(provider.provider_type(), provider);
    }

    pub fn get(&self, kind: AiProviderType) -> Option<Arc<dyn Provider>> {
        self.providers.get(&kind).cloned()
    }

    /// True if the running platform can *potentially* host this provider
    /// (before we even check if it's configured).
    pub fn is_supported_on_platform(kind: AiProviderType) -> bool {
        match kind {
            AiProviderType::Claude
            | AiProviderType::OpenAi
            | AiProviderType::Gemini
            | AiProviderType::Ollama
            | AiProviderType::OpenRouter => true,
            AiProviderType::Mlx => cfg!(all(target_os = "macos", target_arch = "aarch64")),
            AiProviderType::AppleIntelligence => {
                cfg!(all(target_os = "macos", target_arch = "aarch64"))
            }
        }
    }

    /// Asynchronously query all registered providers for availability,
    /// plus any platform-supported providers we don't have implementations
    /// for yet so the UI can explain why they're missing.
    pub async fn status_report(&self) -> Vec<ProviderStatus> {
        let mut out = Vec::new();
        for kind in [
            AiProviderType::Claude,
            AiProviderType::OpenAi,
            AiProviderType::Gemini,
            AiProviderType::Ollama,
            AiProviderType::OpenRouter,
            AiProviderType::Mlx,
            AiProviderType::AppleIntelligence,
        ] {
            if !Self::is_supported_on_platform(kind) {
                out.push(ProviderStatus {
                    provider: kind,
                    display_name: kind.display_name(),
                    available: false,
                    reason: Some("not supported on this platform".into()),
                });
                continue;
            }
            match self.providers.get(&kind) {
                Some(p) => {
                    let available = p.is_available().await;
                    out.push(ProviderStatus {
                        provider: kind,
                        display_name: kind.display_name(),
                        available,
                        reason: (!available).then(|| "not configured".into()),
                    });
                }
                None => out.push(ProviderStatus {
                    provider: kind,
                    display_name: kind.display_name(),
                    available: false,
                    reason: Some("not registered".into()),
                }),
            }
        }
        out
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_providers_are_supported_everywhere() {
        assert!(ProviderRegistry::is_supported_on_platform(AiProviderType::Claude));
        assert!(ProviderRegistry::is_supported_on_platform(AiProviderType::OpenAi));
        assert!(ProviderRegistry::is_supported_on_platform(AiProviderType::Gemini));
        assert!(ProviderRegistry::is_supported_on_platform(AiProviderType::Ollama));
        assert!(ProviderRegistry::is_supported_on_platform(AiProviderType::OpenRouter));
    }

    #[test]
    fn mlx_gated_to_macos_arm() {
        let expected = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(
            ProviderRegistry::is_supported_on_platform(AiProviderType::Mlx),
            expected
        );
    }
}
