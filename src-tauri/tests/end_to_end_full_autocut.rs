//! Full AutoCut pipeline end-to-end test — mirrors what the UI does:
//!
//!   1. Probe video metadata
//!   2. Extract PCM + detect silence regions
//!   3. Transcribe with MLX Whisper → real segments
//!   4. Run filler detection via MLX LM on the transcript
//!   5. Merge silence + filler cut regions, invert to keep ranges
//!   6. Export final MP4
//!   7. Verify output is shorter than source
//!
//! Skipped unless `CREATOR_UTILS_TEST_MEDIA` points at a real file.
//! Output is written to `/tmp/autocut_full_output.mp4` and intentionally
//! NOT deleted so the user can inspect it.

use std::path::PathBuf;

use content_kit::{
    batch::TranscriptBatch,
    filler::{AiFillerDetector, FillerDetector},
    transcript_filler_scan::scan_word_timestamps,
};
use creator_core::SilenceDetectorConfig;
use media_kit::{cut_and_concat, extract_pcm_samples, probe_media};
use silence_kit::detect_silence;

fn test_media_path() -> Option<PathBuf> {
    std::env::var("CREATOR_UTILS_TEST_MEDIA")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

#[tokio::test]
async fn full_pipeline_silence_plus_filler() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };

    // ── 1. Probe ──────────────────────────────────────────────────────────────
    let probe = probe_media(&path).await.expect("probe");
    let total_ms = probe.duration_ms;
    println!("source: {} ms", total_ms);

    // ── 2. PCM + silence detection ────────────────────────────────────────────
    let samples = extract_pcm_samples(&path).await.expect("pcm");
    let silence_result = detect_silence(&samples, &SilenceDetectorConfig::default(), None);
    let silence_regions = &silence_result.regions;
    let silence_cut_ms: i64 = silence_regions.iter().map(|r| r.end_ms - r.start_ms).sum();
    println!(
        "silence: {} regions, {} ms cut",
        silence_regions.len(),
        silence_cut_ms
    );

    // ── 3. Transcribe ─────────────────────────────────────────────────────────
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let transcript = {
        use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};
        let transcriber = MlxWhisperTranscriber::new();
        let opts = TranscriptionOptions::default();
        let segs = transcriber
            .transcribe_file(&path, &opts)
            .await
            .expect("transcribe");
        println!("transcript: {} segments", segs.len());
        for (i, seg) in segs.iter().enumerate() {
            println!(
                "  seg {i}: {}..{} ms — {}",
                seg.start_ms,
                seg.end_ms,
                seg.text.chars().take(80).collect::<String>()
            );
            for w in &seg.words {
                println!("    word: {}..{} ms  {:?}", w.start_ms, w.end_ms, w.text);
            }
        }
        segs
    };

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let transcript: Vec<creator_core::TranscriptionSegment> = {
        eprintln!("skipped transcription: not on Apple Silicon");
        vec![]
    };

    // ── 4a. Filler detection — word-timestamp scan (no AI server needed) ────────
    // Primary method: scan Whisper word timestamps directly against the filler
    // word list. Fast, offline, requires word-level timestamps in the transcript.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let word_scan_cuts: Vec<(i64, i64)> = if transcript.is_empty() {
        vec![]
    } else {
        let detections = scan_word_timestamps(&transcript, 150); // merge gap 150ms
        let cut_ms: i64 = detections.iter().map(|d| d.cut_end_ms - d.cut_start_ms).sum();
        println!(
            "word-scan fillers: {} detections, {} ms cut",
            detections.len(),
            cut_ms
        );
        for d in &detections {
            println!(
                "  word-scan: {}..{} ms — {:?}",
                d.cut_start_ms, d.cut_end_ms, d.filler_words
            );
        }
        detections.iter().map(|d| (d.cut_start_ms, d.cut_end_ms)).collect()
    };

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let word_scan_cuts: Vec<(i64, i64)> = vec![];

    // ── 4b. Filler detection — AI LM (optional, runs if server is up) ─────────
    // Secondary method: AI model for context-aware detection (catches fillers
    // that word-scan misses due to ambiguous words like "thì", "mà", "kiểu").
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let ai_filler_cuts: Vec<(i64, i64)> = if transcript.is_empty() {
        vec![]
    } else {
        use ai_kit::{MlxLmProvider, Provider};
        let provider = MlxLmProvider::default_local();
        if !provider.is_available().await {
            eprintln!("MLX LM server not running — using word-scan only");
            vec![]
        } else {
            let model = std::env::var("CREATOR_UTILS_LM_MODEL")
                .unwrap_or_else(|_| "mlx-community/gemma-4-E4B-it-4bit".into());
            let batch = TranscriptBatch {
                batch_index: 0,
                first_segment_index: 0,
                segments: transcript.clone(),
            };
            let detector = AiFillerDetector { provider: &provider };
            match detector.detect(&batch, &model).await {
                Ok(detections) => {
                    let cut_ms: i64 = detections.iter().map(|d| d.cut_end_ms - d.cut_start_ms).sum();
                    println!("ai-filler: {} detections, {} ms cut", detections.len(), cut_ms);
                    for d in &detections {
                        println!(
                            "  ai: {}..{} ms — {:?} (\"{}\")",
                            d.cut_start_ms, d.cut_end_ms, d.filler_words, d.text
                        );
                    }
                    detections.iter().map(|d| (d.cut_start_ms, d.cut_end_ms)).collect()
                }
                Err(e) => {
                    eprintln!("ai filler detection failed (non-fatal): {e}");
                    vec![]
                }
            }
        }
    };

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let ai_filler_cuts: Vec<(i64, i64)> = vec![];

    // ── 4c. Intra-word silence scan — silence pockets within long words ─────────
    // Words assigned >600ms by Whisper likely hide a filler sound (the filler's
    // audio energy is attributed to the adjacent content word's timestamp span).
    // Running silence detection on just that sub-range finds the actual pause
    // portion (the silence that bookends the filler), giving us a real cut.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let intra_word_cuts: Vec<(i64, i64)> = {
        const MIN_LONG_WORD_MS: i64 = 900;  // Vietnamese syllable rarely exceeds 600ms naturally
        const EDGE_BUFFER_MS: i64 = 200;   // larger buffer to avoid cutting into content word

        // More sensitive config for intra-word scan: shorter min duration, no padding
        let intra_config = SilenceDetectorConfig {
            threshold: 0.015,
            use_auto_threshold: true,
            minimum_duration_s: 0.10,
            padding_left_s: 0.0,
            padding_right_s: 0.0,
            remove_short_spikes_s: 0.05,
        };
        let samples_per_ms = media_kit::TARGET_SAMPLE_RATE as f64 / 1000.0;
        let mut cuts = Vec::new();

        for seg in &transcript {
            for word in &seg.words {
                let duration_ms = word.end_ms - word.start_ms;
                if duration_ms <= MIN_LONG_WORD_MS { continue; }

                let s = (word.start_ms as f64 * samples_per_ms) as usize;
                let e = ((word.end_ms as f64 * samples_per_ms) as usize).min(samples.len());
                if s >= e { continue; }

                let sub = &samples[s..e];
                let result = detect_silence(sub, &intra_config, None);

                println!(
                    "  long word {:?} ({}ms): {} intra-silence sub-region(s)",
                    word.text.trim(), duration_ms, result.regions.len()
                );
                for region in result.regions {
                    let abs_start = word.start_ms + region.start_ms;
                    let abs_end = word.start_ms + region.end_ms;

                    // Leading pause: silence starts ≤100ms from word start and leaves
                    // ≥300ms for the actual word pronunciation after the cut.
                    let is_leading = abs_start <= word.start_ms + 100
                        && abs_end < word.end_ms - 300;
                    // Trailing pause: silence ends ≤100ms from word end and the word
                    // was pronounced for ≥300ms before the pause started.
                    let is_trailing = abs_end >= word.end_ms - 100
                        && abs_start > word.start_ms + 300;
                    // Interior silence: strictly within the word with large buffers.
                    let is_interior = abs_start > word.start_ms + EDGE_BUFFER_MS
                        && abs_end < word.end_ms - EDGE_BUFFER_MS;

                    let kind = if is_leading { "leading" }
                               else if is_trailing { "trailing" }
                               else if is_interior { "interior" }
                               else { "skip" };
                    println!(
                        "    silence: {}..{} ms ({}ms) → {}",
                        abs_start, abs_end, abs_end - abs_start, kind
                    );
                    if is_leading || is_trailing || is_interior {
                        cuts.push((abs_start, abs_end));
                    }
                }
            }
        }
        cuts
    };

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let intra_word_cuts: Vec<(i64, i64)> = vec![];

    // Combine all filler sources
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let filler_cuts: Vec<(i64, i64)> = word_scan_cuts
        .into_iter()
        .chain(ai_filler_cuts)
        .chain(intra_word_cuts)
        .collect();

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let filler_cuts: Vec<(i64, i64)> = vec![];

    // ── 5. Merge + invert → keep ranges ──────────────────────────────────────
    // Clip silence cuts against transcript speech regions to avoid cutting
    // natural pauses within speech (only cut silence between speech segments).
    let speech_regions: Vec<(i64, i64)> = transcript
        .iter()
        .map(|s| (s.start_ms, s.end_ms))
        .collect();

    fn mask_against_speech(s: i64, e: i64, speech: &[(i64, i64)]) -> Vec<(i64, i64)> {
        let mut result = Vec::new();
        let mut cursor = s;
        for &(sp_s, sp_e) in speech {
            if sp_e <= cursor { continue; }
            if sp_s >= e { break; }
            if sp_s > cursor { result.push((cursor, sp_s)); }
            cursor = cursor.max(sp_e);
        }
        if cursor < e { result.push((cursor, e)); }
        result
    }

    let masked_silence: Vec<(i64, i64)> = silence_regions
        .iter()
        .flat_map(|r| mask_against_speech(r.start_ms, r.end_ms, &speech_regions))
        .collect();

    println!(
        "silence after speech masking: {} sub-ranges (was {} regions)",
        masked_silence.len(),
        silence_regions.len()
    );

    let mut all_cuts: Vec<(i64, i64)> = masked_silence
        .iter()
        .copied()
        .chain(filler_cuts.iter().copied())
        .collect();
    all_cuts.sort_by_key(|c| c.0);

    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (s, e) in all_cuts {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 { last.1 = last.1.max(e); continue; }
        }
        merged.push((s, e));
    }

    let mut keeps: Vec<(i64, i64)> = Vec::new();
    let mut cursor = 0i64;
    for (s, e) in &merged {
        if *s > cursor { keeps.push((cursor, *s)); }
        cursor = *e;
    }
    if cursor < total_ms { keeps.push((cursor, total_ms)); }

    let keep_ms: i64 = keeps.iter().map(|(s, e)| e - s).sum();
    println!(
        "keep: {} ranges, {} ms total (removed {} ms)",
        keeps.len(),
        keep_ms,
        total_ms - keep_ms
    );

    // ── 6. Export ─────────────────────────────────────────────────────────────
    let out = PathBuf::from("/tmp/autocut_full_output.mp4");
    if out.exists() { let _ = std::fs::remove_file(&out); }

    cut_and_concat(&path, &out, &keeps, "libx264", "aac")
        .await
        .expect("export");

    let meta = std::fs::metadata(&out).expect("output metadata");
    println!("output: {} bytes", meta.len());

    // ── 7. Verify ─────────────────────────────────────────────────────────────
    assert!(meta.len() > 10_000, "output too small");
    let out_probe = probe_media(&out).await.expect("probe output");
    println!(
        "output duration: {} ms (source {} ms, removed {} ms)",
        out_probe.duration_ms,
        total_ms,
        total_ms - out_probe.duration_ms
    );
    assert!(out_probe.duration_ms < total_ms, "output should be shorter than source");

    println!("\nOutput saved to: {}", out.display());

    // ── 8. Re-transcribe output and compare quality ───────────────────────────
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        use transcription_kit::{MlxWhisperTranscriber, TranscriptionOptions};

        println!("\n── Quality verification: re-transcribing output ──");
        let transcriber = MlxWhisperTranscriber::new();
        let opts = TranscriptionOptions::default();
        let out_segs = transcriber
            .transcribe_file(&out, &opts)
            .await
            .expect("transcribe output");

        let original_text: String = transcript.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        let output_text: String = out_segs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");

        println!("\nOriginal transcript ({} segs):", transcript.len());
        for seg in &transcript {
            println!("  [{:>6}..{:<6}] {}", seg.start_ms, seg.end_ms, seg.text.trim());
        }

        println!("\nOutput transcript ({} segs):", out_segs.len());
        for seg in &out_segs {
            println!("  [{:>6}..{:<6}] {}", seg.start_ms, seg.end_ms, seg.text.trim());
        }

        // Check filler words absent from output
        let filler_patterns = ["ờ", "ừm", "ừ", "ừa", "um", " à ", "À ", " à.", "Ừm", "Ờ"];
        let fillers_in_output: Vec<&str> = filler_patterns
            .iter()
            .filter(|&&f| output_text.contains(f))
            .copied()
            .collect();

        println!("\nFiller check:");
        if fillers_in_output.is_empty() {
            println!("  PASS — no filler words detected in output transcript");
        } else {
            println!("  WARN — filler pattern(s) still present: {:?}", fillers_in_output);
        }

        // Word retention: count words preserved
        let original_words: Vec<&str> = original_text.split_whitespace().collect();
        let output_words: Vec<&str> = output_text.split_whitespace().collect();
        let retention_pct = output_words.len() as f64 / original_words.len().max(1) as f64 * 100.0;
        println!(
            "Word retention: {} / {} words ({:.1}%)",
            output_words.len(), original_words.len(), retention_pct
        );

        // Sanity: at least 50% of words retained (fillers should be small fraction)
        assert!(
            retention_pct >= 50.0,
            "too many words removed ({:.1}% retained) — likely over-cutting",
            retention_pct
        );

        println!("\nQuality verification complete.");
    }
}
