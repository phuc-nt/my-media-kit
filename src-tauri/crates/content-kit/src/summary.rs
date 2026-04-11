//! Video summary generation — two-pass for long transcripts, one-shot for
//! short ones. Output style picked by the caller (`SummaryStyle`).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, TranscriptionSegment};

use crate::batch::{chunk_segments, TranscriptBatch};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SummaryStyle {
    Brief,
    KeyPoints,
    ActionItems,
}

impl SummaryStyle {
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::Brief => {
                "Write a concise 2-3 paragraph narrative summary. No bullet lists."
            }
            Self::KeyPoints => {
                "Return 5-8 key points as a bullet list. Preserve the original \
                 order of ideas from the transcript."
            }
            Self::ActionItems => {
                "Extract concrete action items the viewer should take. Return \
                 a bullet list; omit items that are too vague to act on."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryResult {
    pub style: SummaryStyle,
    pub language: String,
    pub text: String,
}

pub fn system_prompt(language: &str) -> String {
    format!(
        "You summarize video transcripts. Respond in {language}. Never add \
         information that is not in the transcript. If the transcript is \
         partial, say so honestly."
    )
}

pub fn user_prompt_for_batch(
    batch: &TranscriptBatch,
    style: SummaryStyle,
    language: &str,
) -> String {
    format!(
        "{}\n\nRespond in {language}.\n\n\
         Transcript batch {} (segments {}..{}). Each line is \
         `[start_ms - end_ms] text`:\n\n{}",
        style.instruction(),
        batch.batch_index,
        batch.first_segment_index,
        batch.first_segment_index + batch.segments.len(),
        batch.to_prompt_transcript()
    )
}

pub fn user_prompt_for_consolidation(
    partial_summaries: &[String],
    style: SummaryStyle,
    language: &str,
) -> String {
    format!(
        "Below are partial summaries of the same video. Produce a single \
         final summary in {language} using the style: {}.\n\n{}",
        style.instruction(),
        partial_summaries
            .iter()
            .enumerate()
            .map(|(i, s)| format!("--- partial {i} ---\n{s}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "text": { "type": "string" } },
        "required": ["text"]
    })
}

#[async_trait]
pub trait SummaryRunner {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        style: SummaryStyle,
        language: &str,
        model: &str,
        max_batch_seconds: f64,
    ) -> Result<SummaryResult, AiProviderError>;
}

pub struct ProviderSummaryRunner<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> SummaryRunner for ProviderSummaryRunner<'a> {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        style: SummaryStyle,
        language: &str,
        model: &str,
        max_batch_seconds: f64,
    ) -> Result<SummaryResult, AiProviderError> {
        let batches = chunk_segments(segments, max_batch_seconds);
        if batches.is_empty() {
            return Ok(SummaryResult {
                style,
                language: language.into(),
                text: String::new(),
            });
        }

        // Pass 1: summarize each batch.
        let mut partials = Vec::with_capacity(batches.len());
        for batch in &batches {
            let req = CompletionRequest::structured(
                model,
                system_prompt(language),
                user_prompt_for_batch(batch, style, language),
                "BatchSummary",
                response_schema(),
            );
            let v = self.provider.complete(req).await?;
            let text = v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string();
            partials.push(text);
        }

        if partials.len() == 1 {
            return Ok(SummaryResult {
                style,
                language: language.into(),
                text: partials.into_iter().next().unwrap_or_default(),
            });
        }

        // Pass 2: consolidate into a final summary.
        let req = CompletionRequest::structured(
            model,
            system_prompt(language),
            user_prompt_for_consolidation(&partials, style, language),
            "FinalSummary",
            response_schema(),
        );
        let v = self.provider.complete(req).await?;
        let final_text = v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(SummaryResult {
            style,
            language: language.into(),
            text: final_text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[test]
    fn style_instructions_non_empty() {
        for s in [SummaryStyle::Brief, SummaryStyle::KeyPoints, SummaryStyle::ActionItems] {
            assert!(!s.instruction().is_empty());
        }
    }

    #[test]
    fn consolidation_prompt_numbers_partials() {
        let p = user_prompt_for_consolidation(
            &["first".into(), "second".into()],
            SummaryStyle::Brief,
            "English",
        );
        assert!(p.contains("--- partial 0 ---"));
        assert!(p.contains("--- partial 1 ---"));
    }

    struct CountingStub {
        calls: Mutex<usize>,
        responses: Vec<Value>,
    }

    #[async_trait]
    impl Provider for CountingStub {
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
            let mut c = self.calls.lock().unwrap();
            let idx = *c;
            *c += 1;
            Ok(self
                .responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| json!({"text": ""})))
        }
    }

    #[tokio::test]
    async fn single_batch_skips_consolidation() {
        let stub = CountingStub {
            calls: Mutex::new(0),
            responses: vec![json!({"text": "batch summary"})],
        };
        let runner = ProviderSummaryRunner { provider: &stub };
        let segments = vec![TranscriptionSegment::new(0, 5_000, "short transcript")];
        let result = runner
            .run(&segments, SummaryStyle::Brief, "English", "claude-3", 60.0)
            .await
            .unwrap();
        assert_eq!(result.text, "batch summary");
        assert_eq!(*stub.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn multi_batch_runs_consolidation() {
        let stub = CountingStub {
            calls: Mutex::new(0),
            responses: vec![
                json!({"text": "partial 1"}),
                json!({"text": "partial 2"}),
                json!({"text": "final"}),
            ],
        };
        let runner = ProviderSummaryRunner { provider: &stub };
        let segments = vec![
            TranscriptionSegment::new(0, 60_000, "a"),
            TranscriptionSegment::new(60_000, 120_000, "b"),
        ];
        let result = runner
            .run(&segments, SummaryStyle::Brief, "English", "claude-3", 30.0)
            .await
            .unwrap();
        assert_eq!(result.text, "final");
        assert_eq!(*stub.calls.lock().unwrap(), 3);
    }
}
