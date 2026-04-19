//! Use-case quality review test — LOCAL MLX, 2 clips.
//!
//! Unlike the benchmark E2E test, this prints COMPLETE output for each use case
//! so a human can judge content quality, not just correctness.
//!
//! Use cases tested:
//!   UC1  YouTube Description Generator  — Chapters (YT format) + Brief summary
//!   UC2  Bilingual SRT Preview          — Original segs + translated segs side-by-side
//!   UC3  Foreign Content Digest         — Full transcript EN→vi summary
//!   UC4  Meeting Notes (Action Items)   — Summary (action items) on short clip
//!
//! Clips:
//!   - EN TED: "What Makes a Good Life" (~13 min, lang=en) → tests UC1/UC2/UC3
//!   - AutoCut MOV: short vi book intro (~20 s, lang=vi) → tests UC4
//!
//! Prerequisites: same as mlx_local_e2e.rs
//!
//! Run:
//!   cargo test -p content-kit --test mlx_use_case_quality -- --nocapture --test-threads=1

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::PathBuf;

use ai_kit::{MlxLmProvider, Provider};
use content_kit::{
    chapters::{ChapterRunner, ProviderChapterRunner},
    summary::{ProviderSummaryRunner, SummaryRunner, SummaryStyle},
    translate::{ProviderTranslateRunner, TranslateOptions, TranslateRunner},
};
use creator_core::TranscriptionSegment;
use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

fn mlx_lm_model() -> String {
    std::env::var("MLX_LM_MODEL")
        .unwrap_or_else(|_| "mlx-community/Qwen2.5-7B-Instruct-4bit".into())
}

fn workspace_root() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_tauri = crate_dir.parent().unwrap().parent().unwrap();
    src_tauri.parent().unwrap().to_path_buf()
}

async fn extract_audio(source: &std::path::Path) -> PathBuf {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let tmp = std::env::temp_dir().join(format!("{stem}_uc_quality_{}.mp3", uuid::Uuid::new_v4()));
    media_kit::extract_audio_mp3(source, &tmp)
        .await
        .expect("extract audio");
    tmp
}

/// Format chapter timestamp as YouTube MM:SS string.
fn yt_ts(ms: i64) -> String {
    let total_s = ms / 1000;
    let m = total_s / 60;
    let s = total_s % 60;
    format!("{m}:{s:02}")
}

/// Format segment timestamp range as SRT timestamp (HH:MM:SS,mmm).
fn srt_ts(ms: i64) -> String {
    let total_s = ms / 1000;
    let h = total_s / 3600;
    let m = (total_s % 3600) / 60;
    let s = total_s % 60;
    let millis = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{millis:03}")
}

fn divider(label: &str) {
    println!("\n{}", "═".repeat(66));
    println!("  {label}");
    println!("{}", "═".repeat(66));
}

fn section(label: &str) {
    println!("\n── {label} ──────────────────────────────────────────────────────");
}

