//! End-to-end pipeline test — REAL video, REAL Groq API, 4 clips.
//!
//! Flow (same clip feeds every stage):
//!
//!   video.mp4  ─►  media_kit::extract_audio_mp3  ─►  tmp.mp3
//!                                                      │
//!                                         GroqWhisperTranscriber (ASR)
//!                                                      │
//!                                           Vec<TranscriptionSegment>
//!                                                      │
//!             ┌───────┬────────┬─────────┬───────────┬──────────┬─────────┐
//!             ▼       ▼        ▼         ▼           ▼          ▼         ▼
//!          Summary  Chapters  Filler  Duplicate  PromptCut   Translate
//!
//! Clips exercised:
//!   - English  TED talk  (EN → VI translate)
//!   - Japanese TED talk  (JP → VI translate)
//!   - Vietnamese TED     (translate MUST skip — source == target)
//!   - IMG_0451.MOV       (AutoCut demo clip, unknown language)
//!
//! Skipped (not failed) when no Groq key is available.
//!
//! ── API key ────────────────────────────────────────────────────────────
//! Preferred: export `GROQ_API_KEY` in your shell — no keychain prompt.
//!   echo 'export GROQ_API_KEY="gsk_…"' >> ~/.zshrc
//! Fallback: OS keychain (requires you to click "Allow" every time cargo
//!   rebuilds the test binary — which is every run during dev).
//!
//! Run:
//!   GROQ_API_KEY=gsk_… cargo test -p content-kit --test groq_api_e2e \
//!       -- --nocapture --test-threads=1
//!
//! Runtime budget: ~90 s per clip = ~6 min total for all 4.

use std::path::{Path, PathBuf};

use ai_kit::{GroqProvider, KeyringSecretStore, Provider, SecretStore};
use content_kit::{
    batch::{chunk_segments, TranscriptBatch},
    chapters::{ChapterRunner, ProviderChapterRunner},
    duplicate::{AiDuplicateDetector, DuplicateDetector},
    filler::{AiFillerDetector, FillerDetector},
    prompt_cut::{AiPromptCutter, ProviderCutter},
    summary::{ProviderSummaryRunner, SummaryRunner, SummaryStyle},
    translate::{ProviderTranslateRunner, TranslateOptions, TranslateRunner},
};
use creator_core::{AiProviderType, TranscriptionSegment};
use transcription_kit::{GroqWhisperTranscriber, TranscriptionOptions};

const GROQ_LLM_MODEL: &str = "llama-3.3-70b-versatile";
const GROQ_ASR_MODEL: &str = "whisper-large-v3-turbo";

/// 90 s of transcript is plenty to exercise every feature without blowing
/// the LLM token budget or test runtime.
const FEATURE_BUDGET_MS: i64 = 90_000;

#[derive(Clone, Copy)]
struct Clip {
    label: &'static str,
    path: &'static str,
    /// Hint passed to Whisper (`None` = auto-detect).
    asr_hint: Option<&'static str>,
    /// If `Some`, assert the translate runner sets `skipped = true` for
    /// this clip (i.e. the detected language already matches VI).
    expect_translate_skip: bool,
    /// Expected primary BCP-47 tag from Whisper (`None` = don't assert).
    expect_lang_prefix: Option<&'static str>,
}

const CLIPS: &[Clip] = &[
    Clip {
        label: "English TED",
        path: "test-data/transcript-translate-input/What-Makes-a-Good-Life-Lessons-from-the-_Media.mp4",
        asr_hint: Some("en"),
        expect_translate_skip: false,
        expect_lang_prefix: Some("en"),
    },
    Clip {
        label: "Japanese TED",
        path: "test-data/transcript-translate-input/Hope-invites-Tsutomu-Uematsu-TEDxSapporo.mp4",
        asr_hint: Some("ja"),
        expect_translate_skip: false,
        expect_lang_prefix: Some("ja"),
    },
    Clip {
        label: "Vietnamese TED",
        path: "test-data/transcript-translate-input/Su-that-ve-tam-ly-hoc-khong-gian-can-nha.mp4",
        asr_hint: Some("vi"),
        expect_translate_skip: true,
        expect_lang_prefix: Some("vi"),
    },
    Clip {
        label: "AutoCut IMG_0451",
        path: "test-data/auto-cut-input/IMG_0451.MOV",
        asr_hint: None,
        expect_translate_skip: false,
        expect_lang_prefix: None,
    },
];

