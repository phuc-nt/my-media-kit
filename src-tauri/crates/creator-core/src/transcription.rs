//! Transcription output types. These mirror whisper.cpp / WhisperKit word
//! data but stay serde-friendly and free of the whisper-rs type hierarchy so
//! the frontend and downstream crates can consume them without pulling the
//! whole whisper dependency.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordTimestamp {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    /// Whisper confidence in [0, 1]. `None` when the backend doesn't expose
    /// per-word probabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub id: Uuid,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<WordTimestamp>,
    /// BCP-47 language tag (e.g. "en", "vi"). `None` when auto-detection was
    /// used and the backend didn't surface the choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl TranscriptionSegment {
    pub fn new(start_ms: i64, end_ms: i64, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_ms,
            end_ms,
            text: text.into(),
            words: Vec::new(),
            language: None,
        }
    }

    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_words() {
        let mut seg = TranscriptionSegment::new(0, 1000, "hello world");
        seg.words.push(WordTimestamp {
            start_ms: 0,
            end_ms: 400,
            text: "hello".into(),
            confidence: Some(0.93),
        });
        seg.words.push(WordTimestamp {
            start_ms: 500,
            end_ms: 900,
            text: "world".into(),
            confidence: None,
        });
        let json = serde_json::to_string(&seg).unwrap();
        let back: TranscriptionSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(seg, back);
    }

    #[test]
    fn missing_confidence_skipped_in_output() {
        let w = WordTimestamp {
            start_ms: 0,
            end_ms: 100,
            text: "hi".into(),
            confidence: None,
        };
        let v = serde_json::to_value(&w).unwrap();
        assert!(v.get("confidence").is_none());
    }
}
