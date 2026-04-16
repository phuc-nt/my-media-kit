//! Free-form "AI Prompt" cutting. User gives an instruction like
//! "remove the intro and any sponsor mentions"; the detector asks the
//! provider to return matching ranges with a reason string per cut.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, AiPromptDetection};

use crate::batch::TranscriptBatch;

pub fn system_prompt() -> &'static str {
    "You are an editor assistant. Given a transcript and a user instruction, \
     return the millisecond ranges the editor should cut from the source video. \
     Always quote a one-sentence reason in the same language as the instruction. \
     Only return cuts you are confident about; prefer precision over recall."
}

pub fn user_prompt(batch: &TranscriptBatch, instruction: &str) -> String {
    format!(
        "Instruction: {instruction}\n\n\
         Transcript batch {} (segments {}..{}):\n\
         Each line is `[start_ms - end_ms] text`.\n\n\
         {}",
        batch.batch_index,
        batch.first_segment_index,
        batch.first_segment_index + batch.segments.len(),
        batch.to_prompt_transcript()
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "detections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "segmentIndex": { "type": "integer", "minimum": 0 },
                        "cutStartMs":   { "type": "integer", "minimum": 0 },
                        "cutEndMs":     { "type": "integer", "minimum": 0 },
                        "text":         { "type": "string" },
                        "reason":       { "type": "string" }
                    },
                    "required": ["segmentIndex", "cutStartMs", "cutEndMs", "text", "reason"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["detections"],
        "additionalProperties": false
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptResponse {
    detections: Vec<PromptResponseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptResponseEntry {
    #[serde(rename = "segmentIndex")]
    segment_index: usize,
    #[serde(rename = "cutStartMs")]
    cut_start_ms: i64,
    #[serde(rename = "cutEndMs")]
    cut_end_ms: i64,
    text: String,
    reason: String,
}

#[async_trait]
pub trait AiPromptCutter {
    async fn detect(
        &self,
        batch: &TranscriptBatch,
        instruction: &str,
        model: &str,
    ) -> Result<Vec<AiPromptDetection>, AiProviderError>;
}

pub struct ProviderCutter<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> AiPromptCutter for ProviderCutter<'a> {
    async fn detect(
        &self,
        batch: &TranscriptBatch,
        instruction: &str,
        model: &str,
    ) -> Result<Vec<AiPromptDetection>, AiProviderError> {
        let req = CompletionRequest::structured(
            model,
            system_prompt(),
            user_prompt(batch, instruction),
            "PromptCutDetections",
            response_schema(),
        );
        let value = self.provider.complete(req).await?;
        let parsed: PromptResponse =
            serde_json::from_value(value).map_err(|e| AiProviderError::Malformed(e.to_string()))?;
        Ok(parsed
            .detections
            .into_iter()
            .map(|e| {
                AiPromptDetection::new(
                    batch.first_segment_index + e.segment_index,
                    e.cut_start_ms,
                    e.cut_end_ms,
                    e.text,
                    e.reason,
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creator_core::TranscriptionSegment;

    #[test]
    fn user_prompt_includes_instruction_and_transcript() {
        let batch = TranscriptBatch {
            batch_index: 0,
            first_segment_index: 0,
            segments: vec![TranscriptionSegment::new(0, 1_000, "hello sponsor")],
        };
        let p = user_prompt(&batch, "remove any sponsor mentions");
        assert!(p.contains("remove any sponsor mentions"));
        assert!(p.contains("[0 - 1000] hello sponsor"));
    }

    #[test]
    fn schema_contains_reason_field() {
        let s = response_schema();
        assert_eq!(s["properties"]["detections"]["items"]["properties"]["reason"]["type"], "string");
    }
}
