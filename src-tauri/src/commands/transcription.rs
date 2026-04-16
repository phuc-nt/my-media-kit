//! Transcription + cache commands.
//!
//! Apple Silicon uses the `mlx_whisper` sidecar; other platforms return
//! a `not implemented` error until the cross-platform backend lands.
//!
//! Results are stashed in `AppState.transcripts` keyed by source path so
//! downstream content-kit features (summary / chapters / filler /
//! translate) read from cache instead of re-invoking whisper.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{command, AppHandle, Emitter, State};

use creator_core::TranscriptionSegment;

use crate::state::{AppState, TranscriptEntry};

/// RAII guard — deletes the temp audio file when dropped.
struct TempAudio(PathBuf);
impl Drop for TempAudio {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Extract a mono 16 kHz 32 kbps MP3 from any media file into `std::env::temp_dir()`.
/// Returns the path + an RAII guard that deletes it on drop.
async fn prepare_audio(source: &Path) -> Result<(PathBuf, TempAudio), String> {
    use uuid::Uuid;
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let tmp = std::env::temp_dir().join(format!("{stem}_asr_{}.mp3", Uuid::new_v4()));
    media_kit::extract_audio_mp3(source, &tmp)
        .await
        .map_err(|e| format!("audio extraction failed: {e}"))?;
    let guard = TempAudio(tmp.clone());
    Ok((tmp, guard))
}

/// Event name used for streaming mlx_whisper progress to the frontend.
pub const PROGRESS_EVENT: &str = "mlx_whisper_progress";

#[derive(Debug, Clone, Serialize)]
pub struct ProgressPayload {
    pub current_ms: i64,
    pub total_ms: i64,
    pub percent: f32,
}

#[derive(Debug, Serialize)]
pub struct TranscribeOutput {
    pub language: Option<String>,
    pub segments: Vec<TranscriptionSegment>,
    pub from_cache: bool,
}

impl TranscribeOutput {
    fn from_entry(entry: Arc<TranscriptEntry>, from_cache: bool) -> Self {
        Self {
            language: entry.language.clone(),
            segments: entry.segments.clone(),
            from_cache,
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[command]
pub async fn mlx_whisper_transcribe(
    path: String,
    language: Option<String>,
    model: Option<String>,
    force: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<TranscribeOutput, String> {
    use std::sync::Arc as StdArc;
    use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

    let source = PathBuf::from(&path);
    let refresh = force.unwrap_or(false);

    if !refresh {
        if let Some(hit) = state.transcript_get(&source) {
            return Ok(TranscribeOutput::from_entry(hit, true));
        }
    }

    // Extract audio-only MP3 before passing to whisper — video track is never
    // needed for transcription and stripping it cuts file size significantly.
    let (audio_path, _audio_guard) = prepare_audio(&source).await?;

    // Probe duration of the original source for the progress bar.
    // Failure is non-fatal — frontend shows an indeterminate bar.
    let total_ms = media_kit::probe_media(&source)
        .await
        .map(|p| p.duration_ms)
        .unwrap_or(0);

    let mut transcriber = MlxWhisperTranscriber::new();
    if let Some(m) = model {
        transcriber = transcriber.with_model(m);
    }
    let mut options = TranscriptionOptions::default();
    options.language = language;

    // Emit an initial 0% event so the UI swaps to progress mode immediately.
    let _ = app.emit(
        PROGRESS_EVENT,
        ProgressPayload { current_ms: 0, total_ms, percent: 0.0 },
    );

    let app_for_cb = app.clone();
    let on_progress: transcription_kit::ProgressCallback = StdArc::new(move |end_ms: i64| {
        let percent = if total_ms > 0 {
            ((end_ms as f32 / total_ms as f32) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        let _ = app_for_cb.emit(
            PROGRESS_EVENT,
            ProgressPayload { current_ms: end_ms, total_ms, percent },
        );
    });

    let segments = transcriber
        .transcribe_file_with_progress(&audio_path, &options, on_progress)
        .await?;

    // Final 100% tick so the frontend hides the bar cleanly.
    let _ = app.emit(
        PROGRESS_EVENT,
        ProgressPayload { current_ms: total_ms, total_ms, percent: 100.0 },
    );

    let language = segments.iter().find_map(|s| s.language.clone());
    let entry = TranscriptEntry { language, segments };
    let arc = state.transcript_put(source, entry);
    Ok(TranscribeOutput::from_entry(arc, false))
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[command]
pub async fn mlx_whisper_transcribe(
    _path: String,
    _language: Option<String>,
    _model: Option<String>,
    _force: Option<bool>,
    _app: AppHandle,
    _state: State<'_, AppState>,
) -> Result<TranscribeOutput, String> {
    Err("mlx_whisper backend is only available on Apple Silicon (macOS aarch64)".into())
}

/// Transcribe using OpenAI Whisper API (`whisper-1`). Works on all platforms.
/// Requires an OpenAI API key stored in the keyring.
/// The audio is first extracted to a 32 kbps mono MP3 to stay under the
/// 25 MB API limit (≈14 MB / hour of audio).
#[command]
pub async fn openai_whisper_transcribe(
    path: String,
    language: Option<String>,
    model: Option<String>,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<TranscribeOutput, String> {
    use ai_kit::{KeyringSecretStore, SecretStore};
    use creator_core::AiProviderType;
    use transcription_kit::OpenAiWhisperTranscriber;

    let source = PathBuf::from(&path);
    let refresh = force.unwrap_or(false);

    if !refresh {
        if let Some(hit) = state.transcript_get(&source) {
            return Ok(TranscribeOutput::from_entry(hit, true));
        }
    }

    let store = KeyringSecretStore::new();
    let api_key = store
        .get(AiProviderType::OpenAi)
        .map_err(|e| format!("keyring error: {e}"))?
        .ok_or_else(|| "OpenAI API key not configured — add it in Settings".to_string())?;

    let (audio_path, _audio_guard) = prepare_audio(&source).await?;

    let transcriber = OpenAiWhisperTranscriber { api_key };
    let segments = transcriber
        .transcribe(&audio_path, language.as_deref(), model.as_deref())
        .await?;

    let language = segments.iter().find_map(|s| s.language.clone());
    let entry = crate::state::TranscriptEntry { language, segments };
    let arc = state.transcript_put(source, entry);
    Ok(TranscribeOutput::from_entry(arc, false))
}

/// Return the cached transcript for a source path, or `None` if we have
/// not transcribed it in this session.
#[command]
pub async fn get_cached_transcript(
    path: String,
    state: State<'_, AppState>,
) -> Result<Option<TranscribeOutput>, String> {
    let source = PathBuf::from(&path);
    Ok(state
        .transcript_get(&source)
        .map(|arc| TranscribeOutput::from_entry(arc, true)))
}

/// Drop cached PCM + transcript for a path (or everything when `path` is
/// `None`). The frontend calls this when the user picks a new source or
/// explicitly wants to rerun from scratch.
#[command]
pub async fn clear_cache(
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    match path {
        Some(p) => state.clear_for(&PathBuf::from(p)),
        None => state.clear_all(),
    }
    Ok(())
}
