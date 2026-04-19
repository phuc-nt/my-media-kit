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
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use creator_core::{AbortFlag, TranscriptionSegment, WordTimestamp};

use crate::transcriber::{Transcriber, TranscriptionOptions};

/// Callback invoked each time the mlx_whisper verbose output reports a new
/// segment. Argument is the segment's **end** timestamp in milliseconds.
/// Callers usually compare it against a known total duration to compute a %.
pub type ProgressCallback = Arc<dyn Fn(i64) + Send + Sync>;

/// Default mlx-whisper model — pre-downloaded on the primary dev machine.
/// Override via env var `MY_MEDIA_KIT_MLX_WHISPER_MODEL` or builder arg.
pub const DEFAULT_MODEL: &str = "mlx-community/whisper-large-v3-turbo";

pub struct MlxWhisperTranscriber {
    model: String,
    binary: PathBuf,
}

impl MlxWhisperTranscriber {
    pub fn new() -> Self {
        let model = std::env::var("MY_MEDIA_KIT_MLX_WHISPER_MODEL")
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
        self.transcribe_file_inner(audio_path, options, None).await
    }

    /// Same as `transcribe_file` but forwards verbose-output timestamps to
    /// `on_progress` as the subprocess reports each segment. Suitable for a
    /// Tauri command that wants to emit `% complete` events to the frontend.
    pub async fn transcribe_file_with_progress(
        &self,
        audio_path: &Path,
        options: &TranscriptionOptions,
        on_progress: ProgressCallback,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        self.transcribe_file_inner(audio_path, options, Some(on_progress))
            .await
    }

    async fn transcribe_file_inner(
        &self,
        audio_path: &Path,
        options: &TranscriptionOptions,
        on_progress: Option<ProgressCallback>,
    ) -> Result<Vec<TranscriptionSegment>, String> {
        let tmp_dir = std::env::temp_dir().join(format!(
            "my_media_kit_mlx_whisper_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| format!("create tmp dir: {e}"))?;

        let stem = "transcript";
        let mut cmd = Command::new(&self.binary);
        // Force Python to flush stdout/stderr immediately so progress lines
        // arrive in real-time instead of being buffered until process exit.
        cmd.env("PYTHONUNBUFFERED", "1");
        // mlx_whisper internally calls `subprocess.run('ffmpeg', ...)` to
        // decode audio. macOS GUI apps inherit a minimal PATH that doesn't
        // include `/opt/homebrew/bin` or our bundled binaries dir, so the
        // child can't find ffmpeg → cryptic "FileNotFoundError: ffmpeg".
        // Prepend the bundled ffmpeg dir + the standard Homebrew dirs so
        // the child process inherits a usable PATH.
        cmd.env("PATH", augmented_path());
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
                // Enable verbose output so mlx_whisper prints
                // `[MM:SS.sss --> MM:SS.sss] text` lines per segment.
                "--verbose",
                "True",
                // Disable context carry-over to avoid whisper's classic
                // runaway-loop failure mode.
                "--condition-on-previous-text",
                "False",
                // Drop segments whose audio is mostly silence but whisper
                // still emits text for — the usual loop trigger.
                "--hallucination-silence-threshold",
                "2.0",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(lang) = &options.language {
            cmd.arg("--language").arg(lang);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn mlx_whisper: {e}"))?;

        // Drain stdout line-by-line so we can parse `[MM:SS.sss --> ...]`
        // progress markers while whisper is still running. stderr is drained
        // in parallel so long runs don't block on a full pipe.
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "mlx_whisper stdout not piped".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "mlx_whisper stderr not piped".to_string())?;

        // mlx_whisper outputs verbose segment lines (`[MM:SS --> MM:SS] text`)
        // to stderr, not stdout. Parse both streams for progress so we catch it
        // regardless of which stream the CLI uses.
        let on_progress_for_stdout = on_progress.clone();
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(cb) = &on_progress_for_stdout {
                    if let Some(end_ms) = parse_progress_line(&line) {
                        cb(end_ms);
                    }
                }
            }
        });
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut collected = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                // Parse progress from stderr too — mlx_whisper outputs here.
                if let Some(cb) = &on_progress {
                    if let Some(end_ms) = parse_progress_line(&line) {
                        cb(end_ms);
                    }
                }
                collected.push_str(&line);
                collected.push('\n');
            }
            collected
        });

        let status = child
            .wait()
            .await
            .map_err(|e| format!("wait mlx_whisper: {e}"))?;
        let _ = stdout_task.await;
        let stderr_body = stderr_task.await.unwrap_or_default();

        if !status.success() {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(format!(
                "mlx_whisper exited with status {:?}: {}",
                status.code(),
                stderr_body
            ));
        }

        // Find the output JSON. mlx_whisper normally writes `{stem}.json` but
        // some versions append the input basename or use a different layout —
        // probe for any *.json in the tmp dir as a robust fallback.
        let json_path = tmp_dir.join(format!("{stem}.json"));
        let body = match std::fs::read_to_string(&json_path) {
            Ok(b) => b,
            Err(_) => {
                let alt = std::fs::read_dir(&tmp_dir)
                    .ok()
                    .and_then(|d| {
                        d.filter_map(|e| e.ok())
                            .map(|e| e.path())
                            .find(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
                    });
                match alt {
                    Some(p) => std::fs::read_to_string(&p)
                        .map_err(|e| format!("read {}: {e}", p.display()))?,
                    None => {
                        let listing = std::fs::read_dir(&tmp_dir)
                            .map(|d| {
                                d.filter_map(|e| e.ok())
                                    .map(|e| e.file_name().to_string_lossy().into_owned())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|_| "<unreadable>".into());
                        let _ = std::fs::remove_dir_all(&tmp_dir);
                        return Err(format!(
                            "mlx_whisper exited cleanly but produced no JSON output. \
                             Files in tmp dir: [{listing}]. stderr: {}",
                            stderr_body.trim()
                        ));
                    }
                }
            }
        };
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let sanitized = sanitize_python_json(&body);
        let parsed: MlxWhisperOutput = serde_json::from_str(&sanitized)
            .map_err(|e| format!("parse json: {e}"))?;

        Ok(parsed.into_segments())
    }
}

