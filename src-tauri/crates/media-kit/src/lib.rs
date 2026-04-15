//! media-kit — Media I/O wrappers around an ffmpeg sidecar binary.
//!
//! Why ffmpeg, not a rust-native demuxer: ffmpeg already handles every codec
//! creators throw at it (H.264, HEVC, ProRes, AV1, AAC, MP3, WAV, MOV, MP4,
//! MKV, WEBM). Rolling our own via `symphonia` / `ac-ffmpeg` re-solves a
//! solved problem and still needs the user to install system codecs.
//!
//! Binary resolution order:
//!   1. `FFMPEG` / `FFPROBE` env vars (dev override)
//!   2. bundled sidecar path (release builds, resolved by the Tauri app)
//!   3. `PATH` via `which`
//!
//! All command builders stay **pure** (take arguments, return `Vec<String>`)
//! so they can be unit-tested without ffmpeg installed. The actual process
//! execution lives behind `async fn run_*` helpers that use tokio.

pub mod error;
pub mod ffmpeg;
pub mod probe;
pub mod wav;

pub use error::MediaError;
pub use ffmpeg::{
    build_cut_and_concat_args, build_extract_pcm_args, build_probe_duration_args,
    resolve_ffmpeg_binary, resolve_ffprobe_binary, FfmpegBinary,
};
pub use probe::{cut_and_concat, extract_pcm_samples, probe_media, probe_media_full, MediaProbe, MediaProbeFull};
pub use wav::parse_wav_f32_mono;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const TARGET_SAMPLE_RATE: u32 = 16_000;
pub const TARGET_CHANNELS: u16 = 1;
