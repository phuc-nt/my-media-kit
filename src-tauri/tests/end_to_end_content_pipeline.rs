//! Full content-pipeline integration test on real videos in `test-input/`.
//!
//! Reproduces, *without the UI*, the exact flow the user would drive
//! manually: probe → mlx_whisper transcribe → content_translate → save SRT.
//! Asserts the bugs we burned through manually stay dead:
//!
//!   - NaN sanitizer survives long videos with silent stretches
//!     (`avg_logprob: NaN` in the Python JSON output).
//!   - UTF-8 multi-byte characters round-trip cleanly (no `cẠi` mojibake).
//!   - Whisper hallucination loop stays under control
//!     ("The Harvard men never ask that question" × 50).
//!   - Translate length-mismatch never crashes the run; we always end up
//!     with `originals.len() == translations.len()`.
//!
//! Requires:
//!   - Apple Silicon (mlx_whisper + mlx_lm both gated)
//!   - `mlx_lm.server` running on 127.0.0.1:8080 with a translate-capable
//!     model loaded (currently `gemma-4-E4B-it-4bit`)
//!   - Source clips in `test-input/` at the repo root (full-length videos,
//!     not the 30-second `/tmp/` trims used by other smoke tests)
//!
//! Run:
//!   cargo test --test end_to_end_content_pipeline -- --nocapture --test-threads=1
//!
//! Override the source folder with `MY_MEDIA_KIT_TEST_INPUT=/some/dir`.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::time::Instant;

use ai_kit::{MlxLmProvider, Provider};
use content_kit::translate::{
    ProviderTranslateRunner, TranslateOptions, TranslateRunner,
};
use creator_core::TranscriptionSegment;
use media_kit::probe_media;
use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

const HARVARD_LOOP_MAX: usize = 5;
const VN_CLIP_PREFIX: &str = "Su-that";
const EN_CLIP_PREFIX: &str = "What-Makes";
const JP_CLIP_PREFIX: &str = "Hope-invites";

fn input_dir() -> PathBuf {
    if let Ok(p) = std::env::var("MY_MEDIA_KIT_TEST_INPUT") {
        return PathBuf::from(p);
    }
    // Default: walk up from CARGO_MANIFEST_DIR (= src-tauri/) to repo root,
    // then `test-input/`.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(|p| p.join("test-input"))
        .unwrap_or_else(|| PathBuf::from("test-input"))
}

fn pick_clip(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let stem = path.file_stem()?.to_string_lossy().to_string();
        if stem.starts_with(prefix) && path.extension().is_some_and(|e| e == "mp4") {
            return Some(path);
        }
    }
    None
}

async fn mlx_server_or_skip() -> Option<MlxLmProvider> {
    let p = MlxLmProvider::default_local();
    if p.is_available().await {
        Some(p)
    } else {
        eprintln!("skipped: mlx_lm.server not running on 127.0.0.1:8080");
        None
    }
}

fn assert_no_mojibake(segments: &[TranscriptionSegment], lang_hint: &str) {
    // Mojibake patterns we have actually seen in the wild from the Latin-1
    // double-encoding bug. Any of these appearing in transcript text
    // means our UTF-8 sanitiser regressed.
    const MOJIBAKE_NEEDLES: &[&str] = &[
        "Ã¡", "Ã©", "Ã­", "Ã³", "Ãº", // common acute accents
        "á»", "á»±", "Ạ", "Ạ", "Ạ", "Ạ", "Ạ", // ạ family
        "â€", // smart-quote double-encoding
    ];
    for (i, seg) in segments.iter().enumerate() {
        for needle in MOJIBAKE_NEEDLES {
            assert!(
                !seg.text.contains(needle),
                "[{lang_hint}] segment {i}: mojibake pattern {:?} in {:?}",
                needle,
                seg.text
            );
        }
    }
}

fn assert_no_runaway_loop(segments: &[TranscriptionSegment], lang_hint: &str) {
    // Whisper's classic failure: same line repeated dozens of times.
    use std::collections::HashMap;
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for seg in segments {
        let trimmed = seg.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        *counts.entry(trimmed).or_insert(0) += 1;
    }
    if let Some((line, n)) = counts.iter().max_by_key(|(_, n)| **n) {
        assert!(
            *n <= HARVARD_LOOP_MAX,
            "[{lang_hint}] runaway loop detected: line {:?} repeats {} times (max allowed {})",
            line,
            n,
            HARVARD_LOOP_MAX
        );
    }
}

fn segments_to_srt(segments: &[TranscriptionSegment]) -> String {
    let mut out = String::new();
    for (i, s) in segments.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            ms_to_srt(s.start_ms),
            ms_to_srt(s.end_ms),
            s.text.trim()
        ));
    }
    out
}