fn workspace_root() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_tauri = crate_dir.parent().unwrap().parent().unwrap().to_path_buf();
    src_tauri.parent().unwrap().to_path_buf()
}

/// Prefer env var (no macOS keychain prompt), fall back to keyring.
fn groq_api_key() -> Option<String> {
    if let Ok(k) = std::env::var("GROQ_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k);
        }
    }
    KeyringSecretStore::new()
        .get(AiProviderType::Groq)
        .ok()
        .flatten()
        .filter(|k| !k.trim().is_empty())
}

struct TempAudio(PathBuf);
impl Drop for TempAudio {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn prepare_audio(source: &Path) -> PathBuf {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let tmp = std::env::temp_dir().join(format!("{stem}_e2e_{}.mp3", uuid::Uuid::new_v4()));
    media_kit::extract_audio_mp3(source, &tmp)
        .await
        .expect("extract audio");
    tmp
}

fn truncate_for_features(segments: &[TranscriptionSegment]) -> Vec<TranscriptionSegment> {
    segments
        .iter()
        .take_while(|s| s.start_ms < FEATURE_BUDGET_MS)
        .cloned()
        .collect()
}

/// Per-clip outcome so the top-level test can assert "all succeeded" once.
struct ClipReport {
    label: &'static str,
    asr_seg_count: usize,
    asr_lang: Option<String>,
    summary_ok: bool,
    chapters_ok: bool,
    filler_ok: bool,
    duplicate_ok: bool,
    prompt_cut_ok: bool,
    translate_ok: bool,
}

async fn run_pipeline(
    clip: &Clip,
    provider: &GroqProvider,
    asr_key: &str,
) -> Result<ClipReport, String> {
    let path = workspace_root().join(clip.path);
    if !path.exists() {
        return Err(format!("missing clip file: {}", path.display()));
    }

    println!("\n╔════════════════════════════════════════════════════════════════");
    println!("║  {}", clip.label);
    println!("║  {}", path.display());
    println!("╚════════════════════════════════════════════════════════════════");

    // ── 1. audio extract ────────────────────────────────────────────────
    let audio_path = prepare_audio(&path).await;
    let _guard = TempAudio(audio_path.clone());
    let audio_kb = std::fs::metadata(&audio_path).unwrap().len() / 1024;
    println!("[1/7] audio: {audio_kb} KB");

    // ── 2. ASR ──────────────────────────────────────────────────────────
    let transcriber = GroqWhisperTranscriber::new(asr_key.to_string());
    let mut asr_opts = TranscriptionOptions::default();
    asr_opts.language = clip.asr_hint.map(str::to_string);

    let t0 = std::time::Instant::now();
    let segments = transcriber
        .transcribe_file(&audio_path, Some(GROQ_ASR_MODEL), &asr_opts)
        .await
        .map_err(|e| format!("asr failed: {e}"))?;
    let asr_ms = t0.elapsed().as_millis();

    if segments.is_empty() {
        return Err("whisper returned zero segments".into());
    }
    let detected_lang = segments.iter().find_map(|s| s.language.clone());
    println!(
        "[2/7] ASR: {} seg in {} ms  lang={:?}",
        segments.len(),
        asr_ms,
        detected_lang
    );
    if let (Some(expected), Some(actual)) = (clip.expect_lang_prefix, detected_lang.as_deref()) {
        // Whisper returns either tag ("ja") or display name ("Japanese") —
        // accept both by checking case-insensitive prefix or display name.
        let lower = actual.to_lowercase();
        let prefix_ok = lower.starts_with(expected);
        let display_ok = match expected {
            "en" => lower.starts_with("eng"),
            "ja" => lower.starts_with("jap"),
            "vi" => lower.starts_with("vie"),
            _ => false,
        };
        if !(prefix_ok || display_ok) {
            eprintln!(
                "[WARN] language mismatch: expected {expected:?}, got {actual:?}"
            );
        }
    }
    for s in segments.iter().take(2) {
        println!(
            "      [{:>6}..{:>6}ms] {}",
            s.start_ms,
            s.end_ms,
            s.text.chars().take(70).collect::<String>()
        );
    }

    let short = truncate_for_features(&segments);
    println!(
        "      feature-set: {} seg (0..{} ms)",
        short.len(),
        short.last().map(|s| s.end_ms).unwrap_or(0)
    );

    // ── 3. Summary ──────────────────────────────────────────────────────
    let lang_name = detected_lang.clone().unwrap_or_else(|| "English".into());
    let runner = ProviderSummaryRunner { provider };
    let t0 = std::time::Instant::now();
    let summary_ok = match runner
        .run(&short, SummaryStyle::Brief, &lang_name, GROQ_LLM_MODEL, 180.0)
        .await
    {
        Ok(s) => {
            let preview: String = s.text.chars().take(200).collect();
            println!(
                "[3/7] Summary ({} ms): {}",
                t0.elapsed().as_millis(),
                preview
            );
            !s.text.trim().is_empty()
        }
        Err(e) => {
            eprintln!("[3/7] summary errored: {e}");
            false
        }
    };

    // ── 4. Chapters ─────────────────────────────────────────────────────
    let runner = ProviderChapterRunner { provider };
    let t0 = std::time::Instant::now();
    let chapters_ok = match runner.run(&short, &lang_name, GROQ_LLM_MODEL).await {
        Ok(list) => {
            println!(
                "[4/7] Chapters ({} in {} ms):",
                list.chapters.len(),
                t0.elapsed().as_millis()
            );
            for c in list.chapters.iter().take(4) {
                println!("      {:>7}ms — {}", c.timestamp_ms, c.title);
            }
            !list.chapters.is_empty() && list.chapters[0].timestamp_ms == 0
        }
        Err(e) => {
            eprintln!("[4/7] chapters errored: {e}");
            false
        }
    };

    let feature_batch = TranscriptBatch {
        batch_index: 0,
        first_segment_index: 0,
        segments: short.clone(),
    };

    // ── 5. Filler ───────────────────────────────────────────────────────
    let detector = AiFillerDetector { provider };
    let t0 = std::time::Instant::now();
    let filler_ok = match detector.detect(&feature_batch, GROQ_LLM_MODEL).await {
        Ok(f) => {
            println!(
                "[5/7] Filler: {} hits in {} ms",
                f.len(),
                t0.elapsed().as_millis()
            );
            true
        }
        Err(e) => {
            eprintln!("[5/7] filler errored: {e}");
            false
        }
    };

    // ── 6. Duplicate ────────────────────────────────────────────────────
    let detector = AiDuplicateDetector { provider };
    let t0 = std::time::Instant::now();
    let duplicate_ok = match detector.detect(&feature_batch, GROQ_LLM_MODEL).await {
        Ok(d) => {
            println!(
                "[6/7] Duplicate: {} hits in {} ms",
                d.len(),
                t0.elapsed().as_millis()
            );
            true
        }
        Err(e) => {
            eprintln!("[6/7] duplicate errored: {e}");
            false
        }
    };

    // ── 7a. Prompt cut ──────────────────────────────────────────────────
    let cutter = ProviderCutter { provider };
    let t0 = std::time::Instant::now();
    let prompt_cut_ok = match cutter
        .detect(
            &feature_batch,
            "remove any meta-commentary about the talk itself",
            GROQ_LLM_MODEL,
        )
        .await
    {
        Ok(c) => {
            println!(
                "[7a/7] PromptCut: {} hits in {} ms",
                c.len(),
                t0.elapsed().as_millis()
            );
            true
        }
        Err(e) => {
            eprintln!("[7a/7] prompt_cut errored: {e}");
            false
        }
    };

    // ── 7b. Translate ───────────────────────────────────────────────────
    let runner = ProviderTranslateRunner { provider };
    let opts = TranslateOptions {
        target_language: "vi".into(),
        max_batch_seconds: 45.0,
    };
    let planned = chunk_segments(&short, 45.0).len();
    let t0 = std::time::Instant::now();
    let translate_ok = match runner
        .run(&short, detected_lang.as_deref(), &opts, GROQ_LLM_MODEL)
        .await
    {
        Ok(res) => {
            println!(
                "[7b/7] Translate: {} seg in {} ms (skipped={}, planned {} batches)",
                res.segments.len(),
                t0.elapsed().as_millis(),
                res.skipped,
                planned
            );
            if clip.expect_translate_skip {
                assert!(res.skipped, "VN clip must skip translation");
            }
            if !res.skipped {
                for (orig, tr) in short.iter().zip(res.segments.iter()).take(2) {
                    println!(
                        "       SRC: {}",
                        orig.text.chars().take(70).collect::<String>()
                    );
                    println!(
                        "       VI : {}",
                        tr.text.chars().take(70).collect::<String>()
                    );
                }
            }
            res.segments.len() == short.len()
        }
        Err(e) => {
            eprintln!("[7b/7] translate errored: {e}");
            false
        }
    };

    Ok(ClipReport {
        label: clip.label,
        asr_seg_count: segments.len(),
        asr_lang: detected_lang,
        summary_ok,
        chapters_ok,
        filler_ok,
        duplicate_ok,
        prompt_cut_ok,
        translate_ok,
    })
}

#[tokio::test]
async fn groq_full_pipeline_on_all_test_clips() {
    let Some(api_key) = groq_api_key() else {
        eprintln!("skipped: no Groq API key (set GROQ_API_KEY env var)");
        return;
    };
    let provider = GroqProvider::new(api_key.clone());
    assert!(provider.is_available().await);

    println!("\n════════════════════════════════════════════════════════════════");
    println!("  Groq full pipeline E2E — {} clips", CLIPS.len());
    println!("  LLM: {GROQ_LLM_MODEL}");
    println!("  ASR: {GROQ_ASR_MODEL}");
    println!("════════════════════════════════════════════════════════════════");

    let mut reports = Vec::new();
    for clip in CLIPS {
        match run_pipeline(clip, &provider, &api_key).await {
            Ok(r) => reports.push(r),
            Err(e) => eprintln!("✗ {} — {}", clip.label, e),
        }
    }

    // ── Final summary table ─────────────────────────────────────────────
    println!("\n\n════════════════════════════════════════════════════════════════");
    println!("  RESULTS");
    println!("════════════════════════════════════════════════════════════════");
    println!(
        "  {:<22} {:>4} {:<8} {:^5} {:^5} {:^5} {:^5} {:^5} {:^5}",
        "clip", "seg", "lang", "sum", "chp", "fil", "dup", "cut", "trn"
    );
    for r in &reports {
        let lang = r.asr_lang.clone().unwrap_or_else(|| "?".into());
        println!(
            "  {:<22} {:>4} {:<8} {:^5} {:^5} {:^5} {:^5} {:^5} {:^5}",
            r.label,
            r.asr_seg_count,
            lang.chars().take(8).collect::<String>(),
            if r.summary_ok { "✓" } else { "✗" },
            if r.chapters_ok { "✓" } else { "✗" },
            if r.filler_ok { "✓" } else { "✗" },
            if r.duplicate_ok { "✓" } else { "✗" },
            if r.prompt_cut_ok { "✓" } else { "✗" },
            if r.translate_ok { "✓" } else { "✗" },
        );
    }
    println!();

    assert_eq!(
        reports.len(),
        CLIPS.len(),
        "every clip must complete (see stderr for failures)"
    );
    for r in &reports {
        assert!(r.summary_ok, "{} summary failed", r.label);
        assert!(r.chapters_ok, "{} chapters failed", r.label);
        assert!(r.filler_ok, "{} filler failed", r.label);
        assert!(r.duplicate_ok, "{} duplicate failed", r.label);
        assert!(r.prompt_cut_ok, "{} prompt_cut failed", r.label);
        assert!(r.translate_ok, "{} translate failed", r.label);
    }
}
