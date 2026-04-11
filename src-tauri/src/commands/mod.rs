//! Tauri command surface — thin wrappers that translate frontend requests
//! into crate calls. Commands must stay free of business logic; delegate to
//! the right kit and surface errors as JSON-friendly strings.
//!
//! Command groups:
//!   - meta      : app version, platform info
//!   - media     : ffmpeg probe + pcm extract
//!   - silence   : RMS + silence detection (runs fully offline)
//!   - ai        : provider registry status, key management, structured
//!                 completions for downstream features
//!   - nle       : FCPXML / xmeml generation

pub mod ai;
pub mod content;
pub mod export;
pub mod media;
pub mod meta;
pub mod nle;
pub mod silence;
pub mod transcription;

pub use ai::*;
pub use content::*;
pub use export::*;
pub use media::*;
pub use meta::*;
pub use nle::*;
pub use silence::*;
pub use transcription::*;
