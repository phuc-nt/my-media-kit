//! creator-core — Pure-data domain types shared across all CreatorUtils crates.
//!
//! Stays dependency-free of platform-bound crates (no tauri, no ffmpeg, no
//! reqwest). Every other crate depends on this one.

pub mod abort;
pub mod ai;
pub mod cut;
pub mod detection;
pub mod error;
pub mod nle;
pub mod silence;
pub mod time;
pub mod transcription;

pub use abort::AbortFlag;
pub use ai::{AiProviderError, AiProviderType};
pub use cut::{CutReason, CutRegion};
pub use detection::{AiPromptDetection, FillerDetection};
pub use error::CreatorError;
pub use nle::NleExportTarget;
pub use silence::{SilenceDetectorConfig, SilenceRegion};
pub use time::{ms_to_seconds, seconds_to_ms, TimeRangeMs};
pub use transcription::{TranscriptionSegment, WordTimestamp};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
