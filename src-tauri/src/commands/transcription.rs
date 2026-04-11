//! Transcription commands. Apple Silicon uses the `mlx_whisper` sidecar;
//! other platforms get a `not implemented` error until we wire the
//! cross-platform backend (whisper-rs or similar).

use std::path::PathBuf;

use serde::Serialize;
use tauri::command;

use creator_core::TranscriptionSegment;

#[derive(Debug, Serialize)]
pub struct TranscribeOutput {
    pub language: Option<String>,
    pub segments: Vec<TranscriptionSegment>,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[command]
pub async fn mlx_whisper_transcribe(
    path: String,
    language: Option<String>,
    model: Option<String>,
) -> Result<TranscribeOutput, String> {
    use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

    let mut transcriber = MlxWhisperTranscriber::new();
    if let Some(m) = model {
        transcriber = transcriber.with_model(m);
    }
    let mut options = TranscriptionOptions::default();
    options.language = language;

    let segments = transcriber
        .transcribe_file(&PathBuf::from(path), &options)
        .await?;

    let language = segments.iter().find_map(|s| s.language.clone());
    Ok(TranscribeOutput { language, segments })
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[command]
pub async fn mlx_whisper_transcribe(
    _path: String,
    _language: Option<String>,
    _model: Option<String>,
) -> Result<TranscribeOutput, String> {
    Err("mlx_whisper backend is only available on Apple Silicon (macOS aarch64)".into())
}
