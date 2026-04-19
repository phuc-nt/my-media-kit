//! Chapter extraction for YouTube-style descriptions. Single-pass MVP:
//! given a transcript + desired language, ask the provider to return
//! ordered chapters with `timestamp_ms` + `title`. v1's 3-pass flow is
//! overkill for videos under ~90 minutes and we can add it later if QA
//! shows chapter drift on very long content.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, TranscriptionSegment};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub timestamp_ms: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterList {
    pub language: String,
    pub chapters: Vec<Chapter>,
}

pub fn system_prompt(language: &str, summary_hint: Option<&str>) -> String {
    let hint = summary_hint
        .filter(|h| !h.trim().is_empty())
        .map(|h| format!("\nVideo summary for context: {h}"))
        .unwrap_or_default();
    format!(
        "You create YouTube chapter lists. Respond in {language}. First \
         chapter must start at 00:00. Keep titles short (under 8 words), \
         meaningful, and descriptive of the upcoming section. Aim for 5-10 \
         chapters for a 10-minute video; scale with length.{hint}"
    )
}

pub fn user_prompt(segments: &[TranscriptionSegment], language: &str) -> String {
    let transcript = segments
        .iter()
        .map(|s| format!("[{} - {}] {}", s.start_ms, s.end_ms, s.text))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Respond in {language}. Return chapters that start at logical topic \
         boundaries in the transcript.\n\nTranscript:\n{transcript}"
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chapters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "timestampMs": { "type": "integer", "minimum": 0 },
                        "title":       { "type": "string",  "minLength": 1 }
                    },
                    "required": ["timestampMs", "title"],
                    "additionalProperties": false
                },
                "minItems": 1
            }
        },
        "required": ["chapters"],
        "additionalProperties": false
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChapterResponse {
    chapters: Vec<ChapterResponseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChapterResponseEntry {
    #[serde(rename = "timestampMs")]
    timestamp_ms: i64,
    title: String,
}

#[async_trait]
pub trait ChapterRunner {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
        summary_hint: Option<&str>,
    ) -> Result<ChapterList, AiProviderError>;
}

pub struct ProviderChapterRunner<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> ChapterRunner for ProviderChapterRunner<'a> {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
        summary_hint: Option<&str>,
    ) -> Result<ChapterList, AiProviderError> {
        let req = CompletionRequest::structured(
            model,
            system_prompt(language, summary_hint),
            user_prompt(segments, language),
            "ChapterList",
            response_schema(),
        );
        let value = self.provider.complete(req).await?;
        let parsed: ChapterResponse =
            serde_json::from_value(value).map_err(|e| AiProviderError::Malformed(e.to_string()))?;

        let mut chapters: Vec<Chapter> = parsed
            .chapters
            .into_iter()
            .map(|e| Chapter {
                timestamp_ms: e.timestamp_ms,
                title: e.title,
            })
            .collect();

        // Normalize: sort, ensure first is 00:00, dedupe identical timestamps.
        chapters.sort_by_key(|c| c.timestamp_ms);
        chapters.dedup_by_key(|c| c.timestamp_ms);
        if let Some(first) = chapters.first_mut() {
            first.timestamp_ms = 0;
        }

        Ok(ChapterList {
            language: language.into(),
            chapters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn schema_has_min_items() {
        let s = response_schema();
        assert_eq!(s["properties"]["chapters"]["minItems"], 1);
    }

    struct StubProvider {
        response: Value,
        calls: Mutex<usize>,
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
            *self.calls.lock().unwrap() += 1;
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn first_chapter_pinned_to_zero() {
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({
                "chapters": [
                    { "timestampMs": 3_000, "title": "Intro" },
                    { "timestampMs": 60_000, "title": "Main" }
                ]
            }),
        };
        let runner = ProviderChapterRunner { provider: &stub };
        let segments = vec![TranscriptionSegment::new(0, 120_000, "full talk")];
        let list = runner.run(&segments, "English", "claude-3").await.unwrap();
        assert_eq!(list.chapters.len(), 2);
        assert_eq!(list.chapters[0].timestamp_ms, 0);
        assert_eq!(list.chapters[1].timestamp_ms, 60_000);
    }

    #[tokio::test]
    async fn duplicate_timestamps_merged() {
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({
                "chapters": [
                    { "timestampMs": 0, "title": "a" },
                    { "timestampMs": 0, "title": "a dup" },
                    { "timestampMs": 5_000, "title": "b" }
                ]
            }),
        };
        let runner = ProviderChapterRunner { provider: &stub };
        let list = runner.run(&[], "English", "claude-3").await.unwrap();
        assert_eq!(list.chapters.len(), 2);
    }
}
