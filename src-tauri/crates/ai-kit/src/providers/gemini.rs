//! Google Gemini provider.
//!
//! Uses the Generative Language API v1beta `:generateContent` endpoint.
//! Structured output flows through `generationConfig.responseSchema` +
//! `responseMimeType = "application/json"`. System prompts go into
//! `systemInstruction`.

use async_trait::async_trait;
use serde_json::{json, Value};

use creator_core::{AiProviderError, AiProviderType};

use crate::request::{CompletionRequest, ResponseFormat};
use crate::Provider;

pub const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const DEFAULT_MODEL: &str = "gemini-2.0-flash";

pub struct GeminiProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: GEMINI_API_BASE.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    pub fn build_body(request: &CompletionRequest) -> Value {
        let mut gen_config = json!({
            "temperature": request.temperature,
            "maxOutputTokens": request.max_tokens,
        });

        if let ResponseFormat::JsonSchema { schema, .. } = &request.response_format {
            gen_config["responseMimeType"] = json!("application/json");
            gen_config["responseSchema"] = schema.clone();
        }

        json!({
            "systemInstruction": {
                "parts": [ { "text": request.system } ]
            },
            "contents": [{
                "role": "user",
                "parts": [ { "text": request.user_prompt } ]
            }],
            "generationConfig": gen_config,
        })
    }

    pub fn parse_response(
        request: &CompletionRequest,
        body: &Value,
    ) -> Result<Value, AiProviderError> {
        let text = body
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .ok_or_else(|| AiProviderError::Malformed("missing candidates[0].content".into()))?;

        match &request.response_format {
            ResponseFormat::Freeform => Ok(json!({ "text": text })),
            ResponseFormat::JsonSchema { .. } => serde_json::from_str::<Value>(&text)
                .map_err(|e| AiProviderError::Malformed(format!("json parse: {e}"))),
        }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Gemini
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<Value, AiProviderError> {
        if self.api_key.is_empty() {
            return Err(AiProviderError::MissingApiKey(AiProviderType::Gemini));
        }
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, request.model, self.api_key
        );
        let body = Self::build_body(&request);
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiProviderError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AiProviderError::Rejected(format!("{status}: {text}")));
        }
        let json: Value = resp
            .json()
            .await
            .map_err(|e| AiProviderError::Malformed(e.to_string()))?;
        Self::parse_response(&request, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_moves_system_to_system_instruction() {
        let req = CompletionRequest::freeform(DEFAULT_MODEL, "you are helpful", "hi");
        let body = GeminiProvider::build_body(&req);
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "you are helpful"
        );
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn structured_body_sets_mime_and_schema() {
        let req = CompletionRequest::structured(
            DEFAULT_MODEL,
            "s",
            "u",
            "Out",
            json!({"type":"object"}),
        );
        let body = GeminiProvider::build_body(&req);
        assert_eq!(body["generationConfig"]["responseMimeType"], "application/json");
        assert_eq!(body["generationConfig"]["responseSchema"]["type"], "object");
    }

    #[test]
    fn parses_structured_text() {
        let req = CompletionRequest::structured(
            DEFAULT_MODEL,
            "s",
            "u",
            "Out",
            json!({"type":"object"}),
        );
        let body = json!({
            "candidates": [{
                "content": {
                    "parts": [ { "text": "{\"score\": 0.9}" } ]
                }
            }]
        });
        let out = GeminiProvider::parse_response(&req, &body).unwrap();
        assert!((out["score"].as_f64().unwrap() - 0.9).abs() < 1e-9);
    }
}
