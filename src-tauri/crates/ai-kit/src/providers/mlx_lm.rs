//! MLX LM (local) provider.
//!
//! `mlx_lm.server` (shipped with `pip install mlx-lm`) serves an
//! **OpenAI-compatible** HTTP API on `127.0.0.1:<port>`. We cheat and
//! delegate everything to an embedded `OpenAiProvider` pointed at that
//! local URL.
//!
//! Because the server doesn't require an API key, we pass a dummy "mlx"
//! string for the Bearer header — the server ignores it but our
//! `OpenAiProvider::is_available()` check guards against empty strings.
//!
//! **Model name:** newer versions of mlx_lm.server reject requests whose
//! `model` field doesn't match the loaded model (it tries to fetch the name
//! from HuggingFace). We therefore fetch the actual model id from
//! `/v1/models` on first use (cached via `OnceCell`) and substitute it into
//! every CompletionRequest before forwarding. Callers may pass any string —
//! it is always overridden for this provider.
//!
//! **Lifecycle:** for MVP the user is responsible for starting the server
//! (`mlx_lm.server --model <model> --port 8080`). Auto-spawning is a
//! follow-up; it complicates error handling (port busy, model load delay,
//! zombie processes) and the manual flow is fine for local dev.
//!
//! Availability check pings `/v1/models` with a 300 ms timeout.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;


use creator_core::{AiProviderError, AiProviderType};

use crate::providers::openai::OpenAiProvider;
use crate::request::{CompletionRequest, ResponseFormat};
use crate::Provider;

pub const DEFAULT_HOST: &str = "http://127.0.0.1:8080";
/// Qwen3-14B 4-bit — best local translation quality, 140+ languages, ~9 GB RAM.
/// Use /no_think prefix in translation prompts to skip reasoning tokens.
pub const DEFAULT_MODEL: &str = "mlx-community/Qwen3-14B-4bit";
pub const DUMMY_KEY: &str = "mlx";

pub struct MlxLmProvider {
    host: String,
    inner: OpenAiProvider,
    probe: reqwest::Client,
    /// Lazily-fetched model id from the running server's `/v1/models`.
    loaded_model: Arc<OnceCell<String>>,
}

impl MlxLmProvider {
    pub fn new(host: impl Into<String>) -> Self {
        // `host` should be the server root (e.g. "http://127.0.0.1:8080").
        // OpenAiProvider builds `{base_url}/v1/chat/completions` internally,
        // so we strip any trailing `/v1` the caller might have added rather
        // than double it.
        let host = host
            .into()
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .to_string();
        let inner = OpenAiProvider::new(DUMMY_KEY).with_base_url(host.clone());
        let probe = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .expect("build probe client");
        Self {
            host,
            inner,
            probe,
            loaded_model: Arc::new(OnceCell::new()),
        }
    }

    pub fn default_local() -> Self {
        Self::new(DEFAULT_HOST)
    }

    /// Return the model id currently loaded in the server. On first call this
    /// issues a GET /v1/models and caches the first entry's id. Falls back to
    /// DEFAULT_MODEL if the endpoint is unavailable or returns no data.
    async fn server_model(&self) -> String {
        self.loaded_model
            .get_or_init(|| async {
                let url = format!("{}/v1/models", self.host);
                let id: Option<String> = async {
                    let resp = self.probe.get(&url).send().await.ok()?;
                    let body: Value = resp.json().await.ok()?;
                    let s = body
                        .get("data")?
                        .as_array()?
                        .first()?
                        .get("id")?
                        .as_str()?
                        .to_string();
                    Some(s)
                }
                .await;
                id.unwrap_or_else(|| DEFAULT_MODEL.to_string())
            })
            .await
            .clone()
    }
}

#[async_trait]
impl Provider for MlxLmProvider {
    fn provider_type(&self) -> AiProviderType {
        AiProviderType::Mlx
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.host);
        match self.probe.get(&url).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    async fn complete(&self, request: CompletionRequest) -> Result<Value, AiProviderError> {
        // mlx_lm.server does not honour OpenAI's `response_format.json_schema`
        // strict mode (at least as of mlx-lm 0.31.x). Asking for structured
        // output yields plain prose. To keep downstream code (content-kit)
        // working without a special case, we:
        //   1. Downgrade the outgoing request to freeform regardless of what
        //      the caller asked for.
        //   2. Amend the user prompt with an explicit JSON contract when the
        //      original request wanted structured output.
        //   3. On receiving the text response, try to extract the first
        //      well-formed JSON object / array from it. Falls back to
        //      wrapping the text in {"text": "..."} for freeform requests.
        let wanted_json = matches!(request.response_format, ResponseFormat::JsonSchema { .. });

        // Always use the model currently loaded in the server — newer versions
        // of mlx_lm.server reject requests whose `model` field doesn't match.
        let actual_model = self.server_model().await;

        let downgraded = if wanted_json {
            let schema_hint = match &request.response_format {
                ResponseFormat::JsonSchema { name, schema } => {
                    format!(
                        "\n\n-- OUTPUT CONTRACT --\nReturn ONLY a JSON value conforming to this schema, \
                         with NO prose, NO markdown fences, NO explanations. Schema name: {name}. \
                         Schema body: {}",
                        schema
                    )
                }
                _ => String::new(),
            };
            CompletionRequest {
                model: actual_model,
                user_prompt: format!("{}{}", request.user_prompt, schema_hint),
                response_format: ResponseFormat::Freeform,
                ..request.clone()
            }
        } else {
            CompletionRequest {
                model: actual_model,
                ..request.clone()
            }
        };

        let raw = self.inner.complete(downgraded).await?;
        let text = raw
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();

        if wanted_json {
            extract_json_value(&text).ok_or_else(|| {
                AiProviderError::Malformed(format!(
                    "mlx_lm response was not parseable JSON; first 200 chars: {:?}",
                    text.chars().take(200).collect::<String>()
                ))
            })
        } else {
            Ok(json!({ "text": text }))
        }
    }
}

