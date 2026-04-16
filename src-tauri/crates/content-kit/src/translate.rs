//! Transcript translation.
//!
//! Rule shipped in v2: **if the source language is Vietnamese, skip
//! entirely.** The target for every other language defaults to
//! Vietnamese. Callers can override the target via `TranslateOptions`
//! when they actually want English → French (for example).
//!
//! Flow per call:
//!   1. Detect whether to skip (source == target, case-insensitive on
//!      BCP-47 primary tag).
//!   2. Chunk segments via `chunk_segments` so each request stays inside
//!      a reasonable prompt budget.
//!   3. Send batches to the provider, asking for a parallel JSON array of
//!      translated lines (one per input segment).
//!   4. Fan the translated text back into fresh `TranscriptionSegment`s
//!      that preserve every original timestamp + word array (words kept
//!      as-is — word-level re-alignment across languages is out of scope).
//!
//! The parallel-array schema is easier for small local models than
//! per-segment structured output: a single JSON array is harder to drift
//! on, and the length mismatch case is easy to detect.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, TranscriptionSegment};

use crate::batch::{chunk_segments, TranscriptBatch};

pub const DEFAULT_TARGET_LANGUAGE: &str = "vi";
/// Smaller batch window than summary/chapters — local 7B models start
/// dropping lines (returning N-1 translations for N inputs) once the batch
/// crosses ~15 segments. 25 s keeps us safely under that in practice.
pub const DEFAULT_BATCH_SECONDS: f64 = 25.0;

/// Knobs for a single translate call.
#[derive(Debug, Clone)]
pub struct TranslateOptions {
    /// BCP-47 primary tag (e.g. `"vi"`, `"en"`, `"ja"`). Defaults to `"vi"`.
    pub target_language: String,
    /// Max batch duration in seconds. Controls prompt size.
    pub max_batch_seconds: f64,
}

impl Default for TranslateOptions {
    fn default() -> Self {
        Self {
            target_language: DEFAULT_TARGET_LANGUAGE.into(),
            max_batch_seconds: DEFAULT_BATCH_SECONDS,
        }
    }
}

/// Outcome of a translate run. `skipped = true` means we returned the
/// originals untouched because the source already matches the target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResult {
    pub target_language: String,
    pub source_language: Option<String>,
    pub skipped: bool,
    pub segments: Vec<TranscriptionSegment>,
}

/// Returns true when the source language matches the target. Compares only
/// the primary BCP-47 subtag so `"vi"` / `"vi-VN"` / `"VI"` all collapse.
pub fn should_skip(source: Option<&str>, target: &str) -> bool {
    let Some(src) = source else {
        return false;
    };
    primary_tag(src).eq_ignore_ascii_case(primary_tag(target))
}

fn primary_tag(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag).trim()
}

pub fn system_prompt(target_language_name: &str) -> String {
    format!(
        "You translate video transcripts. Respond in {target_language_name}. \
         Preserve meaning, tone, and speaker intent. Keep numbers, names, and \
         technical terms intact. Do NOT summarise, omit, or add content. \
         Return ONLY the requested JSON — no prose, no markdown fences."
    )
}

pub fn user_prompt(batch: &TranscriptBatch, target_language_name: &str) -> String {
    let lines = batch
        .segments
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{i}. {}", s.text.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Translate every numbered line into {target_language_name}. Return a \
         JSON object with a `translations` array containing exactly {} strings \
         in the same order. Each string must be the translation of the line \
         with the same index.\n\nLines:\n{lines}",
        batch.segments.len()
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "translations": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["translations"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
struct TranslateResponse {
    translations: Vec<String>,
}

#[async_trait]
pub trait TranslateRunner {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        source_language: Option<&str>,
        options: &TranslateOptions,
        model: &str,
    ) -> Result<TranslateResult, AiProviderError>;
}

pub struct ProviderTranslateRunner<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> TranslateRunner for ProviderTranslateRunner<'a> {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        source_language: Option<&str>,
        options: &TranslateOptions,
        model: &str,
    ) -> Result<TranslateResult, AiProviderError> {
        if should_skip(source_language, &options.target_language) {
            return Ok(TranslateResult {
                target_language: options.target_language.clone(),
                source_language: source_language.map(|s| s.to_string()),
                skipped: true,
                segments: segments.to_vec(),
            });
        }

        if segments.is_empty() {
            return Ok(TranslateResult {
                target_language: options.target_language.clone(),
                source_language: source_language.map(|s| s.to_string()),
                skipped: false,
                segments: Vec::new(),
            });
        }

        let target_name = language_display_name(&options.target_language);
        let batches = chunk_segments(segments, options.max_batch_seconds);
        let mut out = Vec::with_capacity(segments.len());

        for batch in &batches {
            let translations = translate_batch_with_retry(
                self.provider,
                model,
                batch,
                target_name,
            )
            .await?;

            // Pad or truncate so length always matches — `translate_batch_with_retry`
            // already tried to get an exact count. Any remaining drift means
            // we give the user an almost-translated transcript rather than
            // failing the whole run.
            let aligned =
                align_to_originals(&batch.segments, translations);
            for (original, translated_text) in batch.segments.iter().zip(aligned.into_iter()) {
                let mut translated_segment = original.clone();
                translated_segment.text = translated_text;
                translated_segment.language = Some(options.target_language.clone());
                out.push(translated_segment);
            }
        }

        Ok(TranslateResult {
            target_language: options.target_language.clone(),
            source_language: source_language.map(|s| s.to_string()),
            skipped: false,
            segments: out,
        })
    }
}