/// Parse an mlx_whisper verbose-output line of the form
/// `[MM:SS.sss --> MM:SS.sss]  text...` and return the **end** timestamp in
/// milliseconds. Returns `None` for any line that does not match. We only
/// care about the end timestamp (monotonic, tracks whisper's cursor).
fn parse_progress_line(line: &str) -> Option<i64> {
    let rest = line.trim_start();
    if !rest.starts_with('[') {
        return None;
    }
    let inner_start = 1;
    let close = rest.find(']')?;
    let inner = &rest[inner_start..close];
    let arrow = inner.find("-->")?;
    let end_str = inner[arrow + 3..].trim();
    parse_whisper_timestamp(end_str)
}

/// Parse `MM:SS.sss` (or `HH:MM:SS.sss`) into milliseconds.
fn parse_whisper_timestamp(s: &str) -> Option<i64> {
    let (time_part, frac_part) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s, "0"),
    };
    let parts: Vec<&str> = time_part.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [m, sec] => (0i64, m.parse::<i64>().ok()?, sec.parse::<i64>().ok()?),
        [h, m, sec] => (
            h.parse::<i64>().ok()?,
            m.parse::<i64>().ok()?,
            sec.parse::<i64>().ok()?,
        ),
        _ => return None,
    };
    let frac_ms: i64 = {
        let padded = format!("{:0<3}", frac_part);
        padded.get(..3)?.parse::<i64>().ok()?
    };
    Some(((h * 3600 + m * 60 + sec) * 1000) + frac_ms)
}

