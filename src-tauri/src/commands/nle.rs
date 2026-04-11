//! NLE export commands. The frontend passes a list of keep ranges + source
//! metadata, we call nle-kit and write the resulting bytes to disk.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::command;

use creator_core::NleExportTarget;
use nle_kit::{build_project, NleExportInput};

#[derive(Debug, Deserialize)]
pub struct NleExportRequest {
    pub source_path: String,
    pub output_path: String,
    pub asset_name: String,
    pub project_name: String,
    pub total_duration_ms: i64,
    pub frame_rate: f64,
    pub width: u32,
    pub height: u32,
    pub audio_channels: u8,
    pub keep_ranges_ms: Vec<(i64, i64)>,
    pub target: NleExportTarget,
}

#[derive(Debug, Serialize)]
pub struct NleExportResult {
    pub output_path: String,
    pub size_bytes: usize,
}

#[command]
pub async fn nle_export(request: NleExportRequest) -> Result<NleExportResult, String> {
    let input = NleExportInput {
        source_path: PathBuf::from(&request.source_path),
        asset_name: request.asset_name,
        project_name: request.project_name,
        total_duration_ms: request.total_duration_ms,
        frame_rate: request.frame_rate,
        height: request.height,
        width: request.width,
        audio_channels: request.audio_channels,
        keep_ranges_ms: request.keep_ranges_ms,
    };
    let bytes = build_project(&input, request.target).map_err(|e| e)?;
    let size_bytes = bytes.len();
    std::fs::write(&request.output_path, bytes).map_err(|e| e.to_string())?;
    Ok(NleExportResult {
        output_path: request.output_path,
        size_bytes,
    })
}
