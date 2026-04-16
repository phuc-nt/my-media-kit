//! Content-feature commands: filler detection, summary, chapters. All
//! route through the configured provider. On Apple Silicon the default is
//! MLX (local); other platforms fall back to whatever has a keyring entry.
//!
//! Phase-8 wiring: these commands take raw transcript segments from the
//! frontend. A later phase will source them from a stored transcription
//! result once we have app state plumbed.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::command;

use ai_kit::{Provider, ProviderRegistry, SecretStore, KeyringSecretStore};
use content_kit::{
    batch::TranscriptBatch,
    chapters::{ChapterList, ChapterRunner, ProviderChapterRunner},
    duplicate::{AiDuplicateDetector, DuplicateDetector, DUPLICATE_BATCH_SECONDS},
    filler::{AiFillerDetector, FillerDetector, FILLER_BATCH_SECONDS},
    prompt_cut::{AiPromptCutter, ProviderCutter},
    summary::{ProviderSummaryRunner, SummaryResult, SummaryRunner, SummaryStyle},
    translate::{
        ProviderTranslateRunner, TranslateOptions, TranslateResult, TranslateRunner,
        DEFAULT_TARGET_LANGUAGE,
    },
};
use creator_core::{AiProviderType, AiPromptDetection, DuplicateDetection, FillerDetection, TranscriptionSegment};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRequest {
    pub provider: AiProviderType,
    pub model: String,
    pub segments: Vec<TranscriptionSegment>,
    /// Only used for summary/chapters; filler uses its own bilingual prompt.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FillerOutput {
    pub detections: Vec<FillerDetection>,
}

#[command]
pub async fn content_filler_detect(request: ContentRequest) -> Result<FillerOutput, String> {
    let provider = resolve_provider(request.provider).await?;
    let detector = AiFillerDetector {
        provider: provider.as_ref(),
    };
    // Use chunked detection to avoid huge prompts on long transcripts.
    let detections = detector
        .detect_transcript(&request.segments, &request.model, FILLER_BATCH_SECONDS)
        .await
        .map_err(|e| e.to_string())?;
    Ok(FillerOutput { detections })
}

#[derive(Debug, Serialize)]
pub struct DuplicateOutput {
    pub detections: Vec<DuplicateDetection>,
}

