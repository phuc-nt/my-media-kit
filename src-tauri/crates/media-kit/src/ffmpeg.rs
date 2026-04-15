//! ffmpeg command-line builders + binary resolution.
//!
//! Pure functions take the parameters, return the argument vector. Callers
//! hand the vector to `tokio::process::Command` — that execution layer is
//! small, testable via command echo, and lives in `probe.rs` and future
//! export helpers.

use std::path::{Path, PathBuf};

use crate::error::MediaError;
use crate::{TARGET_CHANNELS, TARGET_SAMPLE_RATE};

/// Resolved sidecar location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegBinary {
    pub path: PathBuf,
}

impl FfmpegBinary {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// Locate ffmpeg on this machine. Env override → PATH lookup. Bundled
/// sidecar resolution lives in the Tauri command layer because it needs
/// `AppHandle::path()`.
pub fn resolve_ffmpeg_binary() -> Result<FfmpegBinary, MediaError> {
    resolve_binary("FFMPEG", "ffmpeg")
}

/// Locate ffprobe. Same rules as `resolve_ffmpeg_binary`.
pub fn resolve_ffprobe_binary() -> Result<FfmpegBinary, MediaError> {
    resolve_binary("FFPROBE", "ffprobe")
}

fn resolve_binary(env_var: &str, default_name: &str) -> Result<FfmpegBinary, MediaError> {
    if let Ok(v) = std::env::var(env_var) {
        if !v.is_empty() {
            return Ok(FfmpegBinary::new(v));
        }
    }
    which_binary(default_name).map(FfmpegBinary::new).ok_or_else(|| {
        MediaError::BinaryNotFound(format!(
            "`{default_name}` not found in {env_var} env var or PATH"
        ))
    })
}

/// Minimal `which` — walks `PATH` entries and checks for an executable file.
/// Kept in-tree so the crate doesn't pull the whole `which` dep.
fn which_binary(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if is_executable(&exe) {
                return Some(exe);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(m) => m.is_file() && (m.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Build the ffmpeg command that decodes the input to a WAV stream on stdout
/// at the whisper-expected format (16 kHz mono PCM f32le).
///
/// The output is a WAV container (not raw PCM) so the caller can use the
/// header to verify format / sample count before decoding.
pub fn build_extract_pcm_args(input: &Path) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vn".into(),
        "-ac".into(),
        TARGET_CHANNELS.to_string(),
        "-ar".into(),
        TARGET_SAMPLE_RATE.to_string(),
        "-acodec".into(),
        "pcm_f32le".into(),
        "-f".into(),
        "wav".into(),
        "pipe:1".into(),
    ]
}

/// Build the ffprobe command that prints the media duration (seconds, float)
/// to stdout. Output is a single line; parse with `trim().parse::<f64>()`.
pub fn build_probe_duration_args(input: &Path) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-show_entries".into(),
        "format=duration".into(),
        "-of".into(),
        "default=noprint_wrappers=1:nokey=1".into(),
        input.to_string_lossy().into_owned(),
    ]
}

/// Build an ffprobe command that returns a JSON blob with stream + format
/// info. Parse result with `probe.rs::parse_probe_full_json`.
///
/// JSON shape:
/// ```json
/// { "streams": [
///     {"codec_type":"video","width":1920,"height":1080,"r_frame_rate":"30/1"},
///     {"codec_type":"audio","channels":2}
///   ],
///   "format": {"duration":"87.43"}
/// }
/// ```
pub fn build_probe_full_args(input: &Path) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-show_streams".into(),
        "-show_entries".into(),
        "stream=codec_type,width,height,r_frame_rate,channels:format=duration".into(),
        "-of".into(),
        "json".into(),
        input.to_string_lossy().into_owned(),
    ]
}

