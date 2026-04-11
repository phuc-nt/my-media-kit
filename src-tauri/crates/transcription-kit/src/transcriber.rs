//! `Transcriber` trait + null backend. Real backends (whisper-rs, WhisperKit
//! bridge, etc.) conform to the same trait so features can swap at runtime.
//!
//! The trait is async so a backend can stream segments as they finalise;
//! the current return shape is a full `Vec` to keep the first milestone
//! simple. Streaming variant lands alongside the UI work.

use async_trait::async_trait;

use creator_core::{AbortFlag, TranscriptionSegment};

#[derive(Debug, Clone)]
pub struct TranscriptionOptions {
    /// BCP-47 language tag to force, or `None` for auto-detect.
    pub language: Option<String>,
    /// Enable per-word timestamps. Recommended on for AutoCut features.
    pub word_timestamps: bool,
    /// Thread count for the backend. `None` means "backend chooses".
    pub threads: Option<u32>,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self {
            language: None,
            word_timestamps: true,
            threads: None,
        }
    }
}

#[async_trait]
pub trait Transcriber: Send + Sync {
    /// Transcribe a 16 kHz mono f32 PCM buffer. Implementations must honour
    /// `abort` at natural checkpoints and return `Err` with a `Cancelled`
    /// variant when triggered.
    async fn transcribe(
        &self,
        samples: &[f32],
        options: &TranscriptionOptions,
        abort: AbortFlag,
    ) -> Result<Vec<TranscriptionSegment>, String>;
}

/// No-op backend. Handy for UI smoke tests and for the content-detection
/// test suite which wants fixed segments without invoking whisper.
pub struct NullTranscriber {
    fixed: Vec<TranscriptionSegment>,
}

impl NullTranscriber {
    pub fn new(fixed: Vec<TranscriptionSegment>) -> Self {
        Self { fixed }
    }

    pub fn empty() -> Self {
        Self { fixed: Vec::new() }
    }
}

#[async_trait]
impl Transcriber for NullTranscriber {
    async fn transcribe(
        &self,
        _samples: &[f32],
        _options: &TranscriptionOptions,
        abort: AbortFlag,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        if abort.is_aborted() {
            return Err("cancelled".into());
        }
        Ok(self.fixed.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creator_core::WordTimestamp;

    #[tokio::test]
    async fn null_transcriber_returns_fixed_segments() {
        let mut seg = TranscriptionSegment::new(0, 1_000, "hello");
        seg.words.push(WordTimestamp {
            start_ms: 0,
            end_ms: 500,
            text: "hello".into(),
            confidence: Some(0.9),
        });
        let t = NullTranscriber::new(vec![seg.clone()]);
        let result = t
            .transcribe(&[0.0], &TranscriptionOptions::default(), AbortFlag::new())
            .await
            .unwrap();
        assert_eq!(result, vec![seg]);
    }

    #[tokio::test]
    async fn null_transcriber_respects_abort() {
        let t = NullTranscriber::empty();
        let abort = AbortFlag::new();
        abort.abort();
        let r = t
            .transcribe(&[], &TranscriptionOptions::default(), abort)
            .await;
        assert!(r.is_err());
    }
}
