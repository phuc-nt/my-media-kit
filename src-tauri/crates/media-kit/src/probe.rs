//! Media probing + audio extraction runners. These use tokio to spawn the
//! ffmpeg/ffprobe sidecar resolved by `ffmpeg.rs`.
//!
//! Only the orchestration logic lives here; the command strings themselves
//! are built by `ffmpeg::build_*_args` so the argument shape stays unit-
//! testable.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

use crate::error::MediaError;
use crate::ffmpeg::{
    build_cut_and_concat_args, build_extract_pcm_args, build_probe_duration_args,
    resolve_ffmpeg_binary, resolve_ffprobe_binary,
};
use crate::wav::parse_wav_f32_mono;

/// Summary of a media file. Extend with resolution/fps when the UI needs it.
#[derive(Debug, Clone)]
pub struct MediaProbe {
    pub duration_ms: i64,
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
