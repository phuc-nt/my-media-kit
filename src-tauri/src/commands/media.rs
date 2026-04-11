//! Media commands — probe a file and extract 16 kHz mono PCM samples via
//! the ffmpeg sidecar. Commands return JSON-friendly values; large sample
//! buffers are not currently streamed to the frontend (they stay inside
//! Rust for silence + transcription work).

use std::path::PathBuf;

use serde::Serialize;
use tauri::command;

use media_kit::{probe::probe_media, probe::extract_pcm_samples};

#[derive(Debug, Serialize)]
pub struct ProbeOutput {
    pub duration_ms: i64,
}

#[command]
pub async fn media_probe(path: String) -> Result<ProbeOutput, String> {
    let probe = probe_media(&PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())?;
    Ok(ProbeOutput {
        duration_ms: probe.duration_ms,
    })
}

#[derive(Debug, Serialize)]
pub struct ExtractSummary {
    pub sample_count: usize,
    pub duration_ms: i64,
}

/// Extracts PCM and returns a summary only. Samples themselves are held in
/// process memory via a future state slot — wired in a follow-up phase.
#[command]
pub async fn media_extract_pcm(path: String) -> Result<ExtractSummary, String> {
    let samples = extract_pcm_samples(&PathBuf::from(path))
        .await
        .map_err(|e| e.to_string())?;
    let duration_ms = (samples.len() as f64 / 16_000.0 * 1000.0).round() as i64;
    Ok(ExtractSummary {
        sample_count: samples.len(),
        duration_ms,
    })
}