/// Try to pull a JSON value out of a possibly-decorated string. Handles:
///   - raw JSON (strict parse)
///   - JSON wrapped in `` ```json ... ``` `` or `` ``` ... ``` `` fences
///   - JSON embedded after / before prose (takes the first balanced
///     object / array)
///
/// Returns `None` when no JSON can be recovered.
fn extract_json_value(text: &str) -> Option<Value> {
    // 1. Strict parse first.
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        return Some(v);
    }

    // 2. Strip common code fences.
    let stripped = strip_code_fence(text);
    if stripped != text {
        if let Ok(v) = serde_json::from_str::<Value>(stripped.trim()) {
            return Some(v);
        }
    }

    // 3. Scan for the earliest `{` or `[` in the text, then walk until it
    //    balances. Try both openers so an array wrapping objects (common in
    //    detection responses) wins over any single-object substring.
    let obj_pos = text.find('{');
    let arr_pos = text.find('[');
    let candidates: Vec<(usize, char, char)> = match (obj_pos, arr_pos) {
        (Some(o), Some(a)) if a < o => vec![(a, '[', ']'), (o, '{', '}')],
        (Some(o), Some(a)) => vec![(o, '{', '}'), (a, '[', ']')],
        (Some(o), None) => vec![(o, '{', '}')],
        (None, Some(a)) => vec![(a, '[', ']')],
        (None, None) => vec![],
    };
    for (start, opener, closer) in candidates {
        if let Some(end) = find_balanced_end(&text[start..], opener, closer) {
            let slice = &text[start..start + end + 1];
            if let Ok(v) = serde_json::from_str::<Value>(slice) {
                return Some(v);
            }
        }
    }
    None
}

fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        return rest.trim().trim_end_matches("```").trim();
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        return rest.trim().trim_end_matches("```").trim();
    }
    text
}

/// Walk a string (relative offset 0) and return the index of the closing
/// char that balances the opener at offset 0. Respects string literals so
/// `"}"` inside values doesn't confuse the counter. Returns the offset of
/// the closer character on success.
fn find_balanced_end(s: &str, opener: char, closer: char) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            continue;
        }
        if c == opener {
            depth += 1;
        } else if c == closer {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_strips_trailing_v1_and_slash() {
        assert_eq!(
            MlxLmProvider::new("http://127.0.0.1:9000").host,
            "http://127.0.0.1:9000"
        );
        assert_eq!(
            MlxLmProvider::new("http://127.0.0.1:9000/").host,
            "http://127.0.0.1:9000"
        );
        assert_eq!(
            MlxLmProvider::new("http://127.0.0.1:9000/v1").host,
            "http://127.0.0.1:9000"
        );
    }

    #[test]
    fn provider_type_is_mlx() {
        let p = MlxLmProvider::default_local();
        assert_eq!(p.provider_type(), AiProviderType::Mlx);
    }

    #[tokio::test]
    async fn is_available_false_when_server_down() {
        // Use an unlikely port so we don't depend on test env.
        let p = MlxLmProvider::new("http://127.0.0.1:59999");
        assert!(!p.is_available().await);
    }

    #[test]
    fn extract_json_handles_strict_object() {
        let v = extract_json_value(r#"{"a": 1, "b": 2}"#).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn extract_json_handles_fenced_block() {
        let text = "```json\n{\"a\": 1}\n```";
        let v = extract_json_value(text).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn extract_json_handles_prose_preamble() {
        let text = "Sure, here is the result:\n{\"answer\": 42, \"notes\": \"ok\"}\nLet me know.";
        let v = extract_json_value(text).unwrap();
        assert_eq!(v["answer"], 42);
    }

    #[test]
    fn extract_json_handles_array_root() {
        let text = "Output: [{\"x\": 1}, {\"x\": 2}]";
        let v = extract_json_value(text).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[test]
    fn extract_json_skips_braces_inside_strings() {
        let text = r#"before {"msg": "a } b", "ok": true} after"#;
        let v = extract_json_value(text).unwrap();
        assert_eq!(v["msg"], "a } b");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn extract_json_returns_none_for_pure_prose() {
        assert!(extract_json_value("Nothing JSON here.").is_none());
    }
}
