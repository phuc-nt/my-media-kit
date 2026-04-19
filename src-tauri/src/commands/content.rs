//! Content-feature commands: summary, chapters, translate, YT pack, viral
//! clips. All route through the configured provider. On Apple Silicon the
//! default is MLX (local); other platforms fall back to OpenAI.

use serde::Deserialize;
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter};

use ai_kit::{Provider, SecretStore, KeyringSecretStore};
use content_kit::{
    batch::chunk_segments,
    chapters::{ChapterList, ChapterRunner, ProviderChapterRunner},
    summary::{ProviderSummaryRunner, SummaryResult, SummaryRunner, SummaryStyle},
    transcript_filler_scan,
    translate::{
        align_to_originals, language_display_name, should_skip, translate_batch_with_retry,
        TranslateResult, DEFAULT_BATCH_SECONDS, DEFAULT_TARGET_LANGUAGE,
    },
    viral_clips::{ProviderViralClipRunner, ViralClipList, ViralClipRunner},
    youtube_pack::{ProviderYouTubePackRunner, YouTubePack, YouTubePackRunner},
};
use creator_core::{AiProviderType, TranscriptionSegment};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRequest {
    pub provider: AiProviderType,
    pub model: String,
    pub segments: Vec<TranscriptionSegment>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    /// Free-form instruction override for summary — replaces the default style instruction.
    #[serde(default)]
    pub custom_instruction: Option<String>,
    /// Optional summary injected into system prompt for context (used by chapters/yt-pack/viral).
    #[serde(default)]
    pub summary_hint: Option<String>,
}

#[command]
pub async fn content_summary(request: ContentRequest) -> Result<SummaryResult, String> {
    let provider = resolve_provider(request.provider).await?;
    let style = parse_summary_style(request.style.as_deref());
    let language = request.language.unwrap_or_else(|| "English".into());
    let runner = ProviderSummaryRunner {
        provider: provider.as_ref(),
    };
    // Single-pass for transcripts under 30 minutes — avoids slow multi-batch
    // + consolidation (was 60s batching → 5+ LLM calls for a 5-min video).
    runner
        .run(
            &request.segments,
            style,
            &language,
            &request.model,
            1800.0,
            request.custom_instruction.as_deref(),
        )
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
        .run(&request.segments, &language, &request.model, request.summary_hint.as_deref())
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
    /// Optional content summary injected into the system prompt so the model
    /// can maintain consistent terminology across batches.
    #[serde(default)]
    pub summary_hint: Option<String>,
}

/// Tauri event emitted after each batch finishes. Payload: `{ batch, total, percent }`.
pub const TRANSLATE_PROGRESS_EVENT: &str = "translate_progress";

#[command]
pub async fn content_translate(
    app: AppHandle,
    request: TranslateCommandRequest,
) -> Result<TranslateResult, String> {
    let target_language = request
        .target_language
        .unwrap_or_else(|| DEFAULT_TARGET_LANGUAGE.to_string());

    if should_skip(request.source_language.as_deref(), &target_language) {
        return Ok(TranslateResult {
            target_language,
            source_language: request.source_language,
            skipped: true,
            segments: request.segments,
        });
    }

    if request.segments.is_empty() {
        return Ok(TranslateResult {
            target_language,
            source_language: request.source_language,
            skipped: false,
            segments: Vec::new(),
        });
    }

    let provider = resolve_provider(request.provider).await?;
    let target_name = language_display_name(&target_language);
    let batches = chunk_segments(&request.segments, DEFAULT_BATCH_SECONDS);
    let total = batches.len();
    let mut out = Vec::with_capacity(request.segments.len());
    let mut prev_context: Vec<String> = Vec::new();

    for (i, batch) in batches.iter().enumerate() {
        let _ = app.emit(TRANSLATE_PROGRESS_EVENT, serde_json::json!({
            "batch": i + 1,
            "total": total,
            "percent": ((i as f64) / (total as f64)) * 100.0,
        }));

        let translations = translate_batch_with_retry(
            provider.as_ref(),
            &request.model,
            batch,
            target_name,
            request.summary_hint.as_deref(),
            &prev_context,
        )
        .await
        .map_err(|e| e.to_string())?;

        let aligned = align_to_originals(&batch.segments, translations);
        // Slide context window: keep last 5 translated texts for the next batch.
        prev_context.extend(aligned.iter().cloned());
        if prev_context.len() > 5 {
            let drain = prev_context.len() - 5;
            prev_context.drain(..drain);
        }
        for (original, translated_text) in batch.segments.iter().zip(aligned) {
            let mut seg = original.clone();
            seg.text = translated_text;
            seg.language = Some(target_language.clone());
            out.push(seg);
        }
    }

    Ok(TranslateResult {
        target_language,
        source_language: request.source_language,
        skipped: false,
        segments: out,
    })
}

