//! Groq Whisper transcription backend.
//!
//! Uploads a pre-extracted audio file to Groq's OpenAI-compatible
//! `/openai/v1/audio/transcriptions` endpoint and parses the `verbose_json`
//! response into `Vec<TranscriptionSegment>` with word-level timestamps.
//!
//! Audio extraction (video → mono MP3) is handled upstream by
//! `media_kit::extract_audio_mp3` before this transcriber is called.
//!
//! One Groq API key covers both this transcriber and the `GroqProvider` LLM
//! in ai-kit — no extra key required.

use std::path::Path;

use serde::Deserialize;
use uuid::Uuid;

use creator_core::{TranscriptionSegment, WordTimestamp};

use crate::transcriber::TranscriptionOptions;

pub const GROQ_TRANSCRIBE_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
pub const DEFAULT_MODEL: &str = "whisper-large-v3-turbo";

pub struct GroqWhisperTranscriber {
    api_key: String,
    client: reqwest::Client,
}

impl GroqWhisperTranscriber {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Transcribe a pre-extracted audio file. The caller is responsible for
    /// providing an audio-only file (e.g. MP3 extracted via `media_kit::extract_audio_mp3`).
    /// `model` defaults to `whisper-large-v3-turbo` when `None`.
    pub async fn transcribe_file(
        &self,
        path: &Path,
        model: Option<&str>,
        options: &TranscriptionOptions,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        let file_bytes = tokio::fs::read(path)
            .await
            .map_err(|e| format!("read file: {e}"))?;

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.mp3")
            .to_string();

        let mime = mime_for_ext(path);
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(filename)
            .mime_str(mime)
            .map_err(|e| format!("mime: {e}"))?;

        let model_str = model.unwrap_or(DEFAULT_MODEL).to_string();

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", model_str)
            .text("response_format", "verbose_json")
            // Request both segment and word granularity so segments carry
            // their own word arrays (easier to map 1-to-1).
            .text("timestamp_granularities[]", "segment")
            .text("timestamp_granularities[]", "word");

        if let Some(lang) = &options.language {
            form = form.text("language", lang.clone());
        }

        let resp = self
            .client
            .post(GROQ_TRANSCRIBE_URL)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("network: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Groq Whisper {status}: {body}"));
        }

        let raw: GroqVerboseResponse =
            resp.json().await.map_err(|e| format!("parse response: {e}"))?;

        Ok(raw.into_segments())
    }
}

// ── Groq verbose_json response types ─────────────────────────────────────────
//
// Groq places ALL word-level timestamps in a top-level `words` array rather
// than nesting them inside each segment. We distribute them to segments by
// matching on time range. If a future API version nests words inside segments,
// we fall back to using those instead.

#[derive(Debug, Deserialize)]
struct GroqVerboseResponse {
    #[serde(default)]
    language: Option<String>,
    /// Top-level flat word list — Groq's actual delivery mechanism.
    #[serde(default)]
    words: Vec<GroqWord>,
    #[serde(default)]
    segments: Vec<GroqSegment>,
}

#[derive(Debug, Deserialize)]
struct GroqSegment {
    start: f64,
    end: f64,
    text: String,
    /// Present only if Groq nests words per-segment (future-proofing).
    #[serde(default)]
    words: Vec<GroqWord>,
}

#[derive(Debug, Deserialize)]
struct GroqWord {
    word: String,
    start: f64,
    end: f64,
    #[serde(default)]
    probability: Option<f64>,
}

impl GroqWord {
    fn to_timestamp(&self) -> WordTimestamp {
        WordTimestamp {
            start_ms: (self.start * 1000.0) as i64,
            end_ms: (self.end * 1000.0) as i64,
            text: self.word.clone(),
            confidence: self.probability.map(|p| p as f32),
        }
    }
}

impl GroqVerboseResponse {
    fn into_segments(self) -> Vec<TranscriptionSegment> {
        let lang = self.language;
        // If the top-level words list is populated, distribute words to
        // segments by time range. Otherwise fall back to per-segment words.
        let use_top_level = !self.words.is_empty();
        let top_words = self.words;

        self.segments
            .into_iter()
            .map(|s| {
                let words: Vec<WordTimestamp> = if use_top_level {
                    // Assign top-level words whose start time falls within
                    // this segment's [start, end) window.
                    top_words
                        .iter()
                        .filter(|w| w.start >= s.start && w.start < s.end)
                        .map(|w| w.to_timestamp())
                        .collect()
                } else {
                    s.words.iter().map(|w| w.to_timestamp()).collect()
                };
                TranscriptionSegment {
                    id: Uuid::new_v4(),
                    start_ms: (s.start * 1000.0) as i64,
                    end_ms: (s.end * 1000.0) as i64,
                    text: s.text.trim().to_string(),
                    words,
                    language: lang.clone(),
                }
            })
            .collect()
    }
}

/// Best-effort MIME type from file extension. Groq is lenient but reqwest
/// needs a valid content-type for the multipart part.
fn mime_for_ext(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("mp3") => "audio/mpeg",
        Some("mp4") | Some("m4a") => "audio/mp4",
        Some("mov") => "video/quicktime",
        Some("wav") => "audio/wav",
        Some("webm") => "audio/webm",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_for_mp4() {
        assert_eq!(mime_for_ext(Path::new("video.mp4")), "audio/mp4");
    }

    #[test]
    fn mime_for_mov() {
        assert_eq!(mime_for_ext(Path::new("clip.mov")), "video/quicktime");
    }

    #[test]
    fn into_segments_distributes_top_level_words() {
        // Groq real behaviour: words at top level, not nested in segments.
        let resp = GroqVerboseResponse {
            language: Some("vi".into()),
            words: vec![
                GroqWord { word: "xin".into(), start: 1.5, end: 2.0, probability: Some(0.95) },
                GroqWord { word: "chào".into(), start: 2.1, end: 2.8, probability: None },
                // word outside segment range — must NOT appear in any segment
                GroqWord { word: "bạn".into(), start: 5.0, end: 5.4, probability: None },
            ],
            segments: vec![GroqSegment {
                start: 1.5,
                end: 3.0,
                text: " xin chào".into(),
                words: vec![], // empty, as Groq returns
            }],
        };
        let segs = resp.into_segments();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_ms, 1500);
        assert_eq!(segs[0].end_ms, 3000);
        assert_eq!(segs[0].text, "xin chào");
        assert_eq!(segs[0].language.as_deref(), Some("vi"));
        // Only the two words within [1.5, 3.0) should be assigned.
        assert_eq!(segs[0].words.len(), 2);
        assert_eq!(segs[0].words[0].start_ms, 1500);
        assert_eq!(segs[0].words[0].end_ms, 2000);
        assert_eq!(segs[0].words[0].confidence, Some(0.95));
        assert_eq!(segs[0].words[1].text, "chào");
        assert!(segs[0].words[1].confidence.is_none());
    }

    #[test]
    fn into_segments_falls_back_to_nested_words_when_top_level_empty() {
        let resp = GroqVerboseResponse {
            language: None,
            words: vec![],
            segments: vec![GroqSegment {
                start: 0.0,
                end: 1.0,
                text: "hello".into(),
                words: vec![GroqWord { word: "hello".into(), start: 0.0, end: 1.0, probability: None }],
            }],
        };
        let segs = resp.into_segments();
        assert_eq!(segs[0].words.len(), 1);
        assert_eq!(segs[0].words[0].text, "hello");
    }
}
