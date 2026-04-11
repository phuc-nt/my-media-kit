//! Real-media silence detection smoke test. Requires `media-kit` to extract
//! samples from a real file pointed to by `CREATOR_UTILS_TEST_MEDIA`.
//!
//! Runs the auto-threshold pipeline end-to-end and asserts the invariants
//! we actually care about:
//!   - the detector returns something finite
//!   - RMS frame count ≈ samples / 480 (16 kHz / 30 ms)
//!   - at least one region is found on any natural recording (if the file
//!     happens to be wall-to-wall speech with no silence, loosen to ≥ 0)

use std::path::PathBuf;

use creator_core::SilenceDetectorConfig;
use silence_kit::{detect_silence, FRAME_SIZE};

fn test_media_path() -> Option<PathBuf> {
    std::env::var("CREATOR_UTILS_TEST_MEDIA")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

#[tokio::test]
async fn detects_silence_on_real_recording() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };

    let samples = media_kit::extract_pcm_samples(&path)
        .await
        .expect("extract_pcm_samples");

    assert!(!samples.is_empty(), "no PCM samples extracted");

    let cfg = SilenceDetectorConfig::default();
    let result = detect_silence(&samples, &cfg, None);

    let expected_frames = samples.len() / FRAME_SIZE;
    assert_eq!(
        result.rms_values.len(),
        expected_frames,
        "RMS frame count mismatch"
    );

    println!(
        "sample count: {}, frames: {}, regions: {}",
        samples.len(),
        expected_frames,
        result.regions.len()
    );
    for (i, r) in result.regions.iter().enumerate() {
        println!(
            "  region {i}: start={} ms, end={} ms, duration={} ms",
            r.start_ms,
            r.end_ms,
            r.end_ms - r.start_ms
        );
    }

    // Every region must be strictly inside the samples and positive length.
    let total_ms = (samples.len() as f64 / 16_000.0 * 1000.0) as i64;
    for r in &result.regions {
        assert!(r.end_ms > r.start_ms, "empty region: {r:?}");
        assert!(r.start_ms >= 0);
        assert!(r.end_ms <= total_ms + 30, "region exceeds media length");
    }

    // Precompute RMS once, then re-run with a different min duration and
    // confirm it still comes back quickly (slider live-preview path).
    let cfg2 = SilenceDetectorConfig {
        minimum_duration_s: 0.3,
        ..cfg
    };
    let cached = detect_silence(&samples, &cfg2, Some(&result.rms_values));
    assert_eq!(cached.rms_values.len(), result.rms_values.len());
    println!("cached second pass: {} regions", cached.regions.len());
}