#[tokio::test]
async fn use_case_quality_review() {
    // ── Guard: prerequisites (same pattern as mlx_local_e2e) ───────────
    let provider = MlxLmProvider::default_local();
    if !provider.is_available().await {
        eprintln!("SKIP — mlx_lm.server not running on 127.0.0.1:8080");
        return;
    }

    let model = mlx_lm_model();

    let en_path = workspace_root().join(
        "test-data/transcript-translate-input/What-Makes-a-Good-Life-Lessons-from-the-_Media.mp4",
    );
    let mov_path = workspace_root().join("test-data/auto-cut-input/IMG_0451.MOV");

    if !en_path.exists() || !mov_path.exists() {
        println!("SKIP — test clips not found");
        return;
    }

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════");
    println!("║  USE-CASE QUALITY REVIEW — MLX LOCAL");
    println!("║  LLM: {model}");
    println!("║  Whisper: mlx-community/whisper-large-v3-turbo");
    println!("╚══════════════════════════════════════════════════════════════════");

    // ── Transcribe EN TED ───────────────────────────────────────────────
    section("Transcribing EN TED…");
    let audio_en = extract_audio(&en_path).await;
    let transcriber = MlxWhisperTranscriber::new();
    let mut opts = TranscriptionOptions::default();
    opts.language = Some("en".into());
    let en_segs: Vec<TranscriptionSegment> = transcriber
        .transcribe_file(&audio_en, &opts)
        .await
        .expect("ASR EN TED");
    let _ = std::fs::remove_file(&audio_en);
    println!("  → {} segments, lang=en", en_segs.len());

    // Use first 3 min for feature calls (quality > speed trade-off).
    let en_3min: Vec<TranscriptionSegment> = en_segs
        .iter()
        .take_while(|s| s.start_ms < 180_000)
        .cloned()
        .collect();

    // ── Transcribe AutoCut MOV ──────────────────────────────────────────
    section("Transcribing AutoCut MOV…");
    let audio_mov = extract_audio(&mov_path).await;
    let mut opts_vi = TranscriptionOptions::default();
    opts_vi.language = None; // auto-detect
    let mov_segs: Vec<TranscriptionSegment> = transcriber
        .transcribe_file(&audio_mov, &opts_vi)
        .await
        .expect("ASR MOV");
    let _ = std::fs::remove_file(&audio_mov);
    let detected_mov_lang = mov_segs.iter().find_map(|s| s.language.clone());
    println!("  → {} segments, lang={:?}", mov_segs.len(), detected_mov_lang);

    // ════════════════════════════════════════════════════════════════════
    // UC1: YouTube Description Generator
    // ════════════════════════════════════════════════════════════════════
    divider("UC1 · YouTube Description Generator");
    println!("  Input: EN TED ({} segs, first 3 min)", en_3min.len());

    let ch_runner = ProviderChapterRunner { provider: &provider };
    let chapters = ch_runner
        .run(&en_3min, "English", &model)
        .await
        .expect("UC1 chapters");

    let sum_runner = ProviderSummaryRunner { provider: &provider };
    let summary_brief = sum_runner
        .run(&en_3min, SummaryStyle::Brief, "English", &model, 60.0, None)
        .await
        .expect("UC1 summary");

    println!("\n  ▶ CHAPTERS (YouTube format):");
    for ch in &chapters.chapters {
        println!("    {}  {}", yt_ts(ch.timestamp_ms), ch.title);
    }
    println!("\n  ▶ DESCRIPTION:");
    println!("    {}", summary_brief.text);

    // ════════════════════════════════════════════════════════════════════
    // UC2: Bilingual SRT Preview
    // ════════════════════════════════════════════════════════════════════
    divider("UC2 · Bilingual SRT Preview (EN → VI)");
    println!("  Input: EN TED first 90 s");

    let en_90s: Vec<TranscriptionSegment> = en_segs
        .iter()
        .take_while(|s| s.start_ms < 90_000)
        .cloned()
        .collect();

    let tr_runner = ProviderTranslateRunner { provider: &provider };
    let tr_opts = TranslateOptions {
        target_language: "vi".into(),
        max_batch_seconds: 45.0,
    };
    let translate_result = tr_runner
        .run(&en_90s, Some("en"), &tr_opts, &model)
        .await
        .expect("UC2 translate");

    println!("\n  ▶ BILINGUAL SRT (original + translation):");
    let paired: Vec<_> = en_90s.iter().zip(translate_result.segments.iter()).collect();
    for (i, (orig, trans)) in paired.iter().enumerate() {
        let idx = i + 1;
        let start = srt_ts(orig.start_ms);
        let end = srt_ts(orig.end_ms);
        println!("  {idx}");
        println!("  {start} --> {end}");
        println!("  [EN] {}", orig.text.trim());
        println!("  [VI] {}", trans.text.trim());
        println!();
    }

    // ════════════════════════════════════════════════════════════════════
    // UC3: Foreign Content Digest (EN → Vietnamese summary)
    // ════════════════════════════════════════════════════════════════════
    divider("UC3 · Foreign Content Digest (summary in Vietnamese)");
    println!("  Input: EN TED first 3 min → summary output in Vietnamese");

    let digest_summary = sum_runner
        .run(&en_3min, SummaryStyle::KeyPoints, "Vietnamese", &model, 60.0, None)
        .await
        .expect("UC3 summary vi");

    println!("\n  ▶ KEY POINTS (Vietnamese):");
    println!("    {}", digest_summary.text);

    // ════════════════════════════════════════════════════════════════════
    // UC4: Meeting Notes — Action Items
    // ════════════════════════════════════════════════════════════════════
    divider("UC4 · Meeting Notes — Action Items");
    println!("  Input: AutoCut MOV ({} segs, lang={:?})", mov_segs.len(), detected_mov_lang);

    let action_sum_runner = ProviderSummaryRunner { provider: &provider };
    let action_items = action_sum_runner
        .run(&mov_segs, SummaryStyle::ActionItems, "Vietnamese", &model, 60.0, None)
        .await
        .expect("UC4 action items");

    println!("\n  ▶ ACTION ITEMS:");
    println!("    {}", action_items.text);

    // ════════════════════════════════════════════════════════════════════
    divider("QUALITY REVIEW COMPLETE — human review of output above");
    println!("  All use cases ran successfully.");
    println!("  Review chapter titles, summary coherence, and translation accuracy.");
    println!();
}