/// Build an ffmpeg invocation that cuts the input into the specified keep
/// ranges and concatenates them into `output`. Uses the concat demuxer via
/// multiple trims in a filter_complex — slower than stream copy but robust
/// across codecs and does not require re-encoding for the trims themselves.
///
/// `keep_ranges_ms` is a list of `[start_ms, end_ms)` in source timebase.
/// Output codec + bitrate let the caller pick between passthrough-ish HEVC
/// re-encode (default) and a provided codec.
///
/// Errors if the range list is empty or has an inverted interval.
pub fn build_cut_and_concat_args(
    input: &Path,
    output: &Path,
    keep_ranges_ms: &[(i64, i64)],
    video_codec: &str,
    audio_codec: &str,
) -> Result<Vec<String>, MediaError> {
    if keep_ranges_ms.is_empty() {
        return Err(MediaError::InvalidArgument(
            "keep_ranges_ms must not be empty".into(),
        ));
    }
    for (i, (s, e)) in keep_ranges_ms.iter().enumerate() {
        if *e <= *s {
            return Err(MediaError::InvalidArgument(format!(
                "range {i} is not positive-length: {s}..{e}"
            )));
        }
    }

    let n = keep_ranges_ms.len();
    let mut filter = String::new();
    for (i, (start_ms, end_ms)) in keep_ranges_ms.iter().enumerate() {
        let start_s = *start_ms as f64 / 1000.0;
        let end_s = *end_ms as f64 / 1000.0;
        filter.push_str(&format!(
            "[0:v]trim=start={start_s}:end={end_s},setpts=PTS-STARTPTS[v{i}];\
             [0:a]atrim=start={start_s}:end={end_s},asetpts=PTS-STARTPTS[a{i}];"
        ));
    }
    for i in 0..n {
        filter.push_str(&format!("[v{i}][a{i}]"));
    }
    filter.push_str(&format!("concat=n={n}:v=1:a=1[outv][outa]"));

    Ok(vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "[outv]".into(),
        "-map".into(),
        "[outa]".into(),
        "-c:v".into(),
        video_codec.into(),
        "-c:a".into(),
        audio_codec.into(),
        output.to_string_lossy().into_owned(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pcm_args_contain_target_format() {
        let args = build_extract_pcm_args(Path::new("/tmp/video.mov"));
        assert!(args.iter().any(|a| a == "pcm_f32le"));
        assert!(args.iter().any(|a| a == "wav"));
        assert!(args.iter().any(|a| a == "16000"));
        assert!(args.iter().any(|a| a == "1"));
        assert!(args.iter().any(|a| a == "pipe:1"));
    }

    #[test]
    fn probe_duration_args_are_ffprobe_shape() {
        let args = build_probe_duration_args(Path::new("/tmp/video.mov"));
        assert!(args.iter().any(|a| a == "format=duration"));
        assert!(args.iter().any(|a| a.ends_with("video.mov")));
    }

    #[test]
    fn cut_concat_filter_has_trim_per_range() {
        let args = build_cut_and_concat_args(
            Path::new("/in.mov"),
            Path::new("/out.mp4"),
            &[(0, 1_000), (2_000, 3_500)],
            "libx264",
            "aac",
        )
        .unwrap();
        let filter = args
            .iter()
            .find(|a| a.contains("trim="))
            .expect("filter arg missing");
        assert!(filter.contains("trim=start=0:end=1"));
        assert!(filter.contains("trim=start=2:end=3.5"));
        assert!(filter.contains("concat=n=2:v=1:a=1"));
    }

    #[test]
    fn cut_concat_rejects_empty_ranges() {
        let e = build_cut_and_concat_args(
            Path::new("/in.mov"),
            Path::new("/out.mp4"),
            &[],
            "libx264",
            "aac",
        )
        .unwrap_err();
        assert!(matches!(e, MediaError::InvalidArgument(_)));
    }

    #[test]
    fn cut_concat_rejects_inverted_range() {
        let e = build_cut_and_concat_args(
            Path::new("/in.mov"),
            Path::new("/out.mp4"),
            &[(2_000, 1_000)],
            "libx264",
            "aac",
        )
        .unwrap_err();
        assert!(matches!(e, MediaError::InvalidArgument(_)));
    }
}
