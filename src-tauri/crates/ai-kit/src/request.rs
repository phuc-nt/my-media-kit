//! Shared request envelope for provider calls.
//!
//! `system` + `user_prompt` map cleanly onto every API we target:
//!   - Claude: `system` + one user message
//!   - OpenAI: `system` role + `user` role
//!   - Gemini: `systemInstruction` + contents[0]
//!   - Ollama: `system` + `prompt`
//!
//! `response_format` is how callers request structured output. The
//! `Freeform` variant produces plain text; `JsonSchema` asks the provider to
//! return a JSON object matching the supplied schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub system: String,
    pub user_prompt: String,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_temperature() -> f32 {
    0.2
}

fn default_max_tokens() -> u32 {
    2048
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResponseFormat {
    /// Plain text; provider returns a string under the convention key
    /// `{"text": "..."}` so downstream code stays uniform.
    Freeform,
    /// Structured output against a JSON schema. Providers adapt this into
    /// their native structured-output syntax (Claude tools, OpenAI
    /// `response_format.json_schema`, Gemini `responseSchema`, Ollama
    /// `format`).
    JsonSchema { name: String, schema: Value },
}

impl Default for ResponseFormat {
    fn default() -> Self {
        Self::Freeform
    }
}

impl CompletionRequest {
    pub fn freeform(
        model: impl Into<String>,
        system: impl Into<String>,
        user_prompt: impl Into<String>,
    ) -> Self {
        Self {
            system: system.into(),
            user_prompt: user_prompt.into(),
            model: model.into(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            response_format: ResponseFormat::Freeform,
        }
    }

    pub fn structured(
        model: impl Into<String>,
        system: impl Into<String>,
        user_prompt: impl Into<String>,
        schema_name: impl Into<String>,
        schema: Value,
    ) -> Self {
        Self {
            system: system.into(),
            user_prompt: user_prompt.into(),
            model: model.into(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            response_format: ResponseFormat::JsonSchema {
                name: schema_name.into(),
                schema,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freeform_defaults() {
        let r = CompletionRequest::freeform("claude-sonnet-4", "sys", "usr");
        assert_eq!(r.temperature, 0.2);
        assert_eq!(r.max_tokens, 2048);
        assert!(matches!(r.response_format, ResponseFormat::Freeform));
    }

    #[test]
    fn structured_carries_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "title": { "type": "string" } },
            "required": ["title"]
        });
        let r =
            CompletionRequest::structured("gpt-4o", "sys", "usr", "TitleOut", schema.clone());
        match r.response_format {
            ResponseFormat::JsonSchema { name, schema: s } => {
                assert_eq!(name, "TitleOut");
                assert_eq!(s, schema);
            }
            _ => panic!("wrong format"),
        }
    }

    #[test]
    fn request_serialises_with_tagged_format() {
        let r = CompletionRequest::freeform("m", "s", "u");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["response_format"]["kind"], "freeform");
    }
}
