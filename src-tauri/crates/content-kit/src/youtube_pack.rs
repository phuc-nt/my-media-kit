//! YouTube Content Pack — generates title suggestions, description, and tags
//! from a transcript in a single AI call.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, TranscriptionSegment};

use crate::batch::TranscriptBatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YouTubePack {
    pub language: String,
    pub titles: Vec<String>,
    pub description: String,
    pub tags: Vec<String>,
}

pub fn system_prompt(language: &str) -> String {
    format!(
        "You are a YouTube SEO expert. Given a video transcript, generate:\n\
         1. Five catchy title suggestions (hook-style, under 70 chars each)\n\
         2. A full YouTube description (intro paragraph + content overview + \
            call to action, 150-300 words)\n\
         3. 15-20 relevant tags/keywords for SEO\n\n\
         Respond in {language}. Base everything on the actual transcript \
         content — never invent facts."
    )
}

pub fn user_prompt(batch: &TranscriptBatch, language: &str) -> String {
    format!(
        "Respond in {language}.\n\nTranscript (segments {}..{}):\n\n{}",
        batch.first_segment_index,
        batch.first_segment_index + batch.segments.len(),
        batch.to_prompt_transcript()
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "titles": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 3,
                "maxItems": 7
            },
            "description": { "type": "string" },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 5
            }
        },
        "required": ["titles", "description", "tags"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
struct PackResponse {
    titles: Vec<String>,
    description: String,
    tags: Vec<String>,
}

#[async_trait]
pub trait YouTubePackRunner {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
    ) -> Result<YouTubePack, AiProviderError>;
}

pub struct ProviderYouTubePackRunner<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> YouTubePackRunner for ProviderYouTubePackRunner<'a> {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
    ) -> Result<YouTubePack, AiProviderError> {
        let batch = TranscriptBatch {
            batch_index: 0,
            first_segment_index: 0,
            segments: segments.to_vec(),
        };
        let req = CompletionRequest::structured(
            model,
            system_prompt(language),
            user_prompt(&batch, language),
            "YouTubePack",
            response_schema(),
        );
        let value = self.provider.complete(req).await?;
        let parsed: PackResponse =
            serde_json::from_value(value).map_err(|e| AiProviderError::Malformed(e.to_string()))?;

        Ok(YouTubePack {
            language: language.into(),
            titles: parsed.titles,
            description: parsed.description,
            tags: parsed.tags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_requires_all_fields() {
        let s = response_schema();
        let required = s["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "titles"));
        assert!(required.iter().any(|v| v == "description"));
        assert!(required.iter().any(|v| v == "tags"));
        assert_eq!(s["additionalProperties"], false);
    }
}
