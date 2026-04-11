//! Ollama provider — local HTTP daemon, cross-platform, zero-cost.
//!
//! Structured output uses the `format: <json-schema>` field that Ollama ≥
//! 0.5 accepts. If the daemon is older, the schema is still sent and
//! Ollama falls back to free-form JSON; downstream `serde_json::from_str`
//! catches malformed output and surfaces a `Malformed` error.

use async_trait::async_trait;
use serde_json::{json, Value};

use creator_core::{AiProviderError, AiProviderType};

use crate::request::{CompletionRequest, ResponseFormat};
use crate::Provider;

pub const DEFAULT_HOST: &str = "http://127.0.0.1:11434";
pub const DEFAULT_MODEL: &str = "llama3.2";

pub struct OllamaProvider {
    host: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn default_local() -> Self {
        Self::new(DEFAULT_HOST)
    }

    pub fn build_body(request: &CompletionRequest) -> Value {
        let mut body = json!({
            "model": request.model,
            "prompt": request.user_prompt,
            "system": request.system,
            "stream": false,
            "options": {
                "temperature": request.temperature,
                "num_predict": request.max_tokens,
            }
        });
        if let ResponseFormat::JsonSchema { schema, .. } = &request.response_format {
            body["format"] = schema.clone();
        }
        body
    }

    pub fn parse_response(
        request: &CompletionRequest,
        body: &Value,
    ) -> Result<Value, AiProviderError> {
        let text = body
            .get("response")
            .and_then(|r| r.as_str())
            .ok_or_else(|| AiProviderError::Malformed("missing response field".into()))?;

        match &request.response_format {
            ResponseFormat::Freeform => Ok(json!({ "text": text })),
            ResponseFormat::JsonSchema { .. } => serde_json::from_str::<Value>(text)
                .map_err(|e| AiProviderError::Malformed(format!("json parse: {e}"))),
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Ollama
    }

    async fn is_available(&self) -> bool {
        // Ping the /api/tags endpoint — returns 200 when daemon is up.
        let url = format!("{}/api/tags", self.host);
        match self.client.get(&url).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<Value, AiProviderError> {
        let url = format!("{}/api/generate", self.host);
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
    fn body_includes_options() {
        let req = CompletionRequest::freeform(DEFAULT_MODEL, "sys", "usr");
        let body = OllamaProvider::build_body(&req);
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["prompt"], "usr");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["stream"], false);
        assert!(body["options"]["temperature"].is_number());
    }

    #[test]
    fn structured_body_sets_format_schema() {
        let req = CompletionRequest::structured(
            DEFAULT_MODEL,
            "s",
            "u",
            "Out",
            json!({"type":"object","required":["x"]}),
        );
        let body = OllamaProvider::build_body(&req);
        assert_eq!(body["format"]["type"], "object");
    }

    #[test]
    fn parses_response_field() {
        let req = CompletionRequest::freeform(DEFAULT_MODEL, "s", "u");
        let body = json!({ "response": "hello", "done": true });
        let out = OllamaProvider::parse_response(&req, &body).unwrap();
        assert_eq!(out["text"], "hello");
    }
}
