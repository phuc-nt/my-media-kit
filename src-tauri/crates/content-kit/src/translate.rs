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

/// `summary_hint` — brief content description injected into the system prompt so
/// the model can maintain consistent terminology and proper-noun romanization.
pub fn system_prompt(target_language_name: &str, summary_hint: Option<&str>) -> String {
    let hint_section = summary_hint
        .filter(|h| !h.trim().is_empty())
        .map(|h| format!(
            "\nContent context (use for consistent terminology and proper-noun \
             romanization across all batches): {h}"
        ))
        .unwrap_or_default();
    format!(
        "You are a professional subtitle translator. Your ONLY job is to translate \
         text into {target_language_name}.{hint_section}\n\
         CRITICAL RULES:\n\
         - Every output string MUST be written entirely in {target_language_name}.\n\
         - Do NOT output Chinese characters, Japanese characters, or any other script.\n\
         - For proper nouns (names, places) you cannot translate, write them in Latin \
           script (romanization) or keep the original spelling — never use CJK characters.\n\
         - Preserve meaning, tone, and speaker intent.\n\
         - Do NOT summarise, omit, or add content.\n\
         - Return ONLY the requested JSON — no prose, no markdown fences."
    )
}

/// `prev_context` — last few already-translated lines from the preceding batch,
/// included so the model maintains narrative continuity.
pub fn user_prompt(
    batch: &TranscriptBatch,
    target_language_name: &str,
    prev_context: &[String],
) -> String {
    let context_section = if !prev_context.is_empty() {
        format!(
            "Previous translated lines (context only — do NOT include in output):\n{}\n\n",
            prev_context
                .iter()
                .map(|l| format!("  • {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        String::new()
    };
    let lines = batch
        .segments
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{i}. {}", s.text.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    // /no_think disables Qwen3 reasoning mode; ignored by other models.
    format!(
        "/no_think\n{context_section}Translate every numbered line into {target_language_name}. \
         ALL output strings must be in {target_language_name} only — no other scripts. \
         Return a JSON object with a `translations` array containing exactly {} strings \
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

/// Robustly extract a translations list from whatever the model returned.
/// Local 7B models drift between several shapes; we accept all of them:
///   - `{"translations": ["a","b"]}` — canonical
///   - `{"translations": "a"}`       — single string for 1-segment batches
///   - `["a","b"]`                   — bare array (object envelope dropped)
///   - `"a"`                         — bare string for 1-segment batches
///   - `{"0": "a", "1": "b"}`        — numeric-keyed object
/// Anything else returns Err so the retry loop can take another swing.
fn parse_translations(value: &Value) -> Result<Vec<String>, String> {
    if let Some(field) = value.get("translations") {
        return coerce_to_string_vec(field);
    }
    if value.is_array() || value.is_string() {
        return coerce_to_string_vec(value);
    }
    if let Some(obj) = value.as_object() {
        let mut entries: Vec<(usize, String)> = obj
            .iter()
            .filter_map(|(k, v)| Some((k.parse::<usize>().ok()?, v.as_str()?.to_string())))
            .collect();
        if !entries.is_empty() {
            entries.sort_by_key(|(i, _)| *i);
            return Ok(entries.into_iter().map(|(_, s)| s).collect());
        }
    }
    Err(format!(
        "could not extract translations from response shape: {value}"
    ))
}

fn coerce_to_string_vec(v: &Value) -> Result<Vec<String>, String> {
    match v {
        Value::Array(arr) => Ok(arr
            .iter()
            .map(|item| match item {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect()),
        Value::String(s) => Ok(vec![s.clone()]),
        other => Err(format!("expected array or string, got: {other}")),
    }
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
        let mut prev_context: Vec<String> = Vec::new();

        for batch in &batches {
            let translations = translate_batch_with_retry(
                self.provider,
                model,
                batch,
                target_name,
                None,
                &prev_context,
            )
            .await?;

            // Pad or truncate so length always matches — `translate_batch_with_retry`
            // already tried to get an exact count. Any remaining drift means
            // we give the user an almost-translated transcript rather than
            // failing the whole run.
            let aligned = align_to_originals(&batch.segments, translations);
            // Slide context window: keep last 5 translated texts for next batch.
            prev_context.extend(aligned.iter().cloned());
            if prev_context.len() > 5 {
                let drain = prev_context.len() - 5;
                prev_context.drain(..drain);
            }
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

/// Maximum attempts to translate a single batch before giving up. Local
/// models occasionally drift on schema; the retry loop restates the
/// contract more aggressively each round.
pub const MAX_BATCH_ATTEMPTS: usize = 3;

/// Translate one batch. Retries up to `MAX_BATCH_ATTEMPTS` times on parse
/// failures or count mismatches, escalating the prompt instructions each
/// round. Returns the best attempt found (downstream `align_to_originals`
/// pads any remaining drift) — only fails when every attempt errors at the
/// network/provider layer or returns truly unparseable text.
pub async fn translate_batch_with_retry(
    provider: &dyn Provider,
    model: &str,
    batch: &TranscriptBatch,
    target_name: &str,
    summary_hint: Option<&str>,
    prev_context: &[String],
) -> Result<Vec<String>, AiProviderError> {
    let want = batch.segments.len();
    let mut best: Option<Vec<String>> = None;
    let mut last_err: Option<AiProviderError> = None;

    for attempt in 0..MAX_BATCH_ATTEMPTS {
        let user = if attempt == 0 {
            user_prompt(batch, target_name, prev_context)
        } else {
            // Escalate: tell the model exactly what went wrong last time.
            let prev_len = best.as_ref().map(|v| v.len()).unwrap_or(0);
            format!(
                "{}\n\nIMPORTANT: previous attempt {} — required exactly {} \
                 strings in the `translations` array (one per numbered line). \
                 Return ONLY a JSON object: {{\"translations\": [\"...\", \"...\"]}}. \
                 No prose, no markdown.",
                user_prompt(batch, target_name, prev_context),
                if prev_len == 0 {
                    "could not be parsed".to_string()
                } else {
                    format!("returned {prev_len} strings")
                },
                want,
            )
        };

        let req = CompletionRequest {
            // Translations can be verbose: 25-second batches produce ~10-15 lines
            // each averaging ~30 tokens in the target language → ~500 tokens output.
            // 4096 gives 8× headroom; 2048 was too tight for long-sentence EN clips.
            max_tokens: 4096,
            ..CompletionRequest::structured(
                model,
                system_prompt(target_name, summary_hint),
                user,
                "TranslatedBatch",
                response_schema(),
            )
        };

        match provider.complete(req).await {
            Ok(value) => match parse_translations(&value) {
                Ok(translations) => {
                    let exact = translations.len() == want;
                    let better = match &best {
                        None => true,
                        Some(prev) => {
                            let d_new = (translations.len() as i64 - want as i64).abs();
                            let d_old = (prev.len() as i64 - want as i64).abs();
                            d_new < d_old
                        }
                    };
                    if better {
                        best = Some(translations);
                    }
                    if exact {
                        return Ok(best.unwrap());
                    }
                }
                Err(parse_err) => {
                    last_err = Some(AiProviderError::Malformed(format!(
                        "translate parse (attempt {}/{}): {parse_err}",
                        attempt + 1,
                        MAX_BATCH_ATTEMPTS
                    )));
                }
            },
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    // Prefer the best parsed result (caller pads/truncates) over a hard error.
    if let Some(t) = best {
        return Ok(t);
    }
    Err(last_err.unwrap_or_else(|| {
        AiProviderError::Malformed("translate: no usable response from provider".into())
    }))
}

/// Force the translations array to match the batch length by padding with
/// the original text on under-run and truncating on over-run. Preserves
/// alignment at index positions 0..min(len, expected).
pub fn align_to_originals(
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
        let p = user_prompt(&batch, "Vietnamese", &[]);
        assert!(p.contains("0. hello"));
        assert!(p.contains("1. world"));
        assert!(p.contains("exactly 2 strings"));
    }

    #[test]
    fn user_prompt_includes_prev_context() {
        let batch = TranscriptBatch {
            batch_index: 1,
            first_segment_index: 2,
            segments: vec![seg(2000, 3000, "next line")],
        };
        let ctx = vec!["Xin chào".to_string(), "Thế giới".to_string()];
        let p = user_prompt(&batch, "Vietnamese", &ctx);
        assert!(p.contains("Xin chào"));
        assert!(p.contains("Thế giới"));
        assert!(p.contains("context only"));
    }

    #[test]
    fn system_prompt_includes_summary_hint() {
        let p = system_prompt("Vietnamese", Some("A talk about rockets"));
        assert!(p.contains("A talk about rockets"));
        let p_no_hint = system_prompt("Vietnamese", None);
        assert!(!p_no_hint.contains("Content context"));
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
