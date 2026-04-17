//! Viral clip finder — identifies the most engaging segments for short-form
//! platforms (YouTube Shorts, TikTok, Instagram Reels).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, TranscriptionSegment};

use crate::batch::TranscriptBatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViralClip {
    pub start_ms: i64,
    pub end_ms: i64,
    pub hook: String,
    pub caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViralClipList {
    pub clips: Vec<ViralClip>,
}

pub fn system_prompt(language: &str) -> String {
    format!(
        "You are a social media content strategist. Analyze the transcript \
         and find 3-5 segments that would make the best short-form clips \
         (15-60 seconds each) for YouTube Shorts, TikTok, or Reels.\n\n\
         For each clip provide:\n\
         - Precise start and end timestamps (in milliseconds)\n\
         - A hook explaining why this moment is engaging (emotional peak, \
           surprising fact, strong opening, quotable moment)\n\
         - A suggested social media caption\n\n\
         Prioritize: strong hooks, emotional moments, surprising revelations, \
         standalone insights that work without full context.\n\n\
         Respond in {language}."
    )
}

pub fn user_prompt(batch: &TranscriptBatch, language: &str) -> String {
    format!(
        "Respond in {language}. Find the best viral-worthy moments.\n\n\
         Transcript:\n\n{}",
        batch.to_prompt_transcript()
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "clips": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "startMs": { "type": "integer", "minimum": 0 },
                        "endMs":   { "type": "integer", "minimum": 0 },
                        "hook":    { "type": "string" },
                        "caption": { "type": "string" }
                    },
                    "required": ["startMs", "endMs", "hook", "caption"],
                    "additionalProperties": false
                },
                "minItems": 1
            }
        },
        "required": ["clips"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
struct ClipResponse {
    clips: Vec<ClipEntry>,
}

#[derive(Debug, Deserialize)]
struct ClipEntry {
    #[serde(rename = "startMs")]
    start_ms: i64,
    #[serde(rename = "endMs")]
    end_ms: i64,
    hook: String,
    caption: String,
}

#[async_trait]
pub trait ViralClipRunner {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
    ) -> Result<ViralClipList, AiProviderError>;
}

pub struct ProviderViralClipRunner<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> ViralClipRunner for ProviderViralClipRunner<'a> {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
    ) -> Result<ViralClipList, AiProviderError> {
        let batch = TranscriptBatch {
            batch_index: 0,
            first_segment_index: 0,
            segments: segments.to_vec(),
        };
        let req = CompletionRequest::structured(
            model,
            system_prompt(language),
            user_prompt(&batch, language),
            "ViralClipList",
            response_schema(),
        );
        let value = self.provider.complete(req).await?;
        let parsed: ClipResponse =
            serde_json::from_value(value).map_err(|e| AiProviderError::Malformed(e.to_string()))?;

        let mut clips: Vec<ViralClip> = parsed
            .clips
            .into_iter()
            .map(|e| ViralClip {
                start_ms: e.start_ms,
                end_ms: e.end_ms,
                hook: e.hook,
                caption: e.caption,
            })
            .collect();

        clips.sort_by_key(|c| c.start_ms);
        Ok(ViralClipList { clips })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_strict_at_all_levels() {
        let s = response_schema();
        assert_eq!(s["additionalProperties"], false);
        let item = &s["properties"]["clips"]["items"];
        assert_eq!(item["additionalProperties"], false);
        let req = item["required"].as_array().unwrap();
        assert_eq!(req.len(), 4);
    }
}
