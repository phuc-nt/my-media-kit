//! End-to-end pipeline test using OpenAI API only (whisper-1 + gpt-4o-mini).
//!
//! Validates the "1 API covers all" story for non-Apple-Silicon users:
//!   1. Transcribe   — OpenAI Whisper (whisper-1)
//!   2. Translate    — OpenAI LLM (gpt-4o-mini)
//!   3. Summary      — OpenAI LLM (gpt-4o-mini)
//!   4. Chapters     — OpenAI LLM (gpt-4o-mini)
//!
//! Prerequisites:
//!   - OpenAI API key in OS keyring (service=tech.lighton.media.CreatorUtils,
//!     account=ai.provider.openai.apiKey) — or env var OPENAI_API_KEY
//!
//! Run:
//!   cargo test -p content-kit --test openai_pipeline_e2e -- --nocapture --test-threads=1

use std::path::PathBuf;

use ai_kit::{KeyringSecretStore, OpenAiProvider, SecretStore};
use content_kit::{
    chapters::{ChapterRunner, ProviderChapterRunner},
    summary::{ProviderSummaryRunner, SummaryRunner, SummaryStyle},
    translate::{ProviderTranslateRunner, TranslateOptions, TranslateRunner},
};
use creator_core::{AiProviderType, TranscriptionSegment};
use transcription_kit::OpenAiWhisperTranscriber;

const LLM_MODEL: &str = "gpt-4o-mini";
const WHISPER_MODEL: &str = "whisper-1";

fn workspace_root() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // content-kit → crates → src-tauri → workspace root
    crate_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn openai_key() -> Option<String> {
    // Prefer env var (CI), fall back to OS keyring (local dev).
    if let Ok(k) = std::env::var("OPENAI_API_KEY") {
        return Some(k);
    }
    let store = KeyringSecretStore::new();
    store.get(AiProviderType::OpenAi).unwrap_or(None)
}

async fn extract_audio(source: &std::path::Path) -> PathBuf {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let tmp = std::env::temp_dir()
        .join(format!("{stem}_oai_e2e_{}.mp3", uuid::Uuid::new_v4()));
    media_kit::extract_audio_mp3(source, &tmp)
        .await
        .expect("ffmpeg audio extraction");
    tmp
}

fn divider(label: &str) {
    println!("\n{}", "═".repeat(66));
    println!("  {label}");
    println!("{}", "═".repeat(66));
}

fn section(label: &str) {
    println!("\n── {label}");
}

#[tokio::test]
async fn openai_full_pipeline() {
    // ── Guard: API key ──────────────────────────────────────────────────
    let api_key = match openai_key() {
        Some(k) => k,
        None => {
            eprintln!("SKIP — no OpenAI API key (keyring or OPENAI_API_KEY)");
            return;
        }
    };

    // ── Test clip: EN TED (~13 min) — use first 90 s for cost efficiency ─
    let clip = workspace_root().join(
        "test-data/transcript-translate-input/What-Makes-a-Good-Life-Lessons-from-the-_Media.mp4",
    );
    if !clip.exists() {
        eprintln!("SKIP — test clip not found: {}", clip.display());
        return;
    }

    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════");
    println!("║  OPENAI FULL PIPELINE E2E");
    println!("║  Whisper: {WHISPER_MODEL}  ·  LLM: {LLM_MODEL}");
    println!("╚══════════════════════════════════════════════════════════════════");

    let provider = std::sync::Arc::new(OpenAiProvider::new(api_key.clone()));

    // ── Step 1: Transcribe ──────────────────────────────────────────────
    divider("Step 1 — Transcribe (whisper-1)");
    section("Extracting audio…");
    let audio = extract_audio(&clip).await;
    println!("  audio: {}", audio.display());

    let transcriber = OpenAiWhisperTranscriber { api_key };
    let all_segs: Vec<TranscriptionSegment> = transcriber
        .transcribe(&audio, Some("en"), Some(WHISPER_MODEL))
        .await
        .expect("OpenAI Whisper transcription");
    let _ = std::fs::remove_file(&audio);

    println!("  → {} segments total", all_segs.len());
    println!("  First 3 segments:");
    for s in all_segs.iter().take(3) {
        println!("    [{} → {}]  {}", s.start_ms / 1000, s.end_ms / 1000, s.text);
    }

    // Use first 90 s for downstream calls (cost + speed).
    let segs: Vec<TranscriptionSegment> = all_segs
        .iter()
        .take_while(|s| s.start_ms < 90_000)
        .cloned()
        .collect();
    println!("  → using {} segments (first 90 s) for downstream", segs.len());
    assert!(!segs.is_empty(), "no segments transcribed");

    // ── Step 2: Translate (EN → VI) ─────────────────────────────────────
    divider("Step 2 — Translate EN → VI (gpt-4o-mini)");
    let runner = ProviderTranslateRunner { provider: provider.as_ref() };
    let translate_opts = TranslateOptions {
        target_language: "vi".into(),
        ..TranslateOptions::default()
    };
    let translated = runner
        .run(&segs, Some("en"), &translate_opts, LLM_MODEL)
        .await
        .expect("translate");
    println!("  skipped={}", translated.skipped);
    println!("  First 3 translated segments:");
    for s in translated.segments.iter().take(3) {
        println!("    [{} → {}]  {}", s.start_ms / 1000, s.end_ms / 1000, s.text);
    }
    assert!(!translated.segments.is_empty());

    // ── Step 3: Summary ─────────────────────────────────────────────────
    divider("Step 3 — Summary brief (gpt-4o-mini)");
    let summarizer = ProviderSummaryRunner { provider: provider.as_ref() };
    let summary = summarizer
        .run(&segs, SummaryStyle::Brief, "Vietnamese", LLM_MODEL, 60.0)
        .await
        .expect("summary");
    println!("{}", summary.text);
    assert!(!summary.text.is_empty());

    // ── Step 4: Chapters ────────────────────────────────────────────────
    divider("Step 4 — Chapters (gpt-4o-mini)");
    let chaprunner = ProviderChapterRunner { provider: provider.as_ref() };
    let chapters = chaprunner
        .run(&segs, "Vietnamese", LLM_MODEL)
        .await
        .expect("chapters");
    for ch in &chapters.chapters {
        let total_s = ch.timestamp_ms / 1000;
        let m = total_s / 60;
        let s = total_s % 60;
        println!("  {m}:{s:02}  {}", ch.title);
    }
    assert!(!chapters.chapters.is_empty());

    println!("\n\n✅  OPENAI PIPELINE — ALL STEPS PASSED");
}
