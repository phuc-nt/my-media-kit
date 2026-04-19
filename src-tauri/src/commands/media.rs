//! Media probe — duration, resolution, frame rate, audio channel count via
//! the ffmpeg sidecar. Used by the source manager to render file info.

use std::path::PathBuf;

use serde::Serialize;
use tauri::command;

use media_kit::probe::probe_media_full;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutput {
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub audio_channels: u8,
}

#[command]
pub async fn media_probe(path: String) -> Result<ProbeOutput, String> {
    let p = probe_media_full(&PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())?;
    Ok(ProbeOutput {
        duration_ms: p.duration_ms,
        width: p.width,
        height: p.height,
        frame_rate: p.frame_rate,
        audio_channels: p.audio_channels,
    })
}
