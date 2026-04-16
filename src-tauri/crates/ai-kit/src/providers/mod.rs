//! Concrete provider implementations.
//!
//! Each provider lives in its own file, conforms to the `Provider` trait in
//! the crate root, and wraps a `reqwest::Client` for HTTP calls. Local
//! providers (MLX, Apple Intelligence) will land in `mlx.rs` /
//! `apple_intelligence.rs` under `#[cfg]` gates.

pub mod claude;
pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod openrouter;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod mlx_lm;

pub use claude::ClaudeProvider;
pub use gemini::GeminiProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use openrouter::OpenRouterProvider;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use mlx_lm::MlxLmProvider;
