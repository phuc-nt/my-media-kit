//! Silence detection commands. Runs the pipeline end-to-end:
//!   1. ffmpeg decode to 16 kHz mono f32 PCM
//!   2. silence-kit detection with the config the frontend passed
//!   3. Return JSON regions (ms ranges) so the timeline UI can render them
//!
//! The PCM buffer is held in-process only; slider live-preview will be
//! wired once the frontend timeline lands (Phase 9) via a cached-samples
//! state slot keyed by `path`.

use std::path::PathBuf;

use serde::Serialize;
use tauri::command;

use creator_core::{SilenceDetectorConfig, SilenceRegion};
use media_kit::probe::extract_pcm_samples;
use silence_kit::detect_silence;

#[derive(Debug, Serialize)]
pub struct SilenceDetectionOutput {
    pub regions: Vec<SilenceRegion>,
    pub frame_count: usize,
}

#[command]
pub async fn detect_silence_in_file(
    path: String,
    config: SilenceDetectorConfig,
) -> Result<SilenceDetectionOutput, String> {
    let samples = extract_pcm_samples(&PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())?;
    let result = detect_silence(&samples, &config, None);
    Ok(SilenceDetectionOutput {
        frame_count: result.rms_values.len(),
        regions: result.regions,
    })
}