/// Ask the provider once, and if the returned array length does not match
/// the batch, ask a second time with an even more explicit instruction. We
/// do not fail on mismatch here — downstream `align_to_originals` pads with
/// the source text so the user always gets something back.
async fn translate_batch_with_retry(
    provider: &dyn Provider,
    model: &str,
    batch: &TranscriptBatch,
    target_name: &str,
) -> Result<Vec<String>, AiProviderError> {
    let req = CompletionRequest {
        // Translations can be verbose: 25-second batches produce ~10-15 lines
        // each averaging ~30 tokens in the target language → ~500 tokens output.
        // 4096 gives 8× headroom; 2048 was too tight for long-sentence EN clips.
        max_tokens: 4096,
        ..CompletionRequest::structured(
            model,
            system_prompt(target_name),
            user_prompt(batch, target_name),
            "TranslatedBatch",
            response_schema(),
        )
    };
    let value = provider.complete(req).await?;
    let parsed: TranslateResponse = serde_json::from_value(value)
        .map_err(|e| AiProviderError::Malformed(format!("translate parse: {e}")))?;

    if parsed.translations.len() == batch.segments.len() {
        return Ok(parsed.translations);
    }

    // Retry once — restate the count explicitly and re-send. Small local
    // models sometimes drop a line when a run has lots of short segments.
    let retry_user = format!(
        "{}\n\nIMPORTANT: you returned {} strings on the previous attempt \
         but exactly {} are required — one per numbered line, same order, \
         no omissions. Retry now.",
        user_prompt(batch, target_name),
        parsed.translations.len(),
        batch.segments.len(),
    );
    let retry_req = CompletionRequest {
        max_tokens: 4096,
        ..CompletionRequest::structured(
            model,
            system_prompt(target_name),
            retry_user,
            "TranslatedBatch",
            response_schema(),
        )
    };
    let retry_value = provider.complete(retry_req).await?;
    let retry_parsed: TranslateResponse = serde_json::from_value(retry_value)
        .map_err(|e| AiProviderError::Malformed(format!("translate parse: {e}")))?;

    // Return whichever attempt is closer to the expected length — caller
    // pads the rest.
    let want = batch.segments.len();
    let first_drift = (parsed.translations.len() as i64 - want as i64).abs();
    let retry_drift = (retry_parsed.translations.len() as i64 - want as i64).abs();
    if retry_drift <= first_drift {
        Ok(retry_parsed.translations)
    } else {
        Ok(parsed.translations)
    }
}

/// Force the translations array to match the batch length by padding with
/// the original text on under-run and truncating on over-run. Preserves
/// alignment at index positions 0..min(len, expected).
fn align_to_originals(
    originals: &[TranscriptionSegment],
    mut translations: Vec<String>,
) -> Vec<String> {
    match translations.len().cmp(&originals.len()) {
        std::cmp::Ordering::Equal => translations,
        std::cmp::Ordering::Less => {
            for orig in &originals[translations.len()..] {
                translations.push(orig.text.clone());
            }
            translations
        }
        std::cmp::Ordering::Greater => {
            translations.truncate(originals.len());
            translations
        }
    }
}

