//! OpenAI Whisper API transcriber.
//!
//! Sends the audio file to `/v1/audio/transcriptions` with `verbose_json`
//! response format so we get segment-level timestamps. Works on any platform
//! that has an OpenAI API key — the intended fallback for non-Apple-Silicon
//! machines where MLX Whisper is unavailable.
//!
//! Size limit: OpenAI caps uploads at 25 MB. The caller should pre-extract
//! a 32 kbps mono MP3 via `media_kit::extract_audio_mp3` (≈14 MB / hour)
//! before invoking this transcriber.

use creator_core::TranscriptionSegment;
use reqwest::multipart;
use serde::Deserialize;
use std::path::Path;

pub struct OpenAiWhisperTranscriber {
    pub api_key: String,
}

// ── OpenAI verbose_json schema ──────────────────────────────────────────────

#[derive(Deserialize)]
struct VerboseResponse {
    language: Option<String>,
    segments: Vec<OaSegment>,
}

#[derive(Deserialize)]
struct OaSegment {
    start: f64,
    end: f64,
    text: String,
}

// ── impl ────────────────────────────────────────────────────────────────────

impl OpenAiWhisperTranscriber {
    /// Transcribe `audio_path` and return timestamped segments.
    ///
    /// `model` defaults to `"whisper-1"`.
    /// `language` is an optional BCP-47 hint (e.g. `"en"`, `"vi"`).
    pub async fn transcribe(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        let model = model.unwrap_or("whisper-1").to_string();

        let file_bytes = tokio::fs::read(audio_path)
            .await
            .map_err(|e| format!("cannot read audio file: {e}"))?;

        let filename = audio_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.mp3")
            .to_string();

        let part = multipart::Part::bytes(file_bytes)
            .file_name(filename)
            .mime_str("audio/mpeg")
            .map_err(|e| e.to_string())?;

        let mut form = multipart::Form::new()
            .part("file", part)
            .text("model", model)
            .text("response_format", "verbose_json");

        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI Whisper error {status}: {body}"));
        }

        let data: VerboseResponse = resp
            .json()
            .await
            .map_err(|e| format!("failed to parse OpenAI response: {e}"))?;

        let lang_tag = data.language.clone();
        let segments = data
            .segments
            .into_iter()
            .map(|s| {
                let mut seg = TranscriptionSegment::new(
                    (s.start * 1000.0) as i64,
                    (s.end * 1000.0) as i64,
                    s.text.trim(),
                );
                seg.language = lang_tag.clone();
                seg
            })
            .collect();

        Ok(segments)
    }
}
