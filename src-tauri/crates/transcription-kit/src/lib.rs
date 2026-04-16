//! transcription-kit — Whisper wrapper.
//!
//! Phase 6 scaffold: defines the shared `Transcriber` trait plus a
//! `ModelCatalog` for ggml/gguf whisper models. The real `whisper-rs` backend
//! is wired in a follow-up phase because it requires `cmake` + a toolchain
//! on the build host, which we cannot assume in CI. The trait-first shape
//! means the content-detection pipeline (filler, summary, chapters) can be
//! developed and tested against a fake transcriber today.
//!
//! Flow once wired:
//!
//!   media-kit::extract_pcm_samples → Vec<f32> 16 kHz mono
//!                                  ↓
//!            Transcriber.transcribe(&samples, language) → Vec<TranscriptionSegment>
//!                                  ↓
//!                     creator-core models flow downstream

pub mod catalog;
pub mod transcriber;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod mlx_whisper;

pub use catalog::{ModelCatalog, WhisperModelId};
pub use transcriber::{NullTranscriber, Transcriber, TranscriptionOptions};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use mlx_whisper::{MlxWhisperTranscriber, ProgressCallback};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