/// Map common BCP-47 codes to a readable English name the LLM can use.
/// Unknown codes fall through verbatim so the model still has something
/// to work with.
pub fn language_display_name(tag: &str) -> &str {
    match primary_tag(tag).to_ascii_lowercase().as_str() {
        "vi" => "Vietnamese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "th" => "Thai",
        "id" => "Indonesian",
        "hi" => "Hindi",
        _ => tag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    fn seg(start_ms: i64, end_ms: i64, text: &str) -> TranscriptionSegment {
        TranscriptionSegment::new(start_ms, end_ms, text)
    }

    #[test]
    fn skip_when_source_equals_target() {
        assert!(should_skip(Some("vi"), "vi"));
        assert!(should_skip(Some("VI"), "vi"));
        assert!(should_skip(Some("vi-VN"), "vi"));
        assert!(!should_skip(Some("en"), "vi"));
        assert!(!should_skip(None, "vi"));
    }

    #[test]
    fn language_display_maps_known_tags() {
        assert_eq!(language_display_name("vi"), "Vietnamese");
        assert_eq!(language_display_name("en-US"), "English");
        assert_eq!(language_display_name("ja"), "Japanese");
        assert_eq!(language_display_name("xx"), "xx");
    }

    #[test]
    fn user_prompt_numbers_each_line() {
        let batch = TranscriptBatch {
            batch_index: 0,
            first_segment_index: 0,
            segments: vec![seg(0, 1000, "hello"), seg(1000, 2000, "world")],
        };
        let p = user_prompt(&batch, "Vietnamese");
        assert!(p.contains("0. hello"));
        assert!(p.contains("1. world"));
        assert!(p.contains("exactly 2 strings"));
    }

    #[test]
    fn schema_requires_translations_array() {
        let s = response_schema();
        assert_eq!(s["required"][0], "translations");
        assert_eq!(s["properties"]["translations"]["type"], "array");
    }

    struct StubProvider {
        response: Value,
        calls: Mutex<usize>,
    }

    #[async_trait]
    impl Provider for StubProvider {
        fn provider_type(&self) -> creator_core::AiProviderType {
            creator_core::AiProviderType::Mlx
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
    async fn translate_runner_skips_vi_source() {
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({"translations": []}),
        };
        let runner = ProviderTranslateRunner { provider: &stub };
        let segments = vec![seg(0, 1000, "xin chào")];
        let result = runner
            .run(&segments, Some("vi"), &TranslateOptions::default(), "model")
            .await
            .unwrap();
        assert!(result.skipped);
        assert_eq!(result.segments[0].text, "xin chào");
        assert_eq!(*stub.calls.lock().unwrap(), 0, "no provider call");
    }

    #[tokio::test]
    async fn translate_runner_emits_translated_segments() {
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({
                "translations": ["xin chào", "thế giới"]
            }),
        };
        let runner = ProviderTranslateRunner { provider: &stub };
        let segments = vec![seg(0, 1000, "hello"), seg(1000, 2000, "world")];
        let result = runner
            .run(&segments, Some("en"), &TranslateOptions::default(), "model")
            .await
            .unwrap();
        assert!(!result.skipped);
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].text, "xin chào");
        assert_eq!(result.segments[1].text, "thế giới");
        assert_eq!(result.segments[0].start_ms, 0);
        assert_eq!(result.segments[0].language.as_deref(), Some("vi"));
        assert_eq!(*stub.calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn translate_runner_pads_missing_translations_with_originals() {
        // LLM returns only 1 translation for 2 segments — we now pad with
        // the original text on under-run and retry once before that.
        // The StubProvider always returns the same payload so both the
        // first attempt and the retry hand back 1 translation; align_to_originals
        // fills the tail with the original text.
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({
                "translations": ["chỉ có một dòng"]
            }),
        };
        let runner = ProviderTranslateRunner { provider: &stub };
        let segments = vec![seg(0, 1000, "hello"), seg(1000, 2000, "world")];
        let result = runner
            .run(&segments, Some("en"), &TranslateOptions::default(), "model")
            .await
            .unwrap();
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].text, "chỉ có một dòng");
        assert_eq!(result.segments[1].text, "world"); // padded fallback
        // Two provider calls: initial + retry (both came back short).
        assert_eq!(*stub.calls.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn translate_runner_truncates_overflowing_translations() {
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({
                "translations": ["một", "hai", "ba thừa"]
            }),
        };
        let runner = ProviderTranslateRunner { provider: &stub };
        let segments = vec![seg(0, 1000, "one"), seg(1000, 2000, "two")];
        let result = runner
            .run(&segments, Some("en"), &TranslateOptions::default(), "model")
            .await
            .unwrap();
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].text, "một");
        assert_eq!(result.segments[1].text, "hai");
    }

    #[tokio::test]
    async fn translate_runner_handles_empty_input() {
        let stub = StubProvider {
            calls: Mutex::new(0),
            response: json!({"translations": []}),
        };
        let runner = ProviderTranslateRunner { provider: &stub };
        let result = runner
            .run(&[], Some("en"), &TranslateOptions::default(), "model")
            .await
            .unwrap();
        assert!(!result.skipped);
        assert!(result.segments.is_empty());
        assert_eq!(*stub.calls.lock().unwrap(), 0);
    }
}
