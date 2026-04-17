//! E2E test for the new content features:
//!   - YouTube Content Pack (titles + description + tags)
//!   - Viral Clip Finder (best short-form moments)
//!   - Blog Article (structured article from transcript)
//!   - Clean Transcript (rule-based filler removal)
//!
//! Uses OpenAI (gpt-4o-mini) on a 3-min crop of the EN TED clip.
//! Skips gracefully when OPENAI_API_KEY is absent.
//!
//! Run:
//!   cargo test -p content-kit --test new_features_e2e -- --nocapture --test-threads=1

use std::path::PathBuf;

use ai_kit::{KeyringSecretStore, OpenAiProvider, SecretStore};
use content_kit::{
    blog_article::{BlogArticleRunner, ProviderBlogArticleRunner},
    transcript_filler_scan,
    viral_clips::{ProviderViralClipRunner, ViralClipRunner},
    youtube_pack::{ProviderYouTubePackRunner, YouTubePackRunner},
};
use creator_core::AiProviderType;
use transcription_kit::OpenAiWhisperTranscriber;

const LLM_MODEL: &str = "gpt-4o-mini";

fn workspace_root() -> PathBuf {
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

async fn extract_audio_cropped(source: &std::path::Path, max_secs: u32) -> (PathBuf, TempFile) {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let tmp = std::env::temp_dir()
        .join(format!("{stem}_new_feat_{}.mp3", uuid::Uuid::new_v4()));
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
            "-i", source.to_str().unwrap(),
            "-t", &max_secs.to_string(),
            "-vn", "-ac", "1", "-ar", "16000", "-b:a", "32k",
            tmp.to_str().unwrap(),
        ])
        .status().await.expect("ffmpeg");
    assert!(status.success());
    (tmp.clone(), TempFile(tmp))
}

fn yt_ts(ms: i64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

#[tokio::test]
async fn new_features_pipeline() {
    let api_key = match openai_key() {
        Some(k) => k,
        None => { eprintln!("SKIP — no OpenAI API key"); return; }
    };

    let en_path = workspace_root()
        .join("test-data/transcript-translate-input/What-Makes-a-Good-Life-Lessons-from-the-_Media.mp4");
    if !en_path.exists() {
        eprintln!("SKIP — test clip not found: {}", en_path.display());
        return;
    }

    println!("\n╔═══════════════════════════════════════════════════════════");
    println!("║  NEW FEATURES E2E — EN TED (3 min crop)");
    println!("║  LLM: {LLM_MODEL}");
    println!("╚═══════════════════════════════════════════════════════════");

    // ── Transcribe ────────────────────────────────────────────────────────
    let (audio, _g) = extract_audio_cropped(&en_path, 180).await;
    let t = OpenAiWhisperTranscriber { api_key: api_key.clone() };
    let segs = t.transcribe(&audio, Some("en"), Some("whisper-1")).await.expect("whisper");
    println!("\n[ASR] {} segments", segs.len());
    assert!(!segs.is_empty());

    let provider = std::sync::Arc::new(OpenAiProvider::new(api_key));

    // ══════════════════════════════════════════════════════════════════════
    // 1. YouTube Content Pack
    // ══════════════════════════════════════════════════════════════════════
    println!("\n{}", "═".repeat(60));
    println!("  1. YouTube Content Pack");
    let yt_runner = ProviderYouTubePackRunner { provider: provider.as_ref() };
    let pack = yt_runner.run(&segs, "Vietnamese", LLM_MODEL).await.expect("youtube_pack");

    println!("  Titles ({}):", pack.titles.len());
    for (i, t) in pack.titles.iter().enumerate() {
        println!("    {}. {}", i + 1, t);
    }
    println!("  Description ({} chars):", pack.description.len());
    let preview: String = pack.description.chars().take(150).collect();
    println!("    {preview}…");
    println!("  Tags ({}): {}", pack.tags.len(), pack.tags.join(", "));

    assert!(pack.titles.len() >= 3, "expected ≥3 titles, got {}", pack.titles.len());
    assert!(!pack.description.is_empty(), "empty description");
    assert!(pack.tags.len() >= 5, "expected ≥5 tags, got {}", pack.tags.len());
    println!("  ✓ YouTube Pack OK");

    // ══════════════════════════════════════════════════════════════════════
    // 2. Viral Clips
    // ══════════════════════════════════════════════════════════════════════
    println!("\n  2. Viral Clips");
    let vc_runner = ProviderViralClipRunner { provider: provider.as_ref() };
    let clips = vc_runner.run(&segs, "Vietnamese", LLM_MODEL).await.expect("viral_clips");

    println!("  Found {} clips:", clips.clips.len());
    for (i, c) in clips.clips.iter().enumerate() {
        let dur = (c.end_ms - c.start_ms) / 1000;
        println!("    #{} {} – {} ({dur}s)", i + 1, yt_ts(c.start_ms), yt_ts(c.end_ms));
        println!("       Hook: {}", c.hook);
        println!("       Caption: {}", c.caption);
    }

    assert!(!clips.clips.is_empty(), "no viral clips found");
    for c in &clips.clips {
        assert!(c.end_ms > c.start_ms, "clip end before start");
        assert!(!c.hook.is_empty(), "empty hook");
    }
    println!("  ✓ Viral Clips OK");

    // ══════════════════════════════════════════════════════════════════════
    // 3. Blog Article
    // ══════════════════════════════════════════════════════════════════════
    println!("\n  3. Blog Article");
    let blog_runner = ProviderBlogArticleRunner { provider: provider.as_ref() };
    let article = blog_runner.run(&segs, "Vietnamese", LLM_MODEL, 60.0).await.expect("blog_article");

    println!("  Title: {}", article.title);
    println!("  Sections ({}):", article.sections.len());
    for s in &article.sections {
        println!("    ## {} ({} chars)", s.heading, s.content.len());
    }

    assert!(!article.title.is_empty(), "empty article title");
    assert!(!article.sections.is_empty(), "no article sections");
    for s in &article.sections {
        assert!(!s.heading.is_empty(), "empty section heading");
        assert!(!s.content.is_empty(), "empty section content");
    }
    println!("  ✓ Blog Article OK");

    // ══════════════════════════════════════════════════════════════════════
    // 4. Clean Transcript (rule-based, no AI)
    // ══════════════════════════════════════════════════════════════════════
    println!("\n  4. Clean Transcript (rule-based)");
    let fillers = transcript_filler_scan::scan_word_timestamps(&segs, 100);
    println!("  Filler detections: {}", fillers.len());
    for f in fillers.iter().take(5) {
        println!("    [{} – {}] {:?}", f.cut_start_ms, f.cut_end_ms, f.filler_words);
    }
    // Rule-based scan should at least run without error. Filler count depends
    // on word-timestamp availability (OpenAI Whisper may or may not provide them).
    println!("  ✓ Clean Transcript OK");

    // ══════════════════════════════════════════════════════════════════════
    println!("\n══════════════════════════════════════════════════════════");
    println!("  ALL NEW FEATURES PASS");
    println!("  YouTube Pack: {} titles, {} tags", pack.titles.len(), pack.tags.len());
    println!("  Viral Clips:  {} clips", clips.clips.len());
    println!("  Blog Article: {} sections", article.sections.len());
    println!("  Clean:        {} fillers detected", fillers.len());
    println!("══════════════════════════════════════════════════════════\n");
}
