//! Comprehensive OpenAI pipeline E2E — all clips, all use cases.
//!
//! Tests the "1 API covers all" story with whisper-1 + gpt-4o-mini:
//!
//!   UC1  YouTube Description — EN TED chapters (YT format) + brief summary
//!   UC2  Bilingual SRT       — EN TED segments + EN→VI translation side-by-side
//!   UC3  Foreign Digest      — JP TED transcribed, summarised in Vietnamese
//!   UC4  VI Content Notes    — Vietnamese clip key points + action items
//!   UC5  Short clip check    — 21 s MOV transcription quality sanity check
//!
//! Audio is cropped to ≤ 3 min per clip for cost control (whisper-1 = $0.006/min).
//! Estimated total API cost: ~$0.12 whisper + minimal gpt-4o-mini.
//!
//! Prerequisites:
//!   OpenAI key in keyring (service=tech.lighton.media.My Media Kit,
//!   account=ai.provider.openai.apiKey) or env var OPENAI_API_KEY.
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

// ── Helpers ─────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    // content-kit → crates → src-tauri → workspace root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap()
        .parent().unwrap()
        .to_path_buf()
}

fn openai_key() -> Option<String> {
    if let Ok(k) = std::env::var("OPENAI_API_KEY") {
        return Some(k);
    }
    KeyringSecretStore::new().get(AiProviderType::OpenAi).unwrap_or(None)
}

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}

/// Extract audio from `source`, cropped to `max_secs`. Returns path + RAII guard.
async fn extract_audio_cropped(source: &std::path::Path, max_secs: u32) -> (PathBuf, TempFile) {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let tmp = std::env::temp_dir()
        .join(format!("{stem}_oai_{}.mp3", uuid::Uuid::new_v4()));

    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
            "-i", source.to_str().unwrap(),
            "-t", &max_secs.to_string(),
            "-vn", "-ac", "1", "-ar", "16000", "-b:a", "32k",
            tmp.to_str().unwrap(),
        ])
        .status()
        .await
        .expect("ffmpeg crop");
    assert!(status.success(), "ffmpeg crop failed for {}", source.display());

    let guard = TempFile(tmp.clone());
    (tmp, guard)
}

fn divider(label: &str) {
    println!("\n{}", "═".repeat(70));
    println!("  {label}");
    println!("{}", "═".repeat(70));
}

fn section(label: &str) {
    println!("\n── {label}");
}

fn yt_ts(ms: i64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

fn srt_ts(ms: i64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02},{:03}", s / 3600, (s % 3600) / 60, s % 60, ms % 1000)
}

async fn transcribe(
    t: &OpenAiWhisperTranscriber,
    audio: &std::path::Path,
    lang: Option<&str>,
) -> Vec<TranscriptionSegment> {
    t.transcribe(audio, lang, Some(WHISPER_MODEL))
        .await
        .expect("OpenAI Whisper")
}

