//! Output folder management.
//!
//! Convention: every source file gets a sibling `{stem}_output/` directory.
//! All generated artifacts (transcript, translation, summary, etc.) live
//! inside it. On source load the frontend calls `ensure_output_dir` to
//! create the folder (no-op if it exists) and `scan_output_status` to
//! check which outputs are already present.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use creator_core::TranscriptionSegment;
use tauri::command;

/// Known output filenames → status key mapping.
const OUTPUT_FILES: &[(&str, &str)] = &[
    ("transcript.srt", "transcript"),
    ("transcript.txt", "transcript"),
    ("clean.srt", "clean"),
    ("summary.md", "summary"),
    ("chapters.json", "chapters"),
    ("youtube-pack.json", "youtube-pack"),
    ("viral-clips.json", "viral-clips"),
    ("blog.md", "blog"),
];

/// Prefix-matched outputs (translations can be any language).
const TRANSLATE_PREFIX: &str = "translate.";

/// Derive the `{stem}_output` directory path for a given source file.
fn output_dir_for(source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("source");
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}_output"))
}

/// Create the output directory if it doesn't exist. Returns the path.
#[command]
pub async fn ensure_output_dir(source_path: String) -> Result<String, String> {
    let source = PathBuf::from(&source_path);
    let dir = output_dir_for(&source);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("create output dir {}: {e}", dir.display()))?;
    }
    Ok(dir.to_string_lossy().into_owned())
}

/// Scan the output directory and return a map of `{ key: true }` for each
/// output type that already has a file on disk.
#[command]
pub async fn scan_output_status(source_path: String) -> Result<HashMap<String, bool>, String> {
    let source = PathBuf::from(&source_path);
    let dir = output_dir_for(&source);
    let mut status: HashMap<String, bool> = HashMap::new();

    if !dir.exists() {
        return Ok(status);
    }

    // Check known fixed filenames.
    for &(filename, key) in OUTPUT_FILES {
        if dir.join(filename).exists() {
            status.insert(key.to_string(), true);
        }
    }

    // Check for any translate.*.srt or translate.*.txt files.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(TRANSLATE_PREFIX) {
                status.insert("translate".to_string(), true);
            }
        }
    }

    Ok(status)
}

/// List all files in the output directory. Returns a vec of `{ name, size }`.
#[derive(Debug, serde::Serialize)]
pub struct OutputFile {
    pub name: String,
    pub size: u64,
}

#[command]
pub async fn list_output_files(source_path: String) -> Result<Vec<OutputFile>, String> {
    let source = PathBuf::from(&source_path);
    let dir = output_dir_for(&source);
    let mut files = Vec::new();

    if !dir.exists() {
        return Ok(files);
    }

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    files.push(OutputFile {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        size: meta.len(),
                    });
                }
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

/// Load and parse a transcript.srt from the output folder.
/// Returns segments if the file exists, None otherwise.
#[command]
pub async fn load_transcript_from_output(
    source_path: String,
) -> Result<Option<Vec<TranscriptionSegment>>, String> {
    let source = PathBuf::from(&source_path);
    let srt_path = output_dir_for(&source).join("transcript.srt");
    if !srt_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&srt_path)
        .map_err(|e| format!("read transcript.srt: {e}"))?;
    Ok(Some(parse_srt(&content)))
}

/// Minimal SRT parser → Vec<TranscriptionSegment>.
fn parse_srt(content: &str) -> Vec<TranscriptionSegment> {
    let mut segments = Vec::new();
    let blocks: Vec<&str> = content.split("\n\n").collect();
    for block in blocks {
        let lines: Vec<&str> = block.trim().lines().collect();
        if lines.len() < 3 {
            continue;
        }
        // Line 0 = index, Line 1 = timestamps, Line 2+ = text
        let time_line = lines[1];
        let (start_ms, end_ms) = match parse_srt_timecodes(time_line) {
            Some(t) => t,
            None => continue,
        };
        let text: String = lines[2..].join(" ");
        segments.push(TranscriptionSegment::new(start_ms, end_ms, text.trim()));
    }
    segments
}

/// Parse "HH:MM:SS,mmm --> HH:MM:SS,mmm" into (start_ms, end_ms).
fn parse_srt_timecodes(line: &str) -> Option<(i64, i64)> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }
    let start = parse_srt_time(parts[0].trim())?;
    let end = parse_srt_time(parts[1].trim())?;
    Some((start, end))
}

fn parse_srt_time(s: &str) -> Option<i64> {
    // "HH:MM:SS,mmm" or "HH:MM:SS.mmm"
    let s = s.replace(',', ".");
    let colon_parts: Vec<&str> = s.split(':').collect();
    if colon_parts.len() != 3 {
        return None;
    }
    let h: i64 = colon_parts[0].parse().ok()?;
    let m: i64 = colon_parts[1].parse().ok()?;
    let sec_parts: Vec<&str> = colon_parts[2].split('.').collect();
    let sec: i64 = sec_parts[0].parse().ok()?;
    let ms: i64 = if sec_parts.len() > 1 {
        let frac = sec_parts[1];
        let padded = format!("{:0<3}", frac);
        padded[..3].parse().unwrap_or(0)
    } else {
        0
    };
    Some((h * 3600 + m * 60 + sec) * 1000 + ms)
}
