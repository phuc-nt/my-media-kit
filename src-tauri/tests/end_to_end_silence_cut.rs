//! End-to-end pipeline smoke test exercised from the app crate so we can
//! pull in every kit at once. Runs the full non-LLM flow:
//!
//!   1. ffprobe duration
//!   2. extract PCM via ffmpeg
//!   3. silence detection with default config
//!   4. invert to keep ranges
//!   5. run the direct cut-and-concat export
//!   6. ffprobe the output to verify it's a real video file
//!
//! Skipped unless `MY_MEDIA_KIT_TEST_MEDIA` points at a file.

use std::path::PathBuf;

use creator_core::SilenceDetectorConfig;
use media_kit::{cut_and_concat, extract_pcm_samples, probe_media};
use silence_kit::{detect_silence, invert_regions};

fn test_media_path() -> Option<PathBuf> {
    std::env::var("MY_MEDIA_KIT_TEST_MEDIA")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

#[tokio::test]
async fn silence_cut_direct_export_roundtrip() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: MY_MEDIA_KIT_TEST_MEDIA not set");
        return;
    };

    let probe = probe_media(&path).await.expect("probe");
    println!("source: {} ms", probe.duration_ms);

    let samples = extract_pcm_samples(&path).await.expect("pcm");
    let result = detect_silence(&samples, &SilenceDetectorConfig::default(), None);
    println!(
        "silence regions: {} (frames analysed: {})",
        result.regions.len(),
        result.rms_values.len()
    );

    let keeps = invert_regions(&result.regions, probe.duration_ms);
    let expected_out_ms: i64 = keeps.iter().map(|(s, e)| e - s).sum();
    println!(
        "keep ranges: {} ({} ms → {} ms)",
        keeps.len(),
        probe.duration_ms,
        expected_out_ms
    );

    let out = std::env::temp_dir().join("my_media_kit_e2e_silence_cut.mp4");
    if out.exists() {
        let _ = std::fs::remove_file(&out);
    }

    cut_and_concat(&path, &out, &keeps, "libx264", "aac")
        .await
        .expect("cut_and_concat");

    let metadata = std::fs::metadata(&out).expect("output metadata");
    println!("output file: {} bytes", metadata.len());
    assert!(metadata.len() > 10_000, "output too small");

    let out_probe = probe_media(&out).await.expect("output probe");
    let drift = (out_probe.duration_ms - expected_out_ms).abs();
    println!(
        "output duration: {} ms (expected {}, drift {})",
        out_probe.duration_ms, expected_out_ms, drift
    );
    assert!(drift < 500, "duration drift too large: {} ms", drift);

    // Compression ratio sanity: we removed silence, so the output should be
    // shorter than the input by the total silence duration ± a few hundred
    // milliseconds of ffmpeg seek overhead.
    let removed_ms: i64 = result.regions.iter().map(|r| r.end_ms - r.start_ms).sum();
    let shortened_by = probe.duration_ms - out_probe.duration_ms;
    println!(
        "removed silence: {} ms, actual shortening: {} ms",
        removed_ms, shortened_by
    );
    assert!(
        (shortened_by - removed_ms).abs() < 500,
        "shortened by {} ms but removed silence was {} ms",
        shortened_by,
        removed_ms
    );

    let _ = std::fs::remove_file(&out);
}
