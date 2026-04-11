//! Real-media integration smoke test.
//!
//! Exercises the media-kit pipeline against an actual video file passed via
//! the `CREATOR_UTILS_TEST_MEDIA` env var. Skipped when the env var is
//! unset so `cargo test` stays fast on CI / fresh checkouts.
//!
//! Usage:
//!   CREATOR_UTILS_TEST_MEDIA=/path/to/video.mov cargo test -p media-kit \
//!     --test real_media_smoke -- --nocapture
//!
//! Checks:
//!   1. ffprobe returns a plausible duration.
//!   2. extract_pcm_samples produces N samples ≈ duration * 16000.
//!   3. cut_and_concat writes a valid file to a temp location.

use std::path::PathBuf;

fn test_media_path() -> Option<PathBuf> {
    std::env::var("CREATOR_UTILS_TEST_MEDIA")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

#[tokio::test]
async fn probe_returns_duration() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };
    let probe = media_kit::probe_media(&path).await.expect("probe_media");
    println!("probe duration: {} ms", probe.duration_ms);
    assert!(probe.duration_ms > 0, "duration must be positive");
    assert!(
        probe.duration_ms < 24 * 60 * 60 * 1_000,
        "duration looks insane, maybe parse failure"
    );
}

#[tokio::test]
async fn extract_pcm_samples_matches_duration() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };
    let probe = media_kit::probe_media(&path).await.expect("probe_media");
    let samples = media_kit::extract_pcm_samples(&path)
        .await
        .expect("extract_pcm_samples");

    // At 16 kHz mono, expected sample count = duration_s * 16_000
    let expected = (probe.duration_ms as f64 / 1000.0 * 16_000.0).round() as usize;
    let diff = (samples.len() as isize - expected as isize).unsigned_abs();
    println!(
        "extracted {} samples (expected ~{}, diff {})",
        samples.len(),
        expected,
        diff
    );
    // Allow 1% slack for end-of-stream rounding.
    assert!(
        diff < expected / 100 + 1_000,
        "sample count {} far from expected {}",
        samples.len(),
        expected
    );
    assert!(!samples.is_empty(), "no samples extracted");
    // Sanity: most real audio is nonzero.
    let nonzero = samples.iter().filter(|&&s| s.abs() > 1e-6).count();
    assert!(
        nonzero > samples.len() / 10,
        "almost everything is zero — probably a format mismatch"
    );
}

#[tokio::test]
async fn cut_and_concat_writes_output() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };

    let probe = media_kit::probe_media(&path).await.expect("probe_media");
    // Take the middle third of the clip and the last third; skip the first
    // third. That exercises both a seek-in and a concat.
    let total = probe.duration_ms;
    let keeps = vec![
        (total / 3, (total * 2) / 3),
        ((total * 2) / 3, total),
    ];

    let out = std::env::temp_dir().join("creator_utils_cut_and_concat_test.mp4");
    if out.exists() {
        let _ = std::fs::remove_file(&out);
    }

    media_kit::cut_and_concat(&path, &out, &keeps, "libx264", "aac")
        .await
        .expect("cut_and_concat");

    let metadata = std::fs::metadata(&out).expect("output file metadata");
    assert!(
        metadata.len() > 10_000,
        "output file looks empty: {} bytes",
        metadata.len()
    );

    // Probe the output to verify ffmpeg produced a readable file.
    let out_probe = media_kit::probe_media(&out).await.expect("probe output");
    let expected_ms: i64 = keeps.iter().map(|(s, e)| e - s).sum();
    let drift = (out_probe.duration_ms - expected_ms).abs();
    println!(
        "cut+concat output: {} ms (expected {}, drift {})",
        out_probe.duration_ms, expected_ms, drift
    );
    assert!(
        drift < 500,
        "output duration drift {} ms > 500 ms",
        drift
    );

    let _ = std::fs::remove_file(&out);
}
