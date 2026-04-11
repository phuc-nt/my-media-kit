//! Typed errors for media-kit. Kept independent of `creator_core::CreatorError`
//! so the caller can decide how to surface them.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg binary not found: {0}")]
    BinaryNotFound(String),

    #[error("failed to spawn ffmpeg: {0}")]
    Spawn(String),

    #[error("ffmpeg exited with status {status}: {stderr}")]
    ExitFailed { status: i32, stderr: String },

    #[error("no audio track found in input")]
    NoAudioTrack,

    #[error("failed to read pcm output: {0}")]
    PcmRead(String),

    #[error("invalid media file: {0}")]
    InvalidMedia(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("operation cancelled")]
    Cancelled,
}
