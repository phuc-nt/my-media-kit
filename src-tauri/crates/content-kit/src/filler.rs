//! Filler-word detection. Takes a transcript batch, asks an AI provider to
//! return segments whose `fillerWords` should be cut. The prompt is ported
//! from the v1 Swift implementation with the same language-specific filler
//! lists.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, FillerDetection};

use crate::batch::TranscriptBatch;

pub const EN_FILLERS: &[&str] = &[
    "um",
    "uh",
    "er",
    "ah",
    "hmm",
    "like",
    "you know",
    "i mean",
    "basically",
    "actually",
    "literally",
    "right",
    "so",
    "well",
    "kind of",
    "sort of",
    "anyway",
    "obviously",
];

pub const VI_FILLERS: &[&str] = &[
    "ờ",
    "à",
    "ừm",
    "ừ",
    "thì",
    "mà",
    "kiểu",
    "kiểu như",
    "đại khái",
    "nói chung",
    "thực ra",
    "cơ bản là",
    "nói thật là",
    "ý là",
    "tức là",
    "đúng không",
    "hiểu không",
    "biết không",
];

/// Build the system prompt used for filler detection. Multilingual — we
/// describe both English and Vietnamese filler sets so the model handles
/// mixed-language content (common in VN creator videos).
pub fn system_prompt() -> String {
    format!(
        "You identify filler words and verbal tics in speech transcripts. \
         For each segment you receive, list the filler words you want removed \
         along with the exact millisecond range to cut. Treat these as filler: \
         English — {}. Vietnamese — {}. Only cut obvious fillers; do not touch \
         words that carry meaning in context.",
        EN_FILLERS.join(", "),
        VI_FILLERS.join(", ")
    )
}

/// Build the user prompt for a single transcript batch.
pub fn user_prompt(batch: &TranscriptBatch) -> String {
    format!(
        "Transcript batch {} (segments {}..{}): \n\
         Each line is `[start_ms - end_ms] text`. Return every cut you recommend.\n\n\
         {}",
        batch.batch_index,
        batch.first_segment_index,
        batch.first_segment_index + batch.segments.len(),
        batch.to_prompt_transcript()
    )
}

/// JSON schema for the structured response.
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
                        "fillerWords":  { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["segmentIndex", "cutStartMs", "cutEndMs", "text", "fillerWords"]
                }
            }
        },
        "required": ["detections"]
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FillerResponse {
    detections: Vec<FillerResponseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FillerResponseEntry {
    #[serde(rename = "segmentIndex")]
    segment_index: usize,
    #[serde(rename = "cutStartMs")]
    cut_start_ms: i64,
    #[serde(rename = "cutEndMs")]
    cut_end_ms: i64,
    text: String,
    #[serde(rename = "fillerWords")]
    filler_words: Vec<String>,
}

/// Abstract the call so tests can inject a stub provider without network.
#[async_trait]
pub trait FillerDetector {
    async fn detect(
        &self,
        batch: &TranscriptBatch,
        model: &str,
    ) -> Result<Vec<FillerDetection>, AiProviderError>;
}

pub struct AiFillerDetector<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> FillerDetector for AiFillerDetector<'a> {
    async fn detect(
        &self,
        batch: &TranscriptBatch,
        model: &str,
    ) -> Result<Vec<FillerDetection>, AiProviderError> {
        let req = CompletionRequest::structured(
            model,
            system_prompt(),
            user_prompt(batch),
            "FillerDetections",
            response_schema(),
        );
        let value = self.provider.complete(req).await?;
        let parsed: FillerResponse =
            serde_json::from_value(value).map_err(|e| AiProviderError::Malformed(e.to_string()))?;
        Ok(parsed
            .detections
            .into_iter()
            .map(|e| {
                FillerDetection::new(
                    batch.first_segment_index + e.segment_index,
                    e.cut_start_ms,
                    e.cut_end_ms,
                    e.text,
                    e.filler_words,
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creator_core::TranscriptionSegment;

    fn batch() -> TranscriptBatch {
        TranscriptBatch {
            batch_index: 0,
            first_segment_index: 5,
            segments: vec![TranscriptionSegment::new(0, 1_000, "um hello there")],
        }
    }

    #[test]
    fn system_prompt_mentions_both_languages() {
        let p = system_prompt();
        assert!(p.contains("um"));
        assert!(p.contains("ờ"));
    }

    #[test]
    fn user_prompt_shows_batch_metadata() {
        let p = user_prompt(&batch());
        assert!(p.contains("Transcript batch 0"));
        assert!(p.contains("segments 5..6"));
        assert!(p.contains("[0 - 1000]"));
    }

    #[test]
    fn schema_has_detections_array() {
        let s = response_schema();
        assert_eq!(s["properties"]["detections"]["type"], "array");
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
    async fn detector_offsets_segment_index_by_batch_base() {
        let stub = StubProvider {
            response: json!({
                "detections": [{
                    "segmentIndex": 0,
                    "cutStartMs": 0,
                    "cutEndMs": 200,
                    "text": "um hello",
                    "fillerWords": ["um"]
                }]
            }),
        };
        let detector = AiFillerDetector { provider: &stub };
        let results = detector.detect(&batch(), "claude-3").await.unwrap();
        assert_eq!(results.len(), 1);
        // batch base = 5, so global segment index should also be 5.
        assert_eq!(results[0].segment_index, 5);
        assert_eq!(results[0].cut_end_ms, 200);
        assert_eq!(results[0].filler_words, vec!["um".to_string()]);
    }
}
