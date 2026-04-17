//! YouTube URL pipeline E2E test.
//!
//! Verifies the core YouTube URL features:
//!   1. URL → video ID parsing (same logic as commands/youtube.rs)
//!   2. yt-dlp download with progress event emission
//!   3. Cache hit: second existence-check is instant (< 10 ms)
//!   4. Downloaded file is probe-able (media metadata)
//!   5. Transcription of a 30s crop via OpenAI Whisper
//!
//! Requirements:
//!   - `yt-dlp` on PATH
//!   - `ffmpeg` on PATH
//!   - `OPENAI_API_KEY` env var (step 5 is skipped when absent)
//!
//! Test video: https://youtu.be/5Uo5_-kq4j0 (~10 min, English)

use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};

const YT_URL: &str = "https://youtu.be/5Uo5_-kq4j0?si=ssE45kZ8AOJTjTkp";
const VIDEO_ID: &str = "5Uo5_-kq4j0";

// ── Helpers (mirror commands/youtube.rs private fns) ─────────────────────────

/// Parse 11-char video ID from youtu.be / youtube.com/watch URLs.
fn extract_video_id(url: &str) -> Option<String> {
    if let Some(rest) = url
        .strip_prefix("https://youtu.be/")
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

/// Parse percent from yt-dlp `[download] XX.X% of …` progress lines.
fn parse_percent(line: &str) -> Option<f32> {
    let trimmed = line.trim_start_matches("[download]").trim();
    let pct_str = trimmed.split('%').next()?.trim();
    pct_str.parse::<f32>().ok()
}

// ── Test ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn youtube_url_pipeline_e2e() {
    // ── 1. URL → video ID ─────────────────────────────────────────────────
    let id = extract_video_id(YT_URL).expect("video ID parse failed");
    assert_eq!(id, VIDEO_ID, "unexpected video ID");
    println!("[1] video ID: {id}  ✓");

    // ── 2. Download via yt-dlp ────────────────────────────────────────────
    let cache_dir = std::env::temp_dir().join("my_media_kit_yt_test");
    std::fs::create_dir_all(&cache_dir).expect("create temp cache dir");
    let dest = cache_dir.join(format!("{id}.mp4"));

    let t_download = Instant::now();
    let mut progress_events = 0usize;

    if !dest.exists() {
        println!("[2] downloading {} → {}", YT_URL, dest.display());

        let mut child = tokio::process::Command::new("yt-dlp")
            .args([
                "-f", "best[ext=mp4]/best",
                "--no-playlist",
                "--newline",
                "--progress",
                "-o", dest.to_str().unwrap(),
                YT_URL,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("yt-dlp not found — install with: pip install yt-dlp");

        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(pct) = parse_percent(&line) {
                    progress_events += 1;
                    // Print first 3 and final event to avoid log noise
                    if progress_events <= 3 || pct >= 99.9 {
                        println!("[2]   progress: {pct:.1}%");
                    }
                }
            }
        }

        let status = child.wait().await.expect("yt-dlp wait");
        assert!(status.success(), "yt-dlp exited with {status}");
        assert!(dest.exists(), "yt-dlp succeeded but output file missing");

        println!(
            "[2] download OK  ({} ms, {} progress events)  ✓",
            t_download.elapsed().as_millis(),
            progress_events
        );
        assert!(
            progress_events > 0,
            "expected progress events from yt-dlp stdout"
        );
    } else {
        println!("[2] already cached — skipped download");
        progress_events = 999; // cached; don't assert on fresh-download progress
    }

    // ── 3. Cache hit: existence check is instant ──────────────────────────
    let t_cache = Instant::now();
    let cache_hit = dest.exists();
    let cache_ms = t_cache.elapsed().as_millis();
    assert!(cache_hit, "file should exist after download");
    assert!(cache_ms < 10, "cache check took {cache_ms} ms — expected < 10 ms");
    println!("[3] cache check: {cache_ms} ms  ✓");

    // ── 4. Probe downloaded file ──────────────────────────────────────────
    let probe = media_kit::probe_media(&dest)
        .await
        .expect("media_probe on downloaded file failed");
    assert!(probe.duration_ms > 0, "probe returned zero duration");
    println!("[4] probe: {} ms  ✓", probe.duration_ms);

    // ── 5. Transcribe first 30s with OpenAI Whisper ───────────────────────
    let Some(api_key) = std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.is_empty()) else {
        eprintln!("[5] skipped — OPENAI_API_KEY not set");
        return;
    };

    let crop = cache_dir.join(format!("{id}_30s.wav"));
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-i", dest.to_str().unwrap(),
            "-t", "30",
            "-vn", "-ar", "16000", "-ac", "1",
            crop.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .expect("ffmpeg not found");
    assert!(status.success(), "ffmpeg crop failed: {status}");

    let t_asr = Instant::now();
    let transcriber = transcription_kit::OpenAiWhisperTranscriber { api_key };
    let segments = transcriber
        .transcribe(&crop, None, None)
        .await
        .expect("OpenAI Whisper transcription failed");

    assert!(
        !segments.is_empty(),
        "expected transcript segments from 30s audio"
    );
    println!(
        "[5] ASR: {} segments in {} ms  ✓",
        segments.len(),
        t_asr.elapsed().as_millis()
    );
    for s in segments.iter().take(3) {
        println!("    [{} – {}] {}", s.start_ms, s.end_ms, s.text.trim());
    }

    // Verify segment timestamps are within the 30s window
    for s in &segments {
        assert!(s.start_ms >= 0, "negative start_ms: {}", s.start_ms);
        assert!(s.end_ms <= 35_000, "end_ms exceeds crop: {}", s.end_ms);
    }
    println!("[5] segment timestamps valid  ✓");

    let _ = progress_events; // suppress unused warning when cached
}