fn ms_to_srt(ms: i64) -> String {
    let total = ms.max(0) as u64;
    let h = total / 3_600_000;
    let m = (total % 3_600_000) / 60_000;
    let s = (total % 60_000) / 1000;
    let millis = total % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, millis)
}

async fn run_full_pipeline(
    label: &str,
    clip: PathBuf,
    provider: Option<&MlxLmProvider>,
    expect_skip: bool,
) {
    println!("\n==== {label}: {} ====", clip.display());
    let started = Instant::now();

    // ── 1. Probe ─────────────────────────────────────────────────────
    let probe = probe_media(&clip).await.expect("probe");
    println!("  duration: {} ms", probe.duration_ms);

    // ── 2. Transcribe ────────────────────────────────────────────────
    let t0 = Instant::now();
    let transcriber = MlxWhisperTranscriber::new();
    let segments = transcriber
        .transcribe_file(&clip, &TranscriptionOptions::default())
        .await
        .unwrap_or_else(|e| panic!("[{label}] whisper failed: {e}"));
    println!(
        "  transcribe: {} segments in {:.1}s",
        segments.len(),
        t0.elapsed().as_secs_f32()
    );

    assert!(!segments.is_empty(), "[{label}] empty transcript");
    assert_no_mojibake(&segments, label);
    assert_no_runaway_loop(&segments, label);

    let language = segments
        .iter()
        .find_map(|s| s.language.clone())
        .expect("language detected");
    println!("  language: {language}");

    // Save the .srt next to the source — gives the user something to eyeball.
    let srt_path = clip.with_extension("transcript.srt");
    std::fs::write(&srt_path, segments_to_srt(&segments)).expect("write srt");
    let srt_size = std::fs::metadata(&srt_path).expect("stat srt").len();
    println!("  saved → {} ({} bytes)", srt_path.display(), srt_size);
    assert!(srt_size > 100, "[{label}] srt suspiciously small");

    // ── 3. Translate (only if provider available) ───────────────────
    let Some(provider) = provider else {
        println!("  translate: skipped (mlx_lm.server not running)");
        println!("  TOTAL: {:.1}s", started.elapsed().as_secs_f32());
        return;
    };
    let runner = ProviderTranslateRunner { provider };
    let t1 = Instant::now();
    let result = runner
        .run(
            &segments,
            Some(&language),
            &TranslateOptions::default(),
            "ignored-by-mlx-server",
        )
        .await
        .unwrap_or_else(|e| panic!("[{label}] translate failed: {e}"));
    println!(
        "  translate: skipped={} in {:.1}s",
        result.skipped,
        t1.elapsed().as_secs_f32()
    );

    assert_eq!(result.skipped, expect_skip, "[{label}] skip-rule mismatch");
    assert_eq!(
        result.segments.len(),
        segments.len(),
        "[{label}] translated count must match originals (padding/truncate failed)"
    );

    if !expect_skip {
        assert_no_mojibake(&result.segments, label);
        // Save translated .srt with target language suffix.
        let translated_path =
            clip.with_extension(format!("{}.srt", result.target_language));
        std::fs::write(&translated_path, segments_to_srt(&result.segments))
            .expect("write translated srt");
        println!("  saved → {}", translated_path.display());
    }

    println!("  TOTAL: {:.1}s", started.elapsed().as_secs_f32());
}

#[tokio::test]
async fn vn_clip_skips_translate() {
    let dir = input_dir();
    let Some(clip) = pick_clip(&dir, VN_CLIP_PREFIX) else {
        eprintln!("skipped: VN clip not found in {}", dir.display());
        return;
    };
    let provider = mlx_server_or_skip().await;
    run_full_pipeline("VN", clip, provider.as_ref(), true).await;
}

#[tokio::test]
async fn en_clip_translates_to_vi() {
    let dir = input_dir();
    let Some(clip) = pick_clip(&dir, EN_CLIP_PREFIX) else {
        eprintln!("skipped: EN clip not found in {}", dir.display());
        return;
    };
    let provider = mlx_server_or_skip().await;
    run_full_pipeline("EN", clip, provider.as_ref(), false).await;
}

#[tokio::test]
async fn jp_clip_translates_to_vi() {
    let dir = input_dir();
    let Some(clip) = pick_clip(&dir, JP_CLIP_PREFIX) else {
        eprintln!("skipped: JP clip not found in {}", dir.display());
        return;
    };
    let provider = mlx_server_or_skip().await;
    run_full_pipeline("JP", clip, provider.as_ref(), false).await;
}
