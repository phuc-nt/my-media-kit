//! mlx-whisper sidecar backend.
//!
//! Spawns the `mlx_whisper` CLI (installed via `pip install mlx-whisper`) as
//! a subprocess, points it at an audio file, and parses the resulting JSON
//! into the shared `TranscriptionSegment` shape. Active only on Apple
//! Silicon (see ADR-012).
//!
//! Flow:
//!   1. Caller passes a file path (preferred) or samples buffer.
//!   2. If samples: we write them to a temp WAV first using a minimal
//!      in-tree writer so we don't force callers to decode twice.
//!   3. Spawn `mlx_whisper <path> --model <model_id> --word-timestamps True
//!      --output-format json --output-dir <tmp>`.
//!   4. Read `<tmp>/<stem>.json`, parse, convert seconds → ms, return.
//!
//! Error surface:
//!   - mlx_whisper not on PATH → bubble up as `"mlx_whisper not found: ..."`.
//!   - non-zero exit → include stderr snippet.
//!   - JSON parse error → include the offending field name.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use creator_core::{AbortFlag, TranscriptionSegment, WordTimestamp};

use crate::transcriber::{Transcriber, TranscriptionOptions};

/// Default mlx-whisper model — pre-downloaded on the primary dev machine.
/// Override via env var `CREATOR_UTILS_MLX_WHISPER_MODEL` or builder arg.
pub const DEFAULT_MODEL: &str = "mlx-community/whisper-large-v3-turbo";

pub struct MlxWhisperTranscriber {
    model: String,
    binary: PathBuf,
}

impl MlxWhisperTranscriber {
    pub fn new() -> Self {
        let model = std::env::var("CREATOR_UTILS_MLX_WHISPER_MODEL")
            .unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        let binary = which_binary("mlx_whisper")
            .unwrap_or_else(|| PathBuf::from("mlx_whisper"));
        Self { model, binary }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_binary(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary = path.into();
        self
    }

    pub async fn transcribe_file(
        &self,
        audio_path: &Path,
        options: &TranscriptionOptions,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        let tmp_dir = std::env::temp_dir().join(format!(
            "creator_utils_mlx_whisper_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| format!("create tmp dir: {e}"))?;

        let stem = "transcript";
        let mut cmd = Command::new(&self.binary);
        cmd.arg(audio_path)
            .args([
                "--model",
                &self.model,
                "--word-timestamps",
                "True",
                "--output-format",
                "json",
                "--output-dir",
                tmp_dir.to_string_lossy().as_ref(),
                "--output-name",
                stem,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(lang) = &options.language {
            cmd.arg("--language").arg(lang);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("spawn mlx_whisper: {e}"))?;

        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(format!(
                "mlx_whisper exited with status {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let json_path = tmp_dir.join(format!("{stem}.json"));
        let body = std::fs::read_to_string(&json_path)
            .map_err(|e| format!("read {}: {e}", json_path.display()))?;
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let parsed: MlxWhisperOutput =
            serde_json::from_str(&body).map_err(|e| format!("parse json: {e}"))?;

        Ok(parsed.into_segments())
    }
}

impl Default for MlxWhisperTranscriber {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transcriber for MlxWhisperTranscriber {
    async fn transcribe(
        &self,
        _samples: &[f32],
        _options: &TranscriptionOptions,
        _abort: AbortFlag,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        // We require a file path because mlx_whisper is a CLI that reads
        // from disk. Callers that only have samples should write them to
        // a temp WAV first via media-kit (mlx_whisper accepts WAV).
        Err("MlxWhisperTranscriber: use transcribe_file(&Path, ...) — samples-based flow not supported".into())
    }
}

/// Schema of a single `mlx_whisper --output-format json` file. Only the
/// fields we need are captured; extras are ignored via `serde`'s default
/// behavior.
#[derive(Debug, Deserialize, Serialize)]
struct MlxWhisperOutput {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    segments: Vec<MlxSegment>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MlxSegment {
    start: f64,
    end: f64,
    text: String,
    #[serde(default)]
    words: Vec<MlxWord>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MlxWord {
    word: String,
    start: f64,
    end: f64,
    #[serde(default)]
    probability: Option<f64>,
}

impl MlxWhisperOutput {
    fn into_segments(self) -> Vec<TranscriptionSegment> {
        let lang = self.language.clone();
        self.segments
            .into_iter()
            .map(|seg| {
                let mut out = TranscriptionSegment::new(
                    seconds_to_ms(seg.start),
                    seconds_to_ms(seg.end),
                    seg.text.trim(),
                );
                out.language = lang.clone();
                out.words = seg
                    .words
                    .into_iter()
                    .map(|w| WordTimestamp {
                        start_ms: seconds_to_ms(w.start),
                        end_ms: seconds_to_ms(w.end),
                        text: w.word.trim().to_string(),
                        confidence: w.probability.map(|p| p as f32),
                    })
                    .collect();
                out
            })
            .collect()
    }
}

fn seconds_to_ms(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

/// Small `which` helper duplicated from media-kit::ffmpeg to avoid a hard
/// dep. Walk PATH, return first executable match.
fn which_binary(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_output_fixture() {
        // Real mlx_whisper JSON snapshot from a VN clip (trimmed).
        let body = r#"{
            "text": " Sự thấu hiểu.",
            "language": "vi",
            "segments": [{
                "id": 0,
                "start": 5.1,
                "end": 7.04,
                "text": " Sự thấu hiểu công giáo về tội đổi nguyên thủy bắt đầu.",
                "words": [
                    { "word": " Sự", "start": 5.1, "end": 5.42, "probability": 0.956 },
                    { "word": " thấu", "start": 5.42, "end": 5.76, "probability": 0.91 }
                ]
            }]
        }"#;
        let parsed: MlxWhisperOutput = serde_json::from_str(body).unwrap();
        let segments = parsed.into_segments();
        assert_eq!(segments.len(), 1);
        let s = &segments[0];
        assert_eq!(s.start_ms, 5100);
        assert_eq!(s.end_ms, 7040);
        assert_eq!(s.language.as_deref(), Some("vi"));
        assert_eq!(s.words.len(), 2);
        assert_eq!(s.words[0].text, "Sự");
        assert_eq!(s.words[0].start_ms, 5100);
        assert!(s.words[0].confidence.is_some());
    }

    #[test]
    fn handles_missing_words_field() {
        let body = r#"{
            "segments": [
                { "start": 0.0, "end": 1.5, "text": "hello" }
            ]
        }"#;
        let parsed: MlxWhisperOutput = serde_json::from_str(body).unwrap();
        let segments = parsed.into_segments();
        assert_eq!(segments.len(), 1);
        assert!(segments[0].words.is_empty());
        assert!(segments[0].language.is_none());
    }
}
