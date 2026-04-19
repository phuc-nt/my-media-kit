//! YouTube download command via yt-dlp.
//!
//! Downloads a YouTube URL to `~/Downloads/MyMediaKit/{title} [{id}].mp4` so
//! the user can find files in a familiar place. The video ID is included in
//! the filename so we can detect prior downloads (cache by ID).
//!
//! Binary resolution: `YT_DLP_BIN` env var (set at app startup to the
//! bundled sidecar) → falls back to `yt-dlp` on PATH for local dev.
//!
//! Progress is emitted as `yt_dlp_progress` Tauri events.

use std::path::PathBuf;
use std::process::Stdio;

use serde_json::json;
use tauri::{command, AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

const SUBDIR: &str = "MyMediaKit";

fn ytdlp_binary() -> String {
    std::env::var("YT_DLP_BIN").unwrap_or_else(|_| "yt-dlp".into())
}

// ── Output dir: ~/Downloads/MyMediaKit (works on macOS, Windows, Linux) ─────

fn output_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("cannot resolve Downloads dir: {e}"))?;
    let dir = downloads.join(SUBDIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {SUBDIR} dir: {e}"))?;
    Ok(dir)
}

// ── Video ID ─────────────────────────────────────────────────────────────────

/// Extract the 11-character YouTube video ID from any supported URL format.
/// Falls back to asking yt-dlp when the URL pattern is not recognised.
async fn video_id(url: &str) -> Result<String, String> {
    if let Some(id) = extract_id_from_url(url) {
        return Ok(id);
    }
    let out = tokio::process::Command::new(ytdlp_binary())
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

fn extract_id_from_url(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("https://youtu.be/")
        .or_else(|| url.strip_prefix("http://youtu.be/"))
    {
        let id = rest.split(['?', '&', '#']).next()?.to_string();
        if id.len() == 11 { return Some(id); }
    }
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

// ── Cache lookup: find existing `* [id].mp4` ────────────────────────────────

fn find_cached(dir: &std::path::Path, id: &str) -> Option<PathBuf> {
    let suffix = format!("[{id}].mp4");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(&suffix))
                .unwrap_or(false)
        })
}

// ── Progress parsing ─────────────────────────────────────────────────────────

fn parse_percent(line: &str) -> Option<f32> {
    let trimmed = line.trim_start_matches("[download]").trim();
    let pct_str = trimmed.split('%').next()?.trim();
    pct_str.parse::<f32>().ok()
}

// ── Main command ─────────────────────────────────────────────────────────────

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
    let dir = output_dir(&app)?;

    if let Some(existing) = find_cached(&dir, &id) {
        emit(100.0, true, "loaded from cache");
        return Ok(existing.to_string_lossy().into_owned());
    }

    emit(0.0, false, "starting download…");

    // Output template: `{title} [{id}].{ext}` — `restrict-filenames` strips
    // characters illegal on Windows/macOS so the file is portable.
    let template = dir.join("%(title).200B [%(id)s].%(ext)s");

    let mut child = tokio::process::Command::new(ytdlp_binary())
        .args([
            "-f", "best[ext=mp4]/best",
            "--no-playlist",
            "--restrict-filenames",
            "--newline",
            "--progress",
            "--print", "after_move:filepath",
            "-o", template.to_str().unwrap(),
            &url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start yt-dlp: {e}"))?;

    let mut written_path: Option<PathBuf> = None;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(pct) = parse_percent(&line) {
                emit(pct, false, &format!("downloading… {pct:.1}%"));
                continue;
            }
            // `--print after_move:filepath` outputs the absolute path of the
            // final file (after any post-process moves) — capture it.
            let trimmed = line.trim();
            if !trimmed.is_empty() && std::path::Path::new(trimmed).is_absolute() {
                written_path = Some(PathBuf::from(trimmed));
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("yt-dlp wait error: {e}"))?;

    if !status.success() {
        return Err(format!("yt-dlp exited with status {status}"));
    }

    let dest = written_path
        .or_else(|| find_cached(&dir, &id))
        .ok_or_else(|| "yt-dlp succeeded but output path not captured".to_string())?;

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
