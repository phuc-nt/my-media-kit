//! Real-media mlx-whisper smoke test.
//!
//! Spawns the real `mlx_whisper` CLI against a file pointed to by
//! `CREATOR_UTILS_TEST_MEDIA`. Skipped when the env var is unset OR when
//! the platform is not Apple Silicon (the `MlxWhisperTranscriber` type
//! doesn't exist there).
//!
//! Runs slowly on first invocation because mlx_whisper fetches model
//! weights from Hugging Face. After the cache is warm the 34 s VN clip
//! transcribes in a few seconds on an M-series chip.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::PathBuf;

use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

fn test_media_path() -> Option<PathBuf> {
    std::env::var("CREATOR_UTILS_TEST_MEDIA")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

#[tokio::test]
async fn mlx_whisper_transcribes_real_file() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };

    let transcriber = MlxWhisperTranscriber::new();
    let options = TranscriptionOptions::default();

    let segments = transcriber
        .transcribe_file(&path, &options)
        .await
        .expect("mlx_whisper must succeed");

    assert!(!segments.is_empty(), "expected at least one segment");
    println!("segments: {}", segments.len());

    for (i, seg) in segments.iter().enumerate().take(5) {
        println!(
            "  seg {i}: {}..{} ms [{}] lang={:?}",
            seg.start_ms,
            seg.end_ms,
            seg.text.chars().take(60).collect::<String>(),
            seg.language
        );
        if let Some(w) = seg.words.first() {
            println!(
                "    first word: {} @ {}..{} ms (conf {:?})",
                w.text, w.start_ms, w.end_ms, w.confidence
            );
        }
    }

    // Monotonic start times
    for pair in segments.windows(2) {
        assert!(
            pair[1].start_ms >= pair[0].start_ms,
            "segments must be monotonically non-decreasing"
        );
    }

    // Language should be populated on any nontrivial clip.
    assert!(
        segments.iter().any(|s| s.language.is_some()),
        "at least one segment should carry a language tag"
    );

    // Word timestamps should be present for most segments.
    let with_words = segments.iter().filter(|s| !s.words.is_empty()).count();
    assert!(
        with_words >= segments.len() / 2,
        "at least half of segments should have word timestamps"
    );
}