// ── Test ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn openai_all_use_cases() {
    // ── Guard ──────────────────────────────────────────────────────────────
    let api_key = match openai_key() {
        Some(k) => k,
        None => { eprintln!("SKIP — no OpenAI API key"); return; }
    };

    let root = workspace_root();
    let en_path  = root.join("test-data/transcript-translate-input/What-Makes-a-Good-Life-Lessons-from-the-_Media.mp4");
    let jp_path  = root.join("test-data/transcript-translate-input/Hope-invites-Tsutomu-Uematsu-TEDxSapporo.mp4");
    let vi_path  = root.join("test-data/transcript-translate-input/Su-that-ve-tam-ly-hoc-khong-gian-can-nha.mp4");
    let mov_path = root.join("test-data/auto-cut-input/IMG_0451.MOV");

    for p in [&en_path, &jp_path, &vi_path, &mov_path] {
        if !p.exists() {
            eprintln!("SKIP — clip not found: {}", p.display());
            return;
        }
    }

    println!("\n╔══════════════════════════════════════════════════════════════════════");
    println!("║  OPENAI FULL USE-CASE SUITE");
    println!("║  Whisper: {WHISPER_MODEL}  ·  LLM: {LLM_MODEL}");
    println!("╚══════════════════════════════════════════════════════════════════════");

    let t = OpenAiWhisperTranscriber { api_key: api_key.clone() };
    let provider = std::sync::Arc::new(OpenAiProvider::new(api_key));
    let sum_r = ProviderSummaryRunner { provider: provider.as_ref() };
    let ch_r  = ProviderChapterRunner { provider: provider.as_ref() };
    let tr_r  = ProviderTranslateRunner { provider: provider.as_ref() };
    let tr_opts_vi = TranslateOptions { target_language: "vi".into(), ..Default::default() };

    // ── Transcribe all clips (≤ 3 min each) ────────────────────────────────
    section("Transcribing all clips (≤ 3 min crop)…");

    let (en_audio, _g1) = extract_audio_cropped(&en_path, 180).await;
    let en_segs = transcribe(&t, &en_audio, Some("en")).await;
    println!("  EN TED  → {} segs", en_segs.len());

    let (jp_audio, _g2) = extract_audio_cropped(&jp_path, 180).await;
    let jp_segs = transcribe(&t, &jp_audio, None).await;
    let jp_lang = jp_segs.iter().find_map(|s| s.language.clone());
    println!("  JP TED  → {} segs, lang={jp_lang:?}", jp_segs.len());

    let (vi_audio, _g3) = extract_audio_cropped(&vi_path, 180).await;
    let vi_segs = transcribe(&t, &vi_audio, None).await;
    let vi_lang = vi_segs.iter().find_map(|s| s.language.clone());
    println!("  VI clip → {} segs, lang={vi_lang:?}", vi_segs.len());

    let (mov_audio, _g4) = extract_audio_cropped(&mov_path, 30).await; // full ~21 s
    let mov_segs = transcribe(&t, &mov_audio, None).await;
    let mov_lang = mov_segs.iter().find_map(|s| s.language.clone());
    println!("  MOV     → {} segs, lang={mov_lang:?}", mov_segs.len());

    assert!(!en_segs.is_empty(),  "EN TED produced no segments");
    assert!(!jp_segs.is_empty(),  "JP TED produced no segments");
    assert!(!vi_segs.is_empty(),  "VI clip produced no segments");
    assert!(!mov_segs.is_empty(), "MOV produced no segments");

    // ══════════════════════════════════════════════════════════════════════
    // UC1: YouTube Description Generator — EN TED
    // ══════════════════════════════════════════════════════════════════════
    divider("UC1 · YouTube Description Generator (EN TED)");
    println!("  {} segments", en_segs.len());

    let uc1_ch = ch_r.run(&en_segs, "English", LLM_MODEL).await.expect("UC1 chapters");
    let uc1_sum = sum_r.run(&en_segs, SummaryStyle::Brief, "Vietnamese", LLM_MODEL, 60.0).await.expect("UC1 summary");

    println!("\n  ▶ CHAPTERS:");
    for ch in &uc1_ch.chapters {
        println!("    {}  {}", yt_ts(ch.timestamp_ms), ch.title);
    }
    println!("\n  ▶ DESCRIPTION (Vietnamese):");
    println!("    {}", uc1_sum.text);

    assert!(!uc1_ch.chapters.is_empty());
    assert!(!uc1_sum.text.is_empty());

    // ══════════════════════════════════════════════════════════════════════
    // UC2: Bilingual SRT — EN TED first 90 s
    // ══════════════════════════════════════════════════════════════════════
    divider("UC2 · Bilingual SRT Preview (EN → VI)");

    let en_90s: Vec<_> = en_segs.iter().take_while(|s| s.start_ms < 90_000).cloned().collect();
    println!("  {} segments (first 90 s)", en_90s.len());

    let uc2_tr = tr_r.run(&en_90s, Some("en"), &tr_opts_vi, LLM_MODEL).await.expect("UC2 translate");

    println!("\n  ▶ BILINGUAL SRT:");
    for (i, (orig, trans)) in en_90s.iter().zip(uc2_tr.segments.iter()).enumerate() {
        println!("  {}", i + 1);
        println!("  {} --> {}", srt_ts(orig.start_ms), srt_ts(orig.end_ms));
        println!("  [EN] {}", orig.text.trim());
        println!("  [VI] {}", trans.text.trim());
        println!();
    }
    assert_eq!(uc2_tr.segments.len(), en_90s.len(), "translation count mismatch");

    // ══════════════════════════════════════════════════════════════════════
    // UC3: Foreign Content Digest — JP TED → Vietnamese
    // ══════════════════════════════════════════════════════════════════════
    divider("UC3 · Foreign Content Digest (JP TED → Vietnamese)");
    println!("  {} segments, detected lang={jp_lang:?}", jp_segs.len());

    let uc3_sum = sum_r
        .run(&jp_segs, SummaryStyle::KeyPoints, "Vietnamese", LLM_MODEL, 60.0)
        .await
        .expect("UC3 key points");

    println!("\n  ▶ KEY POINTS (Vietnamese):");
    println!("    {}", uc3_sum.text);
    assert!(!uc3_sum.text.is_empty());

    // ══════════════════════════════════════════════════════════════════════
    // UC4: VI Content Notes — key points + action items
    // ══════════════════════════════════════════════════════════════════════
    divider("UC4 · Vietnamese Content Notes");
    println!("  {} segments, detected lang={vi_lang:?}", vi_segs.len());

    let uc4_kp = sum_r
        .run(&vi_segs, SummaryStyle::KeyPoints, "Vietnamese", LLM_MODEL, 60.0)
        .await
        .expect("UC4 key points");
    let uc4_ai = sum_r
        .run(&vi_segs, SummaryStyle::ActionItems, "Vietnamese", LLM_MODEL, 60.0)
        .await
        .expect("UC4 action items");

    println!("\n  ▶ KEY POINTS:");
    println!("    {}", uc4_kp.text);
    println!("\n  ▶ ACTION ITEMS:");
    println!("    {}", uc4_ai.text);
    assert!(!uc4_kp.text.is_empty());
    assert!(!uc4_ai.text.is_empty());

    // ══════════════════════════════════════════════════════════════════════
    // UC5: Short Clip Sanity — IMG_0451.MOV (~21 s)
    // ══════════════════════════════════════════════════════════════════════
    divider("UC5 · Short Clip Transcription (IMG_0451.MOV ~21 s)");
    println!("  {} segments, detected lang={mov_lang:?}", mov_segs.len());
    println!("\n  ▶ TRANSCRIPT:");
    for s in &mov_segs {
        println!("    [{} → {}]  {}", s.start_ms / 1000, s.end_ms / 1000, s.text.trim());
    }

    // ══════════════════════════════════════════════════════════════════════
    divider("ALL USE CASES COMPLETE — review output quality above");
    println!("  UC1 chapters:  {}", uc1_ch.chapters.len());
    println!("  UC2 bi-SRT:    {} pairs", en_90s.len());
    println!("  UC3 digest:    {} chars", uc3_sum.text.len());
    println!("  UC4 key pts:   {} chars / action items: {} chars", uc4_kp.text.len(), uc4_ai.text.len());
    println!("  UC5 MOV segs:  {}", mov_segs.len());
    println!();
}