#[command]
pub async fn content_youtube_pack(request: ContentRequest) -> Result<YouTubePack, String> {
    let provider = resolve_provider(request.provider).await?;
    let language = request.language.unwrap_or_else(|| "English".into());
    let runner = ProviderYouTubePackRunner {
        provider: provider.as_ref(),
    };
    runner
        .run(&request.segments, &language, &request.model, request.summary_hint.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn content_viral_clips(request: ContentRequest) -> Result<ViralClipList, String> {
    let provider = resolve_provider(request.provider).await?;
    let language = request.language.unwrap_or_else(|| "English".into());
    let runner = ProviderViralClipRunner {
        provider: provider.as_ref(),
    };
    runner
        .run(&request.segments, &language, &request.model, request.summary_hint.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn content_clean_transcript(
    segments: Vec<TranscriptionSegment>,
) -> Result<Vec<TranscriptionSegment>, String> {
    let fillers = transcript_filler_scan::scan_word_timestamps(&segments, 100);
    if fillers.is_empty() {
        return Ok(segments);
    }
    let cleaned = segments
        .iter()
        .map(|seg| {
            if seg.words.is_empty() {
                return seg.clone();
            }
            let clean_words: Vec<_> = seg
                .words
                .iter()
                .filter(|w| {
                    !fillers
                        .iter()
                        .any(|f| w.start_ms >= f.cut_start_ms && w.end_ms <= f.cut_end_ms)
                })
                .cloned()
                .collect();
            let text = clean_words
                .iter()
                .map(|w| w.text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            let mut s = seg.clone();
            s.text = text;
            s.words = clean_words;
            s
        })
        .collect();
    Ok(cleaned)
}

fn parse_summary_style(raw: Option<&str>) -> SummaryStyle {
    match raw {
        Some("keyPoints") | Some("key_points") => SummaryStyle::KeyPoints,
        Some("actionItems") | Some("action_items") => SummaryStyle::ActionItems,
        _ => SummaryStyle::Brief,
    }
}

/// Resolve only the requested provider — avoids querying the keychain for
/// every cloud provider when only one (e.g. MLX) is actually needed.
async fn resolve_provider(kind: AiProviderType) -> Result<Arc<dyn Provider>, String> {
    let store = KeyringSecretStore::new();
    match kind {
        AiProviderType::Mlx => {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                use ai_kit::MlxLmProvider;
                return Ok(Arc::new(MlxLmProvider::default_local()));
            }
            #[allow(unreachable_code)]
            Err("MLX provider is only available on Apple Silicon".into())
        }
        AiProviderType::OpenAi => {
            let key = store.get(AiProviderType::OpenAi).unwrap_or(None)
                .ok_or("OpenAI API key not set — add it in Settings")?;
            use ai_kit::OpenAiProvider;
            Ok(Arc::new(OpenAiProvider::new(key)))
        }
        AiProviderType::Claude => {
            let key = store.get(AiProviderType::Claude).unwrap_or(None)
                .ok_or("Claude API key not set — add it in Settings")?;
            use ai_kit::ClaudeProvider;
            Ok(Arc::new(ClaudeProvider::new(key)))
        }
        AiProviderType::Gemini => {
            let key = store.get(AiProviderType::Gemini).unwrap_or(None)
                .ok_or("Gemini API key not set — add it in Settings")?;
            use ai_kit::GeminiProvider;
            Ok(Arc::new(GeminiProvider::new(key)))
        }
        AiProviderType::OpenRouter => {
            let key = store.get(AiProviderType::OpenRouter).unwrap_or(None)
                .ok_or("OpenRouter API key not set — add it in Settings")?;
            use ai_kit::OpenRouterProvider;
            Ok(Arc::new(OpenRouterProvider::new(key)))
        }
        AiProviderType::Ollama => {
            use ai_kit::OllamaProvider;
            use ai_kit::providers::ollama::DEFAULT_HOST;
            let host = store.get(AiProviderType::Ollama).unwrap_or(None)
                .unwrap_or_else(|| DEFAULT_HOST.to_string());
            Ok(Arc::new(OllamaProvider::new(host)))
        }
        AiProviderType::AppleIntelligence => {
            Err("Apple Intelligence provider not yet implemented".into())
        }
    }
}

