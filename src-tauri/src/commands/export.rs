//! Direct video export via ffmpeg cut+concat. Non-LLM pipeline; takes keep
//! ranges from the frontend, runs ffmpeg, writes the output file.
//!
//! The frontend sends absolute paths; no path juggling happens here so
//! sandbox considerations stay with the Tauri capability system.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::command;

use media_kit::cut_and_concat;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectExportRequest {
    pub source_path: String,
    pub output_path: String,
    pub keep_ranges_ms: Vec<(i64, i64)>,
    /// Video codec for ffmpeg `-c:v`. Defaults to `libx264` which re-encodes
    /// but works everywhere. Use `"copy"` for stream-copy (passthrough) —
    /// only reliable when trims align to keyframes.
    pub video_codec: Option<String>,
    /// Audio codec for ffmpeg `-c:a`. Defaults to `aac`.
    pub audio_codec: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DirectExportResult {
    pub output_path: String,
    pub size_bytes: u64,
}

#[command]
pub async fn export_video_direct(
    request: DirectExportRequest,
) -> Result<DirectExportResult, String> {
    let src = PathBuf::from(&request.source_path);
    let out = PathBuf::from(&request.output_path);
    let video_codec = request.video_codec.as_deref().unwrap_or("libx264");
    let audio_codec = request.audio_codec.as_deref().unwrap_or("aac");

    cut_and_concat(
        &src,
        &out,
        &request.keep_ranges_ms,
        video_codec,
        audio_codec,
    )
    .await
    .map_err(|e| e.to_string())?;

    let size_bytes = std::fs::metadata(&out)
        .map(|m| m.len())
        .unwrap_or_default();

    Ok(DirectExportResult {
        output_path: request.output_path,
        size_bytes,
    })
}
