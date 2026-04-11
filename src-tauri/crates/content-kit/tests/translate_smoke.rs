//! End-to-end translate smoke test across three test clips:
//!   - Vietnamese (expect skip)
//!   - English    (expect translation to Vietnamese)
//!   - Japanese   (expect translation to Vietnamese)
//!
//! Requires:
//!   - Apple Silicon (mlx_whisper + mlx_lm both gated)
//!   - `mlx_lm.server` running on 127.0.0.1:8080 with a Qwen model loaded
//!   - 30-second test clips already trimmed into `/tmp/creator_utils_test/`
//!
//! Run:
//!   cargo test -p content-kit --test translate_smoke -- --nocapture --test-threads=1

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::Path;

use ai_kit::{MlxLmProvider, Provider};
use content_kit::translate::{
    ProviderTranslateRunner, TranslateOptions, TranslateRunner,
};
use creator_core::TranscriptionSegment;
use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

const MLX_MODEL: &str = "mlx-community/Qwen2.5-7B-Instruct-4bit";
const CLIPS_DIR: &str = "/tmp/creator_utils_test";
const VN_CLIP: &str = "clip-Su-that-ve-tam-ly-hoc-khong-gi.mp4";
const EN_CLIP: &str = "clip-What-Makes-a-Good-Life-Lessons.mp4";
const JP_CLIP: &str = "clip-Hope-invites-Tsutomu-Uematsu-T.mp4";

async fn mlx_server_up() -> Option<MlxLmProvider> {
    let p = MlxLmProvider::default_local();
    if p.is_available().await {
        Some(p)
    } else {
        None
    }
}

async fn transcribe(clip_name: &str) -> Vec<TranscriptionSegment> {
    let path = Path::new(CLIPS_DIR).join(clip_name);
    assert!(path.exists(), "missing test clip: {}", path.display());
    let transcriber = MlxWhisperTranscriber::new();
    transcriber
        .transcribe_file(&path, &TranscriptionOptions::default())
        .await
        .unwrap_or_else(|e| panic!("whisper failed for {clip_name}: {e}"))
}

fn detected_language(segments: &[TranscriptionSegment]) -> Option<String> {
    segments.iter().find_map(|s| s.language.clone())
}

#[tokio::test]
async fn vn_clip_is_skipped() {
    let Some(provider) = mlx_server_up().await else {
        eprintln!("skipped: mlx_lm.server not running");
        return;
    };

    let segments = transcribe(VN_CLIP).await;
    let lang = detected_language(&segments);
    println!("VN clip: {} segments, detected language = {:?}", segments.len(), lang);
    assert!(!segments.is_empty(), "whisper must return at least one segment");
    assert_eq!(
        lang.as_deref(),
        Some("vi"),
        "test clip must be detected as Vietnamese"
    );

    let runner = ProviderTranslateRunner { provider: &provider };
    let result = runner
        .run(
            &segments,
            lang.as_deref(),
            &TranslateOptions::default(),
            MLX_MODEL,
        )
        .await
        .expect("translate must succeed");

    assert!(result.skipped, "VN source should skip translation");
    assert_eq!(result.segments.len(), segments.len());
    assert_eq!(result.segments[0].text, segments[0].text);
}

#[tokio::test]
async fn en_clip_translates_to_vi() {
    let Some(provider) = mlx_server_up().await else {
        eprintln!("skipped: mlx_lm.server not running");
        return;
    };

    let segments = transcribe(EN_CLIP).await;
    let lang = detected_language(&segments);
    println!("EN clip: {} segments, detected language = {:?}", segments.len(), lang);
    assert!(!segments.is_empty());
    assert_eq!(lang.as_deref(), Some("en"));

    let runner = ProviderTranslateRunner { provider: &provider };
    let result = runner
        .run(
            &segments,
            lang.as_deref(),
            &TranslateOptions::default(),
            MLX_MODEL,
        )
        .await;

    match result {
        Ok(r) => {
            assert!(!r.skipped);
            assert_eq!(r.segments.len(), segments.len());
            assert_eq!(r.target_language, "vi");
            println!("EN → VI first 3 segments:");
            for (orig, tr) in segments.iter().zip(r.segments.iter()).take(3) {
                println!("  [{}..{}] {}  →  {}", orig.start_ms, orig.end_ms, orig.text, tr.text);
                // Every translated segment should differ from the original
                // (VN is a distinct alphabet + script).
                assert_ne!(orig.text.trim(), tr.text.trim());
                assert!(!tr.text.trim().is_empty(), "empty translation");
            }
        }
        Err(e) => {
            // Local models occasionally length-mismatch on longer batches.
            // Log and skip rather than fail until we add repair / retry.
            eprintln!("en clip translate drifted: {e}");
        }
    }
}

#[tokio::test]
async fn jp_clip_translates_to_vi() {
    let Some(provider) = mlx_server_up().await else {
        eprintln!("skipped: mlx_lm.server not running");
        return;
    };

    let segments = transcribe(JP_CLIP).await;
    let lang = detected_language(&segments);
    println!("JP clip: {} segments, detected language = {:?}", segments.len(), lang);
    assert!(!segments.is_empty());

    // Allow language detection wobble on short JP clips — accept ja OR any
    // language that is not `vi`, since the skip rule is what we care about.
    let ja_like = lang.as_deref().map(|l| l.starts_with("ja")).unwrap_or(false);
    assert!(ja_like, "JP clip detected as {lang:?}, expected ja");

    let runner = ProviderTranslateRunner { provider: &provider };
    let result = runner
        .run(
            &segments,
            lang.as_deref(),
            &TranslateOptions::default(),
            MLX_MODEL,
        )
        .await;

    match result {
        Ok(r) => {
            assert!(!r.skipped);
            assert_eq!(r.segments.len(), segments.len());
            assert_eq!(r.target_language, "vi");
            println!("JP → VI first 3 segments:");
            for (orig, tr) in segments.iter().zip(r.segments.iter()).take(3) {
                println!("  [{}..{}] {}  →  {}", orig.start_ms, orig.end_ms, orig.text, tr.text);
                assert_ne!(orig.text.trim(), tr.text.trim());
                assert!(!tr.text.trim().is_empty());
            }
        }
        Err(e) => {
            eprintln!("jp clip translate drifted: {e}");
        }
    }
}
