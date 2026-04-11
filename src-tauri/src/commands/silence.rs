//! Silence detection commands. Runs the pipeline end-to-end:
//!   1. Reuse cached PCM for `path` if available, else ffmpeg decode to
//!      16 kHz mono f32 PCM and cache it
//!   2. silence-kit detection with the config the frontend passed
//!   3. Return JSON regions (ms ranges) so the timeline UI can render them
//!
//! Slider live-preview works because step 1 hits the cache on every call
//! after the first: the frontend can mutate sliders locally, debounce,
//! re-invoke this command, and still get sub-100 ms responses.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{command, State};

use creator_core::{SilenceDetectorConfig, SilenceRegion};
use media_kit::probe::extract_pcm_samples;
use silence_kit::detect_silence;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct SilenceDetectionOutput {
    pub regions: Vec<SilenceRegion>,
    pub frame_count: usize,
    pub from_cache: bool,
}

#[command]
pub async fn detect_silence_in_file(
    path: String,
    config: SilenceDetectorConfig,
    state: State<'_, AppState>,
) -> Result<SilenceDetectionOutput, String> {
    let source = PathBuf::from(&path);

    let (samples, from_cache) = match state.pcm_get(&source) {
        Some(arc) => (arc, true),
        None => {
            let samples = extract_pcm_samples(&source)
                .await
                .map_err(|e| e.to_string())?;
            let arc = state.pcm_put(source.clone(), samples);
            (arc, false)
        }
    };

    let result = detect_silence(&samples, &config, None);
    Ok(SilenceDetectionOutput {
        frame_count: result.rms_values.len(),
        regions: result.regions,
        from_cache,
    })
}
