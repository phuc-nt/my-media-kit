//! ai-kit — LLM provider abstraction.
//!
//! Exposes a `Provider` trait plus concrete implementations for Claude,
//! OpenAI, Gemini, and Ollama. Local providers (MLX, Apple Intelligence) live
//! behind `#[cfg(all(target_os = "macos", target_arch = "aarch64"))]` and are
//! registered conditionally in the `Registry`.
//!
//! The trait intentionally takes and returns JSON rather than a strongly
//! typed `Request`/`Response` struct — every provider has its own schema
//! format (Claude tools, OpenAI json_schema, Gemini responseSchema, Ollama
//! format) and converting to a single common shape loses information. The
//! shared surface is:
//!
//!   `fn complete(&self, req: CompletionRequest) -> Result<Value, Err>`
//!
//! Callers (features like Summary, Chapters, Filler Detection) build the
//! request with their own prompt + schema and parse the returned JSON into
//! their own types.

pub mod providers;
pub mod registry;
pub mod request;
pub mod secret_store;

pub use providers::{ClaudeProvider, GeminiProvider, OllamaProvider, OpenAiProvider, OpenRouterProvider};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use providers::MlxLmProvider;
pub use registry::{ProviderRegistry, ProviderStatus};
pub use request::{CompletionRequest, ResponseFormat};
pub use secret_store::{InMemorySecretStore, KeyringSecretStore, SecretStore};

use async_trait::async_trait;

use creator_core::{AiProviderError, AiProviderType};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What every provider must implement. Providers are kept object-safe so
/// the registry can stash them in a `HashMap<_, Arc<dyn Provider>>`.
#[async_trait]
pub trait Provider: Send + Sync {
    fn provider_type(&self) -> AiProviderType;

    /// Cheap availability check. Called on app startup + when user changes
    /// settings. Should not make paid API calls; prefer a health ping or
    /// an env/keyring check.
    async fn is_available(&self) -> bool;

    /// Issue a structured-output completion request. Returns the parsed
    /// JSON value the provider produced; caller is responsible for decoding
    /// it into their schema-specific type.
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<serde_json::Value, AiProviderError>;

    /// Optional cleanup, called on shutdown. Default no-op.
    async fn shutdown(&self) {}
}
