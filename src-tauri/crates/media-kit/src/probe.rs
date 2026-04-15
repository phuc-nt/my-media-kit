//! Media probing + audio extraction runners. These use tokio to spawn the
//! ffmpeg/ffprobe sidecar resolved by `ffmpeg.rs`.
//!
//! Only the orchestration logic lives here; the command strings themselves
//! are built by `ffmpeg::build_*_args` so the argument shape stays unit-
//! testable.

use std::path::Path;
use std::process::Stdio;

use serde_json;

use tokio::process::Command;

use crate::error::MediaError;
use crate::ffmpeg::{
    build_cut_and_concat_args, build_extract_pcm_args, build_probe_duration_args,
    build_probe_full_args, resolve_ffmpeg_binary, resolve_ffprobe_binary,
};
use crate::wav::parse_wav_f32_mono;

/// Basic duration-only probe result (legacy, kept for internal use).
#[derive(Debug, Clone)]
pub struct MediaProbe {
    pub duration_ms: i64,
}

/// Full media probe: duration, video dimensions + frame rate, audio channels.
/// Falls back to sensible defaults when a stream is absent.
#[derive(Debug, Clone)]
pub struct MediaProbeFull {
    pub duration_ms: i64,
    /// Video width in pixels (default 1920 if no video stream).
    pub width: u32,
    /// Video height in pixels (default 1080 if no video stream).
    pub height: u32,
    /// Frames per second, e.g. 29.97 (default 30.0 if unavailable).
    pub frame_rate: f64,
    /// Number of audio channels (default 2 if no audio stream).
    pub audio_channels: u8,
}

/// Probe the duration of a media file via ffprobe.
pub async fn probe_media(input: &Path) -> Result<MediaProbe, MediaError> {
    let bin = resolve_ffprobe_binary()?;
    let args = build_probe_duration_args(input);

    let output = Command::new(bin.as_path())
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| MediaError::Spawn(e.to_string()))?;

    if !output.status.success() {
        return Err(MediaError::ExitFailed {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let seconds: f64 = stdout
        .trim()
        .parse()
        .map_err(|e: std::num::ParseFloatError| MediaError::InvalidMedia(e.to_string()))?;
    Ok(MediaProbe {
        duration_ms: (seconds * 1000.0).round() as i64,
    })
}

/// Probe full media metadata: duration, video resolution + fps, audio channels.
/// One ffprobe call; falls back to defaults for any missing stream/field.
pub async fn probe_media_full(input: &Path) -> Result<MediaProbeFull, MediaError> {
    let bin = resolve_ffprobe_binary()?;
    let args = build_probe_full_args(input);

    let output = Command::new(bin.as_path())
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| MediaError::Spawn(e.to_string()))?;

    if !output.status.success() {
        return Err(MediaError::ExitFailed {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| MediaError::InvalidMedia(e.to_string()))?;

    let mut width = 1920u32;
    let mut height = 1080u32;
    let mut frame_rate = 30.0f64;
    let mut audio_channels = 2u8;

    if let Some(streams) = json.get("streams").and_then(|v| v.as_array()) {
        for s in streams {
            match s.get("codec_type").and_then(|v| v.as_str()) {
                Some("video") => {
                    if let Some(w) = s.get("width").and_then(|v| v.as_u64()) {
                        width = w as u32;
                    }
                    if let Some(h) = s.get("height").and_then(|v| v.as_u64()) {
                        height = h as u32;
                    }
                    if let Some(r) = s.get("r_frame_rate").and_then(|v| v.as_str()) {
                        if let Some(fps) = parse_rational_fps(r) {
                            frame_rate = fps;
                        }
                    }
                }
                Some("audio") => {
                    if let Some(ch) = s.get("channels").and_then(|v| v.as_u64()) {
                        audio_channels = ch as u8;
                    }
                }
                _ => {}
            }
        }
    }

    let duration_ms = json
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|s| (s * 1000.0).round() as i64)
        .unwrap_or(0);

    Ok(MediaProbeFull {
        duration_ms,
        width,
        height,
        frame_rate,
        audio_channels,
    })
}

/// Parse "num/den" rational string (ffprobe r_frame_rate) to f64.
fn parse_rational_fps(s: &str) -> Option<f64> {
    let mut parts = s.splitn(2, '/');
    let num: f64 = parts.next()?.parse().ok()?;
    let den: f64 = parts.next()?.parse().ok()?;
    if den == 0.0 {
        return None;
    }
    Some(num / den)
}

/// Extract 16 kHz mono f32 PCM samples from an arbitrary media file. Runs
/// ffmpeg to produce a WAV bytestream on stdout, then feeds it through the
/// in-tree WAV parser.
pub async fn extract_pcm_samples(input: &Path) -> Result<Vec<f32>, MediaError> {
    let bin = resolve_ffmpeg_binary()?;
    let args = build_extract_pcm_args(input);

    let output = Command::new(bin.as_path())
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| MediaError::Spawn(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("does not contain any stream") || stderr.contains("Stream map") {
            return Err(MediaError::NoAudioTrack);
        }
        return Err(MediaError::ExitFailed {
            status: output.status.code().unwrap_or(-1),
            stderr: stderr.into_owned(),
        });
    }

    parse_wav_f32_mono(&output.stdout)
}

/// Run the cut-and-concat pipeline on a source file, writing the result to
/// `output`. This is the non-LLM "direct export" path used by the AutoCut
/// view when the user wants a final video instead of an NLE project file.
pub async fn cut_and_concat(
    input: &Path,
    output: &Path,
    keep_ranges_ms: &[(i64, i64)],
    video_codec: &str,
    audio_codec: &str,
) -> Result<(), MediaError> {
    let bin = resolve_ffmpeg_binary()?;
    let args = build_cut_and_concat_args(input, output, keep_ranges_ms, video_codec, audio_codec)?;

    let result = Command::new(bin.as_path())
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| MediaError::Spawn(e.to_string()))?;

    if !result.status.success() {
        return Err(MediaError::ExitFailed {
            status: result.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
        });
    }
    Ok(())
}
