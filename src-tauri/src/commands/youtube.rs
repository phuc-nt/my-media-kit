//! YouTube download command via yt-dlp.
//!
//! Downloads a YouTube URL to the app cache directory, keyed by video ID,
//! so re-opening the same video skips the download. Progress is emitted as
//! `yt_dlp_progress` Tauri events so the frontend can show a bar.
//!
//! Cache location: `<app_cache_dir>/youtube/<video_id>.mp4`
//! Dependency: `yt-dlp` must be on PATH.

use std::path::PathBuf;
use std::process::Stdio;

use serde_json::json;
use tauri::{command, AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

const YT_DLP: &str = "yt-dlp";

// ── Cache dir ────────────────────────────────────────────────────────────────

fn youtube_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("cannot resolve app cache dir: {e}"))?;
    let dir = base.join("youtube");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create cache dir: {e}"))?;
    Ok(dir)
}

// ── Video ID ─────────────────────────────────────────────────────────────────

/// Extract the 11-character YouTube video ID from any supported URL format.
/// Falls back to asking yt-dlp when the URL pattern is not recognised.
async fn video_id(url: &str) -> Result<String, String> {
    // Fast path: parse well-known URL patterns without spawning a process.
    if let Some(id) = extract_id_from_url(url) {
        return Ok(id);
    }
    // Slow path: let yt-dlp resolve it (handles short-links, playlist URLs, …)
    let out = tokio::process::Command::new(YT_DLP)
        .args(["--get-id", "--no-playlist", url])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("yt-dlp --get-id failed: {stderr}"));
    }
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() {
        return Err("yt-dlp returned empty video ID".into());
    }
    Ok(id)
}

/// Parse video ID from common YouTube URL formats without network access.
fn extract_id_from_url(url: &str) -> Option<String> {
    // youtu.be/{id}
    if let Some(rest) = url.strip_prefix("https://youtu.be/")
        .or_else(|| url.strip_prefix("http://youtu.be/"))
    {
        let id = rest.split(['?', '&', '#']).next()?.to_string();
        if id.len() == 11 { return Some(id); }
    }
    // youtube.com/watch?v={id}
    if url.contains("youtube.com/watch") {
        for part in url.split('?').nth(1).unwrap_or("").split('&') {
            if let Some(v) = part.strip_prefix("v=") {
                let id = v.split(['&', '#']).next()?.to_string();
                if id.len() == 11 { return Some(id); }
            }
        }
    }
    None
}

// ── Progress parsing ─────────────────────────────────────────────────────────

/// Parse a yt-dlp `[download]` progress line and return percent (0–100).
/// Example: `[download]  42.3% of   44.68MiB at    2.50MiB/s ETA 00:18`
fn parse_percent(line: &str) -> Option<f32> {
    let trimmed = line.trim_start_matches("[download]").trim();
    let pct_str = trimmed.split('%').next()?.trim();
    pct_str.parse::<f32>().ok()
}

// ── Main command ─────────────────────────────────────────────────────────────

/// Download a YouTube URL and return the local cached file path.
///
/// If the video was already downloaded (same video ID), returns the cached
/// path immediately without re-downloading. Progress events are emitted on
/// `yt_dlp_progress` with payload `{ percent, cached, label }`.
#[command]
pub async fn yt_dlp_download(url: String, app: AppHandle) -> Result<String, String> {
    let emit = |percent: f32, cached: bool, label: &str| {
        let _ = app.emit(
            "yt_dlp_progress",
            json!({ "percent": percent, "cached": cached, "label": label }),
        );
    };

    emit(0.0, false, "resolving video ID…");

    let id = video_id(&url).await?;
    let cache_dir = youtube_cache_dir(&app)?;
    let dest = cache_dir.join(format!("{id}.mp4"));

    // Return cached file immediately.
    if dest.exists() {
        emit(100.0, true, "loaded from cache");
        return Ok(dest.to_string_lossy().into_owned());
    }

    emit(0.0, false, "starting download…");

    // yt-dlp: prefer a single-file mp4 to avoid post-merge ffmpeg step.
    // `best[ext=mp4]` picks the best pre-muxed mp4 stream (audio+video in
    // one file). Falls back to `best` if no mp4 is available.
    let mut child = tokio::process::Command::new(YT_DLP)
        .args([
            "-f", "best[ext=mp4]/best",
            "--no-playlist",
            "--newline",        // one progress line per print
            "--progress",
            "-o", dest.to_str().unwrap(),
            &url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())   // suppress warnings (n-challenge noise)
        .spawn()
        .map_err(|e| format!("failed to start yt-dlp: {e}"))?;

    // Stream stdout and emit progress events.
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(pct) = parse_percent(&line) {
                let label = format!("downloading… {pct:.1}%");
                emit(pct, false, &label);
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("yt-dlp wait error: {e}"))?;

    if !status.success() {
        // Clean up partial file if present.
        let _ = std::fs::remove_file(&dest);
        return Err(format!("yt-dlp exited with status {status}"));
    }

    if !dest.exists() {
        return Err(format!("yt-dlp succeeded but output not found at {}", dest.display()));
    }

    emit(100.0, false, "download complete");
    Ok(dest.to_string_lossy().into_owned())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_youtu_be_url() {
        assert_eq!(
            extract_id_from_url("https://youtu.be/5Uo5_-kq4j0?si=ssE45kZ8AOJTjTkp"),
            Some("5Uo5_-kq4j0".into())
        );
    }

    #[test]
    fn parses_watch_url() {
        assert_eq!(
            extract_id_from_url("https://www.youtube.com/watch?v=5Uo5_-kq4j0&t=10s"),
            Some("5Uo5_-kq4j0".into())
        );
    }

    #[test]
    fn parses_progress_line() {
        let line = "[download]  42.3% of   44.68MiB at    2.50MiB/s ETA 00:18";
        assert!((parse_percent(line).unwrap() - 42.3).abs() < 0.01);
    }

    #[test]
    fn ignores_non_progress_line() {
        assert!(parse_percent("[youtube] Extracting URL: https://youtu.be/xxx").is_none());
    }
}