#[command]
pub async fn content_duplicate_detect(request: ContentRequest) -> Result<DuplicateOutput, String> {
    let provider = resolve_provider(request.provider).await?;
    let detector = AiDuplicateDetector {
        provider: provider.as_ref(),
    };
    // Chunked: re-take detection only applies within a local window anyway.
    let detections = detector
        .detect_transcript(&request.segments, &request.model, DUPLICATE_BATCH_SECONDS)
        .await
        .map_err(|e| e.to_string())?;
    Ok(DuplicateOutput { detections })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCutRequest {
    pub provider: AiProviderType,
    pub model: String,
    pub segments: Vec<TranscriptionSegment>,
    pub instruction: String,
}

#[derive(Debug, Serialize)]
pub struct PromptCutOutput {
    pub detections: Vec<AiPromptDetection>,
}

#[command]
pub async fn content_prompt_cut(request: PromptCutRequest) -> Result<PromptCutOutput, String> {
    let provider = resolve_provider(request.provider).await?;
    let batch = TranscriptBatch {
        batch_index: 0,
        first_segment_index: 0,
        segments: request.segments,
    };
    let cutter = ProviderCutter {
        provider: provider.as_ref(),
    };
    let detections = cutter
        .detect(&batch, &request.instruction, &request.model)
        .await
        .map_err(|e| e.to_string())?;
    Ok(PromptCutOutput { detections })
}

#[command]
pub async fn content_summary(request: ContentRequest) -> Result<SummaryResult, String> {
    let provider = resolve_provider(request.provider).await?;
    let style = parse_summary_style(request.style.as_deref());
    let language = request.language.unwrap_or_else(|| "English".into());
    let runner = ProviderSummaryRunner {
        provider: provider.as_ref(),
    };
    runner
        .run(&request.segments, style, &language, &request.model, 60.0)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn content_chapters(request: ContentRequest) -> Result<ChapterList, String> {
    let provider = resolve_provider(request.provider).await?;
    let language = request.language.unwrap_or_else(|| "English".into());
    let runner = ProviderChapterRunner {
        provider: provider.as_ref(),
    };
    runner
        .run(&request.segments, &language, &request.model)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateCommandRequest {
    pub provider: AiProviderType,
    pub model: String,
    pub segments: Vec<TranscriptionSegment>,
    /// BCP-47 source language detected by whisper. `None` means
    /// auto-detect; runner will not skip.
    #[serde(default)]
    pub source_language: Option<String>,
    /// Target BCP-47 language. Defaults to `"vi"` per v2 rules.
    #[serde(default)]
    pub target_language: Option<String>,
}

#[command]
pub async fn content_translate(
    request: TranslateCommandRequest,
) -> Result<TranslateResult, String> {
    let provider = resolve_provider(request.provider).await?;
    let options = TranslateOptions {
        target_language: request
            .target_language
            .unwrap_or_else(|| DEFAULT_TARGET_LANGUAGE.to_string()),
        ..TranslateOptions::default()
    };
    let runner = ProviderTranslateRunner {
        provider: provider.as_ref(),
    };
    runner
        .run(
            &request.segments,
            request.source_language.as_deref(),
            &options,
            &request.model,
        )
        .await
        .map_err(|e| e.to_string())
}

fn parse_summary_style(raw: Option<&str>) -> SummaryStyle {
    match raw {
        Some("keyPoints") | Some("key_points") => SummaryStyle::KeyPoints,
        Some("actionItems") | Some("action_items") => SummaryStyle::ActionItems,
        _ => SummaryStyle::Brief,
    }
}

async fn resolve_provider(
    kind: AiProviderType,
) -> Result<Arc<dyn Provider>, String> {
    let registry = build_registry();
    registry
        .get(kind)
        .ok_or_else(|| format!("provider {kind:?} not registered"))
}

/// Build a `ProviderRegistry` with MLX (on Apple Silicon) + any cloud
/// providers that have a keyring entry. Shared with the settings command
/// layer.
fn build_registry() -> ProviderRegistry {
    let store = KeyringSecretStore::new();
    let mut registry = ProviderRegistry::new();

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        use ai_kit::MlxLmProvider;
        let p: Arc<dyn Provider> = Arc::new(MlxLmProvider::default_local());
        registry.register(p);
    }

    if let Some(key) = store.get(AiProviderType::Claude).unwrap_or(None) {
        use ai_kit::ClaudeProvider;
        let p: Arc<dyn Provider> = Arc::new(ClaudeProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::OpenAi).unwrap_or(None) {
        use ai_kit::OpenAiProvider;
        let p: Arc<dyn Provider> = Arc::new(OpenAiProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::Gemini).unwrap_or(None) {
        use ai_kit::GeminiProvider;
        let p: Arc<dyn Provider> = Arc::new(GeminiProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::OpenRouter).unwrap_or(None) {
        use ai_kit::OpenRouterProvider;
        let p: Arc<dyn Provider> = Arc::new(OpenRouterProvider::new(key));
        registry.register(p);
    }
    if let Some(key) = store.get(AiProviderType::Groq).unwrap_or(None) {
        use ai_kit::GroqProvider;
        let p: Arc<dyn Provider> = Arc::new(GroqProvider::new(key));
        registry.register(p);
    }
    use ai_kit::OllamaProvider;
    let host = store
        .get(AiProviderType::Ollama)
        .unwrap_or(None)
        .unwrap_or_else(|| ai_kit::providers::ollama::DEFAULT_HOST.to_string());
    let p: Arc<dyn Provider> = Arc::new(OllamaProvider::new(host));
    registry.register(p);

    registry
}
