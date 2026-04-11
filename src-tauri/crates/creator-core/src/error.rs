//! Umbrella error type for the domain layer. Crates that want to surface
//! domain errors to the Tauri boundary convert into `CreatorError` which
//! serializes to a frontend-friendly shape.

use serde::Serialize;
use thiserror::Error;

use crate::ai::AiProviderError;

#[derive(Debug, Error)]
pub enum CreatorError {
    #[error("input/output error: {0}")]
    Io(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("media processing failed: {0}")]
    Media(String),

    #[error("transcription failed: {0}")]
    Transcription(String),

    #[error("silence detection failed: {0}")]
    Silence(String),

    #[error("NLE export failed: {0}")]
    NleExport(String),

    #[error("AI provider error: {0}")]
    Ai(#[from] AiProviderError),

    #[error("operation cancelled")]
    Cancelled,
}

/// Serde-friendly error payload delivered to the frontend. Plain-shape
/// struct so the JS side can switch on `kind` without matching Rust enum
/// variants by discriminant.
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub kind: &'static str,
    pub message: String,
}

impl From<&CreatorError> for ErrorPayload {
    fn from(e: &CreatorError) -> Self {
        let kind = match e {
            CreatorError::Io(_) => "io",
            CreatorError::InvalidArgument(_) => "invalidArgument",
            CreatorError::Media(_) => "media",
            CreatorError::Transcription(_) => "transcription",
            CreatorError::Silence(_) => "silence",
            CreatorError::NleExport(_) => "nleExport",
            CreatorError::Ai(_) => "ai",
            CreatorError::Cancelled => "cancelled",
        };
        Self {
            kind,
            message: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_exposes_kind_tag() {
        let e = CreatorError::Media("ffmpeg missing".into());
        let p = ErrorPayload::from(&e);
        assert_eq!(p.kind, "media");
        assert!(p.message.contains("ffmpeg missing"));
    }

    #[test]
    fn cancelled_maps_correctly() {
        let p = ErrorPayload::from(&CreatorError::Cancelled);
        assert_eq!(p.kind, "cancelled");
    }
}