/// Python's `json.dumps` emits bare `NaN`, `Infinity`, `-Infinity` tokens for
/// non-finite floats (mlx_whisper does this for `avg_logprob` on empty /
/// highly-uncertain segments). These are not valid JSON, so `serde_json`
/// refuses to parse them. We only consume a handful of numeric fields and
/// don't care about the non-finite values, so replace them with `null` before
/// parsing. Only matches tokens in value position (`: <tok>` or `, <tok>` /
/// `[<tok>`) so literal text inside string values is left alone.
///
/// Operates on raw bytes so multi-byte UTF-8 sequences in string values
/// (Vietnamese diacritics, CJK, etc.) pass through untouched. A previous
/// version used `String::push(b as char)` which corrupted every non-ASCII
/// byte into a Latin-1 code point, producing classic mojibake like
/// `cẠi thá»±` in the Transcribe view.
fn sanitize_python_json(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            out.push(b);
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            out.push(b'"');
            i += 1;
            continue;
        }
        // Only replace tokens whose left-neighbour (after skipping ASCII
        // whitespace) signals a value position: `:`, `,`, or `[`.
        let preceded_by_value_marker = {
            let mut j = out.len();
            while j > 0 && matches!(out[j - 1], b' ' | b'\t' | b'\n' | b'\r') {
                j -= 1;
            }
            j > 0 && matches!(out[j - 1], b':' | b',' | b'[')
        };
        if preceded_by_value_marker {
            if let Some(len) = match_nonfinite(&bytes[i..]) {
                out.extend_from_slice(b"null");
                i += len;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    // Input was a valid `&str`; we only substitute `null` for `NaN`/
    // `Infinity`/`-Infinity` which are pure ASCII. Therefore the output is
    // guaranteed to be valid UTF-8 — `from_utf8_unchecked` would be safe,
    // but `from_utf8` keeps the assertion cheap and explicit.
    String::from_utf8(out).expect("sanitizer preserves UTF-8")
}

/// Returns Some(len) if `rest` starts with a bare `NaN`, `Infinity`, or
/// `-Infinity` token whose next byte is not an identifier continuation.
fn match_nonfinite(rest: &[u8]) -> Option<usize> {
    const TOKENS: &[&[u8]] = &[b"NaN", b"Infinity", b"-Infinity"];
    for tok in TOKENS {
        if rest.starts_with(tok) {
            let after = rest.get(tok.len()).copied();
            let is_boundary = match after {
                None => true,
                Some(c) => !(c.is_ascii_alphanumeric() || c == b'_'),
            };
            if is_boundary {
                return Some(tok.len());
            }
        }
    }
    None
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

/// Build a PATH value that includes the bundled ffmpeg location plus the
/// standard Homebrew / pipx dirs. mlx_whisper spawns `ffmpeg` directly via
/// Python's subprocess and would otherwise inherit the GUI app's stripped
/// PATH (no `/opt/homebrew/bin`, no `~/.local/bin`).
fn augmented_path() -> String {
    let mut entries: Vec<String> = Vec::new();

    // Bundled ffmpeg is exposed via FFMPEG env var by the app's setup hook.
    if let Ok(ff) = std::env::var("FFMPEG") {
        if let Some(dir) = std::path::Path::new(&ff).parent() {
            entries.push(dir.to_string_lossy().into_owned());
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    for dir in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        &format!("{home}/.local/bin"),
    ] {
        entries.push(dir.to_string());
    }

    if let Ok(existing) = std::env::var("PATH") {
        entries.push(existing);
    }
    entries.join(":")
}

/// Small `which` helper duplicated from media-kit::ffmpeg to avoid a hard
/// dep. Walks PATH then falls back to common pip / pipx install dirs that
/// macOS GUI apps don't see (Finder-launched apps have a minimal PATH that
/// excludes `~/.local/bin` and `/opt/homebrew/bin`).
fn which_binary(name: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
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
    }
    let home = std::env::var("HOME").ok()?;
    let extras = [
        format!("{home}/.local/bin/{name}"),
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
    ];
    extras.into_iter().map(PathBuf::from).find(|p| p.is_file())
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
    fn sanitizer_rewrites_nan_and_infinity_in_value_position() {
        let input = r#"{"a": NaN, "b": -Infinity, "c": Infinity, "d": 1.5}"#;
        let out = sanitize_python_json(input);
        assert_eq!(out, r#"{"a": null, "b": null, "c": null, "d": 1.5}"#);
    }

    #[test]
    fn sanitizer_preserves_multibyte_utf8_in_strings() {
        // Vietnamese "ạ" is `0xE1 0xBA 0xA1` in UTF-8 — a previous version
        // pushed bytes through `char` and turned this into `á º ¡` mojibake.
        let input =
            r#"{"text": "6 cải thảo xanh, sự thật", "score": NaN}"#;
        let out = sanitize_python_json(input);
        assert!(out.contains("6 cải thảo xanh, sự thật"));
        assert!(out.contains("\"score\": null"));
        // Round-trip through serde to confirm we emitted valid UTF-8 JSON.
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["text"], "6 cải thảo xanh, sự thật");
    }

    #[test]
    fn sanitizer_leaves_string_content_untouched() {
        // "NaN" inside a JSON string must not be rewritten.
        let input = r#"{"text": "the NaN token", "score": NaN}"#;
        let out = sanitize_python_json(input);
        assert_eq!(out, r#"{"text": "the NaN token", "score": null}"#);
    }

    #[test]
    fn sanitizer_parses_real_mlx_whisper_shape() {
        // Shape mirrors what mlx_whisper emits for a silent / low-confidence
        // segment: `avg_logprob: NaN`, which the default serde_json parser
        // rejects.
        let body = r#"{
            "language": "en",
            "segments": [{
                "id": 0,
                "start": 0.0,
                "end": 1.2,
                "text": " hi",
                "avg_logprob": NaN,
                "compression_ratio": NaN,
                "words": []
            }]
        }"#;
        let sanitized = sanitize_python_json(body);
        let parsed: MlxWhisperOutput = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(parsed.segments.len(), 1);
        assert_eq!(parsed.segments[0].text, " hi");
    }

    #[test]
    fn progress_parser_reads_mm_ss_lines() {
        assert_eq!(
            parse_progress_line("[00:00.000 --> 00:03.300]  hello"),
            Some(3300)
        );
        assert_eq!(
            parse_progress_line("[12:46.900 --> 12:59.340] !"),
            Some(12 * 60_000 + 59_340)
        );
    }

    #[test]
    fn progress_parser_reads_hh_mm_ss_lines() {
        assert_eq!(
            parse_progress_line("[01:02:03.456 --> 01:02:05.000]  long"),
            Some(((1 * 3600 + 2 * 60 + 5) * 1000) as i64)
        );
    }

    #[test]
    fn progress_parser_ignores_non_segment_lines() {
        assert_eq!(parse_progress_line("Detected language: English"), None);
        assert_eq!(parse_progress_line(""), None);
        assert_eq!(parse_progress_line("[no arrow here]"), None);
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
