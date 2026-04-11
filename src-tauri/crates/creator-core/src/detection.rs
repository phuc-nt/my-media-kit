//! AI-driven detection outputs. Each detector returns a list of these; the
//! view model translates them into `CutRegion`s tagged with the right
//! `CutReason`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillerDetection {
    pub id: Uuid,
    pub segment_index: usize,
    pub cut_start_ms: i64,
    pub cut_end_ms: i64,
    pub text: String,
    pub filler_words: Vec<String>,
}

impl FillerDetection {
    pub fn new(
        segment_index: usize,
        cut_start_ms: i64,
        cut_end_ms: i64,
        text: impl Into<String>,
        filler_words: Vec<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            segment_index,
            cut_start_ms,
            cut_end_ms,
            text: text.into(),
            filler_words,
        }
    }
}

/// Output of the free-form "AI Prompt" detector — e.g. user says
/// "remove the intro and any mention of the sponsor", the detector returns
/// the matching ranges plus the reason string it produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPromptDetection {
    pub id: Uuid,
    pub segment_index: usize,
    pub cut_start_ms: i64,
    pub cut_end_ms: i64,
    pub text: String,
    pub reason: String,
}

impl AiPromptDetection {
    pub fn new(
        segment_index: usize,
        cut_start_ms: i64,
        cut_end_ms: i64,
        text: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            segment_index,
            cut_start_ms,
            cut_end_ms,
            text: text.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filler_serializes_camel_case() {
        let f = FillerDetection::new(0, 0, 500, "um hello", vec!["um".into()]);
        let v = serde_json::to_value(&f).unwrap();
        assert!(v.get("segmentIndex").is_some());
        assert!(v.get("fillerWords").is_some());
    }

    #[test]
    fn ai_prompt_serializes_camel_case() {
        let d = AiPromptDetection::new(2, 0, 1_000, "welcome everyone", "intro removal");
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["segmentIndex"], 2);
        assert_eq!(v["reason"], "intro removal");
    }
}
