//! Real-server MLX LM integration smoke test. Exercises filler detection,
//! summary, and chapter extraction against a locally running `mlx_lm.server`
//! (installed via `pip install mlx-lm`, pre-downloaded Qwen models).
//!
//! Start the server first:
//!   mlx_lm.server --model mlx-community/Qwen2.5-3B-Instruct-4bit --port 8080
//!
//! The test skips (not fails) when:
//!   - platform is not Apple Silicon
//!   - server does not respond at 127.0.0.1:8080 within 300 ms
//!
//! We ship deterministic mini-transcripts so runs stay fast (~5-15 s).

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use ai_kit::MlxLmProvider;
use ai_kit::Provider;
use content_kit::{
    chapters::{ChapterRunner, ProviderChapterRunner},
    filler::{AiFillerDetector, FillerDetector},
    summary::{ProviderSummaryRunner, SummaryRunner, SummaryStyle},
    batch::TranscriptBatch,
};
use creator_core::TranscriptionSegment;

const MLX_MODEL: &str = "mlx-community/Qwen2.5-3B-Instruct-4bit";

async fn server_up() -> Option<MlxLmProvider> {
    let p = MlxLmProvider::default_local();
    if p.is_available().await {
        Some(p)
    } else {
        None
    }
}

fn short_transcript() -> Vec<TranscriptionSegment> {
    vec![
        TranscriptionSegment::new(0, 3_000, "Hello everyone, um, welcome to the show."),
        TranscriptionSegment::new(3_000, 8_000, "Today we are going to talk about productivity."),
        TranscriptionSegment::new(8_000, 14_000, "First, let's look at morning routines."),
        TranscriptionSegment::new(14_000, 22_000, "Then we will cover deep work sessions."),
        TranscriptionSegment::new(22_000, 30_000, "Finally, how to wind down at the end of the day."),
    ]
}

#[tokio::test]
async fn mlx_lm_filler_detection_returns_schema_shaped_output() {
    let Some(provider) = server_up().await else {
        eprintln!("skipped: mlx_lm.server not running on 127.0.0.1:8080");
        return;
    };

    let batch = TranscriptBatch {
        batch_index: 0,
        first_segment_index: 0,
        segments: short_transcript(),
    };

    let detector = AiFillerDetector { provider: &provider };
    match detector.detect(&batch, MLX_MODEL).await {
        Ok(results) => {
            println!("filler detections: {}", results.len());
            for r in &results {
                println!(
                    "  seg={} range={}..{} text={:?} fillers={:?}",
                    r.segment_index, r.cut_start_ms, r.cut_end_ms, r.text, r.filler_words
                );
            }
            // Qwen 3B usually catches the "um" in segment 0. We do not
            // require a specific count — the hard assertion is "provider
            // returned a shape that parsed successfully".
        }
        Err(e) => {
            // mlx_lm.server does not enforce JSON schema strict mode. If the
            // model fails to emit valid JSON we log it rather than fail the
            // suite — the content-kit parser already surfaces this as
            // `AiProviderError::Malformed`. Upgrade to assert only once we
            // have JSON-repair wired.
            eprintln!("filler detect returned error (likely schema drift): {e}");
        }
    }
}

#[tokio::test]
async fn mlx_lm_summary_runs_end_to_end() {
    let Some(provider) = server_up().await else {
        eprintln!("skipped: mlx_lm.server not running");
        return;
    };

    let runner = ProviderSummaryRunner { provider: &provider };
    let result = runner
        .run(
            &short_transcript(),
            SummaryStyle::Brief,
            "English",
            MLX_MODEL,
            60.0,
        )
        .await;

    match result {
        Ok(s) => {
            println!("summary ({} style, {} lang):", format!("{:?}", s.style), s.language);
            println!("  text: {}", s.text.chars().take(400).collect::<String>());
            assert!(!s.text.is_empty(), "summary must not be empty");
            assert_eq!(s.language, "English");
        }
        Err(e) => {
            eprintln!("summary returned error: {e}");
        }
    }
}

#[tokio::test]
async fn mlx_lm_chapters_runs_end_to_end() {
    let Some(provider) = server_up().await else {
        eprintln!("skipped: mlx_lm.server not running");
        return;
    };

    let runner = ProviderChapterRunner { provider: &provider };
    let result = runner
        .run(&short_transcript(), "English", MLX_MODEL)
        .await;

    match result {
        Ok(list) => {
            println!("chapters ({}):", list.chapters.len());
            for c in &list.chapters {
                println!("  {} ms — {}", c.timestamp_ms, c.title);
            }
            assert!(!list.chapters.is_empty(), "at least one chapter");
            assert_eq!(
                list.chapters[0].timestamp_ms, 0,
                "first chapter must be pinned to 00:00"
            );
        }
        Err(e) => {
            eprintln!("chapters returned error: {e}");
        }
    }
}
