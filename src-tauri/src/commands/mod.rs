//! Tauri command surface — thin wrappers that translate frontend requests
//! into crate calls. Commands stay free of business logic; delegate to the
//! right kit and surface errors as JSON-friendly strings.
//!
//! Command groups:
//!   - meta         : app version, platform info
//!   - media        : ffmpeg probe
//!   - ai / content : provider keys + structured AI completions
//!   - transcription: whisper (MLX or OpenAI)
//!   - mlx_server   : auto-spawn / probe the local mlx_lm.server
//!   - files/output : save text, scan output dir, read cached results
//!   - youtube      : yt-dlp wrapper

pub mod ai;
pub mod content;
pub mod files;
pub mod media;
pub mod meta;
pub mod mlx_server;
pub mod output;
pub mod transcription;
pub mod youtube;

pub use ai::*;
pub use content::*;
pub use files::*;
pub use media::*;
pub use meta::*;
pub use mlx_server::*;
pub use output::*;
pub use transcription::*;
pub use youtube::*;
