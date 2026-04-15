//! Groq LLM provider.
//!
//! Uses the same OpenAI-compatible `/openai/v1/chat/completions` API that
//! Groq exposes. Body building and response parsing are delegated to
//! `OpenAiProvider` (DRY). Groq's inference runs on LPUs — typically 5-10×
//! faster than GPU-based providers for the same model size.
//!
//! Recommended models:
//!   - `llama-3.3-70b-versatile`  — best quality, structured output support
//!   - `llama-3.1-8b-instant`     — fastest / cheapest, lighter tasks

use async_trait::async_trait;
use serde_json::{json, Value};

use creator_core::{AiProviderError, AiProviderType};

use crate::providers::openai::OpenAiProvider;
use crate::request::{CompletionRequest, ResponseFormat};
use crate::Provider;

pub const GROQ_API_BASE: &str = "https://api.groq.com/openai/v1";
pub const DEFAULT_MODEL: &str = "llama-3.3-70b-versatile";

pub struct GroqProvider {
    api_key: String,
    client: reqwest::Client,
}

impl GroqProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Provider for GroqProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Groq
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<Value, AiProviderError> {
        if self.api_key.is_empty() {
            return Err(AiProviderError::MissingApiKey(AiProviderType::Groq));
        }
        let url = format!("{}/chat/completions", GROQ_API_BASE);
        let mut body = OpenAiProvider::build_body(&request);
        // llama-3.3-70b-versatile doesn't support json_schema strict mode.
        // Downgrade to json_object — parse_response handles it identically.
        // Two extra requirements from Groq:
        //   1. "json" must appear somewhere in the messages, or the API 400s.
        //   2. Since json_object mode strips the schema, we must embed the
        //      schema into the user prompt so llama knows the expected shape
        //      (otherwise it invents arbitrary key names).
        if let ResponseFormat::JsonSchema { schema, .. } = &request.response_format {
            body["response_format"] = json!({"type": "json_object"});
            let schema_str = serde_json::to_string(schema).unwrap_or_else(|_| "{}".into());
            if let Some(msgs) = body.get_mut("messages").and_then(|m| m.as_array_mut()) {
                if let Some(user_msg) = msgs
                    .iter_mut()
                    .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                {
                    if let Some(content) = user_msg
                        .get("content")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                    {
                        user_msg["content"] = Value::String(format!(
                            "{content}\n\n\
                             Respond ONLY with a valid JSON object matching this schema \
                             (no prose, no markdown fences):\n{schema_str}"
                        ));
                    }
                }
            }
        }
        // Groq's free tier caps at 12K TPM — easy to blow past during a
        // batch pipeline (summary → chapters → filler → …). Retry up to
        // 3 times on 429, honouring the server's "try again in Xs" hint.
        const MAX_ATTEMPTS: u32 = 4;
        for attempt in 0..MAX_ATTEMPTS {
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.api_key)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| AiProviderError::Network(e.to_string()))?;

            if resp.status().is_success() {
                let json: Value = resp
                    .json()
                    .await
                    .map_err(|e| AiProviderError::Malformed(e.to_string()))?;
                return OpenAiProvider::parse_response(&request, &json);
            }

            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let is_last = attempt + 1 == MAX_ATTEMPTS;
            if status.as_u16() == 429 && !is_last {
                let wait = parse_retry_after_seconds(&text).unwrap_or(5.0);
                let wait_ms = ((wait + 0.5) * 1000.0).ceil() as u64;
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                continue;
            }
            return Err(AiProviderError::Rejected(format!("{status}: {text}")));
        }
        // Unreachable — loop either returns or errors on the last attempt.
        Err(AiProviderError::Rejected("exhausted retries".into()))
    }
}

/// Parse Groq's "Please try again in X.Xs" or "X.Xms" hint from a 429 body.
/// Returns seconds. Falls back to 5.0 s at the caller when nothing matches.
fn parse_retry_after_seconds(body: &str) -> Option<f64> {
    let lower = body.to_lowercase();
    let idx = lower.find("try again in")?;
    let tail = &lower[idx + "try again in".len()..];
    let tail = tail.trim_start();
    // Numeric prefix — collect digits + '.'.
    let num_end = tail
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(tail.len());
    let num_str = &tail[..num_end];
    let num: f64 = num_str.parse().ok()?;
    let unit_region = tail[num_end..].trim_start();
    if unit_region.starts_with("ms") {
        Some(num / 1000.0)
    } else {
        // Default to seconds.
        Some(num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::CompletionRequest;

    #[test]
    fn provider_type_is_groq() {
        let p = GroqProvider::new("gsk_test");
        assert_eq!(p.provider_type(), AiProviderType::Groq);
    }

    #[test]
    fn unavailable_when_empty_key() {
        let p = GroqProvider::new("");
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        assert!(!rt.block_on(p.is_available()));
    }

    #[test]
    fn body_uses_openai_format() {
        let req = CompletionRequest::freeform(DEFAULT_MODEL, "sys", "usr");
        let body = OpenAiProvider::build_body(&req);
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert!(body.get("max_completion_tokens").is_some());
    }

    #[test]
    fn retry_after_parser_handles_seconds() {
        let body = r#"{"error":{"message":"Please try again in 7.55s. More info."}}"#;
        let s = parse_retry_after_seconds(body).unwrap();
        assert!((s - 7.55).abs() < 1e-6);
    }

    #[test]
    fn retry_after_parser_handles_milliseconds() {
        let body = "Please try again in 644.999999ms. Need more tokens?";
        let s = parse_retry_after_seconds(body).unwrap();
        assert!((s - 0.645).abs() < 1e-3);
    }

    #[test]
    fn retry_after_parser_returns_none_when_absent() {
        assert!(parse_retry_after_seconds("no hint here").is_none());
    }

    #[test]
    fn json_schema_downgraded_to_json_object() {
        use serde_json::json;
        // A structured request produces json_schema in the body.
        let schema = json!({"type":"object","properties":{"result":{"type":"string"}}});
        let req = CompletionRequest::structured(DEFAULT_MODEL, "sys", "usr", "MySchema", schema);
        let mut body = OpenAiProvider::build_body(&req);
        // Simulate the downgrade applied inside complete().
        if body
            .get("response_format")
            .and_then(|f| f.get("type"))
            .and_then(|t| t.as_str())
            == Some("json_schema")
        {
            body["response_format"] = json!({"type": "json_object"});
        }
        assert_eq!(body["response_format"]["type"], "json_object");
        assert!(body["response_format"].get("json_schema").is_none());
    }
}
