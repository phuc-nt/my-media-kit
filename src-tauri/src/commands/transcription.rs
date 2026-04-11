//! Transcription + cache commands.
//!
//! Apple Silicon uses the `mlx_whisper` sidecar; other platforms return
//! a `not implemented` error until the cross-platform backend lands.
//!
//! Results are stashed in `AppState.transcripts` keyed by source path so
//! downstream content-kit features (summary / chapters / filler /
//! translate) read from cache instead of re-invoking whisper.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{command, State};

use creator_core::TranscriptionSegment;

use crate::state::{AppState, TranscriptEntry};

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
    state: State<'_, AppState>,
) -> Result<TranscribeOutput, String> {
    use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

    let source = PathBuf::from(&path);
    let refresh = force.unwrap_or(false);

    if !refresh {
        if let Some(hit) = state.transcript_get(&source) {
            return Ok(TranscribeOutput::from_entry(hit, true));
        }
    }

    let mut transcriber = MlxWhisperTranscriber::new();
    if let Some(m) = model {
        transcriber = transcriber.with_model(m);
    }
    let mut options = TranscriptionOptions::default();
    options.language = language;

    let segments = transcriber
        .transcribe_file(&source, &options)
        .await?;

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
    _state: State<'_, AppState>,
) -> Result<TranscribeOutput, String> {
    Err("mlx_whisper backend is only available on Apple Silicon (macOS aarch64)".into())
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
