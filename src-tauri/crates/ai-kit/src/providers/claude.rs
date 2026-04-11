//! Anthropic Claude provider.
//!
//! Uses the `/v1/messages` endpoint. Structured output is implemented via
//! the tools + `tool_choice` technique so we get a guaranteed JSON shape
//! without asking the model nicely (which fails 5 % of the time).

use async_trait::async_trait;
use serde_json::{json, Value};

use creator_core::{AiProviderError, AiProviderType};

use crate::request::{CompletionRequest, ResponseFormat};
use crate::Provider;

pub const CLAUDE_API_BASE: &str = "https://api.anthropic.com";
pub const CLAUDE_API_VERSION: &str = "2023-06-01";
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-5-20250929";

pub struct ClaudeProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl ClaudeProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: CLAUDE_API_BASE.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Build the JSON body `/v1/messages` expects. Public for tests.
    pub fn build_body(request: &CompletionRequest) -> Value {
        let messages = vec![json!({
            "role": "user",
            "content": request.user_prompt,
        })];

        let mut body = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "system": request.system,
            "messages": messages,
        });

        if let ResponseFormat::JsonSchema { name, schema } = &request.response_format {
            // Tool-use trick: define a single tool whose input_schema is the
            // caller's schema, then force the model to call it. The tool
            // arguments end up as a JSON value matching the schema.
            let tool = json!({
                "name": name,
                "description": "Return the structured response.",
                "input_schema": schema,
            });
            body["tools"] = json!([tool]);
            body["tool_choice"] = json!({ "type": "tool", "name": name });
        }

        body
    }

    /// Extract the structured payload from a Claude response body. Public
    /// for tests.
    pub fn parse_response(
        request: &CompletionRequest,
        body: &Value,
    ) -> Result<Value, AiProviderError> {
        let content = body
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| AiProviderError::Malformed("missing content array".into()))?;

        match &request.response_format {
            ResponseFormat::Freeform => {
                let text = content
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            block.get("text").and_then(|t| t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                Ok(json!({ "text": text }))
            }
            ResponseFormat::JsonSchema { name, .. } => {
                for block in content {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                        && block.get("name").and_then(|n| n.as_str()) == Some(name.as_str())
                    {
                        return Ok(block.get("input").cloned().unwrap_or(Value::Null));
                    }
                }
                Err(AiProviderError::Malformed(format!(
                    "no tool_use block for {name}"
                )))
            }
        }
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Claude
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<Value, AiProviderError> {
        if self.api_key.is_empty() {
            return Err(AiProviderError::MissingApiKey(AiProviderType::Claude));
        }
        let url = format!("{}/v1/messages", self.base_url);
        let body = Self::build_body(&request);
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", CLAUDE_API_VERSION)
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
    fn structured_body_has_tool_and_choice() {
        let req = CompletionRequest::structured(
            DEFAULT_MODEL,
            "sys",
            "usr",
            "Thing",
            json!({"type":"object","properties":{"a":{"type":"string"}}}),
        );
        let body = ClaudeProvider::build_body(&req);
        assert_eq!(body["model"], DEFAULT_MODEL);
        assert_eq!(body["tools"][0]["name"], "Thing");
        assert_eq!(body["tool_choice"]["name"], "Thing");
        assert_eq!(body["tool_choice"]["type"], "tool");
    }

    #[test]
    fn freeform_body_has_no_tools() {
        let req = CompletionRequest::freeform(DEFAULT_MODEL, "sys", "usr");
        let body = ClaudeProvider::build_body(&req);
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn parses_freeform_response() {
        let req = CompletionRequest::freeform(DEFAULT_MODEL, "s", "u");
        let body = json!({
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "text", "text": " world" }
            ]
        });
        let out = ClaudeProvider::parse_response(&req, &body).unwrap();
        assert_eq!(out["text"], "hello world");
    }

    #[test]
    fn parses_structured_response() {
        let req = CompletionRequest::structured(
            DEFAULT_MODEL,
            "s",
            "u",
            "Result",
            json!({"type":"object"}),
        );
        let body = json!({
            "content": [
                { "type": "tool_use", "name": "Result", "input": { "answer": 42 } }
            ]
        });
        let out = ClaudeProvider::parse_response(&req, &body).unwrap();
        assert_eq!(out["answer"], 42);
    }

    #[test]
    fn structured_response_missing_tool_errors() {
        let req = CompletionRequest::structured(
            DEFAULT_MODEL,
            "s",
            "u",
            "Result",
            json!({"type":"object"}),
        );
        let body = json!({ "content": [] });
        let err = ClaudeProvider::parse_response(&req, &body).unwrap_err();
        assert!(matches!(err, AiProviderError::Malformed(_)));
    }
}
