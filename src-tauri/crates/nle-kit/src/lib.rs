//! nle-kit — XML builders for non-linear editors.
//!
//! Produces **FCPXML 1.11** (Final Cut Pro) and **xmeml v5** (Premiere /
//! DaVinci Resolve) from a shared `NleExportInput`. Both formats are
//! non-destructive: the output timeline references the source media and
//! ffmpeg is never touched here.
//!
//! Port of the Swift reference documented in `docs/06-nle-export.md`. Uses
//! the `writer` module (thin wrapper over `quick-xml::Writer`) to avoid
//! string-concat bugs and ensure attribute escaping.

pub mod fcpxml;
pub mod xmeml;

mod writer;

pub use fcpxml::build_fcpxml;
pub use xmeml::build_xmeml;

use std::path::PathBuf;

use creator_core::NleExportTarget;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared input shape for all NLE exporters. Keep regions already point at
/// source media timecodes in milliseconds.
#[derive(Debug, Clone)]
pub struct NleExportInput {
    /// Absolute path to the source media file. Used to build `file://` URLs
    /// in FCPXML and `<pathurl>` fields in xmeml.
    pub source_path: PathBuf,
    /// Human-readable clip name (displayed in the NLE bin panel).
    pub asset_name: String,
    /// FCPXML library event / Premiere project name.
    pub project_name: String,
    /// Total source duration in milliseconds.
    pub total_duration_ms: i64,
    /// Frame rate of the source media. Standard values: 23.976, 24, 25,
    /// 29.97, 30, 50, 59.94, 60.
    pub frame_rate: f64,
    /// Video resolution height in pixels (used for format name,
    /// e.g. 1080 → "FFVideoFormat1080p30").
    pub height: u32,
    /// Video resolution width in pixels.
    pub width: u32,
    /// Number of audio channels in the source file.
    pub audio_channels: u8,
    /// Keep ranges `(start_ms, end_ms)` in source timebase. Sequential
    /// `asset-clip` / `clipitem` entries are emitted in this order.
    pub keep_ranges_ms: Vec<(i64, i64)>,
}

/// Build the NLE project file for the requested target and return its bytes.
pub fn build_project(input: &NleExportInput, target: NleExportTarget) -> Result<Vec<u8>, String> {
    match target {
        NleExportTarget::FinalCutPro => build_fcpxml(input),
        NleExportTarget::Premiere | NleExportTarget::DavinciResolve => build_xmeml(input),
    }
}
