//! Duplicate / re-take detection. Identifies re-recorded content: re-takes,
//! false starts, and abandoned phrase restarts. Ported 1:1 from the Swift
//! v1 implementation documented in `_research/reverse-engineering/03-autocut-ai.md`.
//!
//! The AI returns a "duplicates" array where each entry has a keep segment
//! and a list of remove segments. We flatten remove segments into
//! `DuplicateDetection` items (one per cut range) so downstream code can
//! treat them uniformly alongside filler detections.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, DuplicateDetection};

use crate::batch::TranscriptBatch;

pub fn system_prompt() -> &'static str {
    "You are a video editor assistant. Analyze the following transcription and find cases \
     where the speaker re-recorded content (re-takes). These are common when recording \
     without a teleprompter.\n\n\
     Detect THREE types of re-takes:\n\n\
     TYPE 1 — CROSS-SEGMENT RE-TAKES:\n\
     Two or more segments within ~30 seconds of each other with substantially similar \
     wording (>50% word overlap), where the speaker re-recorded the same line. Keep the \
     LAST take and cut earlier ones.\n\n\
     TYPE 2 — MID-SENTENCE FALSE STARTS (within a single segment):\n\
     The speaker starts a sentence, stumbles or isn't satisfied, and immediately restarts \
     within the SAME segment. Cut ONLY the abandoned fragment, keep the completed version.\n\n\
     TYPE 3 — ABANDONED PHRASE RESTART (across segments or within one):\n\
     The speaker says a phrase, pauses or trails off, then restarts with the SAME opening \
     words and continues to completion. Cut the abandoned attempt, keep the completed version.\n\n\
     DO NOT flag:\n\
     - Segments that naturally revisit a topic with different wording\n\
     - Intentional callbacks or references to earlier points\n\
     - Segments more than 60 seconds apart\n\n\
     WORD-LEVEL TIMESTAMPS in the format [startMs:word] are provided per segment. \
     Use them for precise cut points.\n\n\
     For each duplicate group:\n\
     - keepSegmentIndex: the segment containing the best take (usually the LAST one)\n\
     - keepStartMs/keepEndMs: timestamps of the content to keep\n\
     - removeSegments: array of parts to cut, each with segmentIndex, cutStartMs, cutEndMs, text, reason\n\n\
     Rules:\n\
     - Use word-level cutStartMs/cutEndMs for precision\n\
     - Keep the LAST complete version by default\n\
     - The reason must be in the SAME LANGUAGE as the transcript\n\n\
     Return JSON with a \"duplicates\" array. If no duplicates found, return {\"duplicates\": []}"
}

pub fn user_prompt(batch: &TranscriptBatch) -> String {
    format!(
        "Transcript batch {} (segments {}..{}):\n\
         Each line is `[start_ms - end_ms] text`. Find re-takes and false starts.\n\n\
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
            "duplicates": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "keepSegmentIndex": { "type": "integer", "minimum": 0 },
                        "keepStartMs":      { "type": "integer", "minimum": 0 },
                        "keepEndMs":        { "type": "integer", "minimum": 0 },
                        "removeSegments": {
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
                                "required": ["segmentIndex", "cutStartMs", "cutEndMs", "text", "reason"]
                            }
                        }
                    },
                    "required": ["keepSegmentIndex", "keepStartMs", "keepEndMs", "removeSegments"]
                }
            }
        },
        "required": ["duplicates"]
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DuplicateResponse {
    duplicates: Vec<DuplicateGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DuplicateGroup {
    #[serde(rename = "keepSegmentIndex")]
    keep_segment_index: usize,
    #[serde(rename = "keepStartMs")]
    keep_start_ms: i64,
    #[serde(rename = "keepEndMs")]
    keep_end_ms: i64,
    #[serde(rename = "removeSegments")]
    remove_segments: Vec<RemoveSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoveSegment {
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
pub trait DuplicateDetector {
    async fn detect(
        &self,
        batch: &TranscriptBatch,
        model: &str,
    ) -> Result<Vec<DuplicateDetection>, AiProviderError>;
}

pub struct AiDuplicateDetector<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> DuplicateDetector for AiDuplicateDetector<'a> {
    async fn detect(
        &self,
        batch: &TranscriptBatch,
        model: &str,
    ) -> Result<Vec<DuplicateDetection>, AiProviderError> {
        let req = CompletionRequest::structured(
            model,
            system_prompt(),
            user_prompt(batch),
            "DuplicateDetections",
            response_schema(),
        );
        let value = self.provider.complete(req).await?;
        let parsed: DuplicateResponse =
            serde_json::from_value(value).map_err(|e| AiProviderError::Malformed(e.to_string()))?;

        // Flatten all removeSegments across all duplicate groups into a flat list.
        let detections = parsed
            .duplicates
            .into_iter()
            .flat_map(|g| {
                let base = batch.first_segment_index;
                g.remove_segments.into_iter().map(move |r| {
                    DuplicateDetection::new(
                        base + r.segment_index,
                        r.cut_start_ms,
                        r.cut_end_ms,
                        r.text,
                        r.reason,
                    )
                })
            })
            .collect();

        Ok(detections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use creator_core::TranscriptionSegment;
    use serde_json::Value;

    fn batch() -> TranscriptBatch {
        TranscriptBatch {
            batch_index: 0,
            first_segment_index: 2,
            segments: vec![
                TranscriptionSegment::new(0, 3_000, "The new phone is a great phone"),
                TranscriptionSegment::new(4_000, 8_000, "The new phone is a great phone upgrade"),
            ],
        }
    }

    struct StubProvider {
        response: Value,
    }

    #[async_trait]
    impl Provider for StubProvider {
        fn provider_type(&self) -> creator_core::AiProviderType {
            creator_core::AiProviderType::Claude
        }
        async fn is_available(&self) -> bool {
            true
        }
        async fn complete(
            &self,
            _req: CompletionRequest,
        ) -> Result<Value, AiProviderError> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn flattens_remove_segments_and_offsets_index() {
        let stub = StubProvider {
            response: json!({
                "duplicates": [{
                    "keepSegmentIndex": 1,
                    "keepStartMs": 4000,
                    "keepEndMs": 8000,
                    "removeSegments": [{
                        "segmentIndex": 0,
                        "cutStartMs": 0,
                        "cutEndMs": 3000,
                        "text": "The new phone is a great phone",
                        "reason": "Earlier take"
                    }]
                }]
            }),
        };
        let detector = AiDuplicateDetector { provider: &stub };
        let results = detector.detect(&batch(), "claude-3").await.unwrap();
        assert_eq!(results.len(), 1);
        // batch first_segment_index = 2, local index = 0 → global = 2
        assert_eq!(results[0].segment_index, 2);
        assert_eq!(results[0].cut_start_ms, 0);
        assert_eq!(results[0].cut_end_ms, 3000);
        assert_eq!(results[0].reason, "Earlier take");
    }

    #[test]
    fn user_prompt_contains_batch_metadata() {
        let p = user_prompt(&batch());
        assert!(p.contains("Transcript batch 0"));
        assert!(p.contains("segments 2..4"));
    }
}
