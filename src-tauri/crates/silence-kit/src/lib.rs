//! silence-kit — Pure DSP silence detection.
//!
//! Port of the Swift SilenceDetector documented in `docs/02-autocut-silence.md`.
//! Feeds on a mono 16 kHz PCM f32 slice (produced by media-kit from arbitrary
//! media via ffmpeg) and emits `SilenceRegion` values in milliseconds.
//!
//! Pipeline:
//!   1. `compute_frame_rms`      — non-overlapping 30 ms windows → per-frame RMS.
//!   2. `auto_threshold`         — P15 noise floor + 15 % headroom, clamped.
//!   3. `mark_silent_frames`     — threshold each RMS frame.
//!   4. `remove_short_spikes`    — merge short non-silent bursts into silence.
//!   5. `build_regions`          — group runs into `SilenceRegion`s, filter by duration.
//!   6. `apply_padding`          — inward shrink both edges to leave breath room.
//!
//! All functions are deterministic and allocation-light; `detect_silence`
//! accepts a pre-computed RMS buffer so slider edits skip the heavy step.

pub mod rms;
pub mod threshold;
pub mod regions;

use creator_core::{SilenceDetectorConfig, SilenceRegion};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SIZE_MS: u32 = 30;
pub const FRAME_SIZE: usize = (SAMPLE_RATE as usize * FRAME_SIZE_MS as usize) / 1_000; // 480

/// Output of a full silence detection pass. The RMS buffer is returned so
/// callers can cache it for fast slider re-runs.
#[derive(Debug, Clone)]
pub struct SilenceDetectionResult {
    pub regions: Vec<SilenceRegion>,
    pub rms_values: Vec<f32>,
}

/// Detect silence from raw 16 kHz mono PCM samples.
///
/// When `precomputed_rms` is supplied, the RMS computation step is skipped —
/// the caller is responsible for ensuring it matches the given samples and
/// frame size (same length as `samples.len() / FRAME_SIZE`).
pub fn detect_silence(
    samples: &[f32],
    config: &SilenceDetectorConfig,
    precomputed_rms: Option<&[f32]>,
) -> SilenceDetectionResult {
    let rms_values: Vec<f32> = match precomputed_rms {
        Some(rms) => rms.to_vec(),
        None => rms::compute_frame_rms(samples, FRAME_SIZE),
    };

    let threshold = if config.use_auto_threshold {
        threshold::auto_threshold(&rms_values)
    } else {
        config.threshold
    };

    let mut is_silent: Vec<bool> = rms_values.iter().map(|&v| v < threshold).collect();

    regions::remove_short_spikes(&mut is_silent, config.remove_short_spikes_s, FRAME_SIZE_MS);

    let raw_regions = regions::build_regions(&is_silent, config.minimum_duration_s, FRAME_SIZE_MS);
    let padded = regions::apply_padding(
        raw_regions,
        config.padding_left_s,
        config.padding_right_s,
    );

    SilenceDetectionResult {
        regions: padded,
        rms_values,
    }
}

/// Invert silence regions into "keep" regions bounded by `total_ms`.
/// Useful for direct export pipelines that concatenate non-silent segments.
pub fn invert_regions(silence: &[SilenceRegion], total_ms: i64) -> Vec<(i64, i64)> {
    let mut keeps = Vec::with_capacity(silence.len() + 1);
    let mut cursor = 0_i64;
    for r in silence {
        if r.start_ms > cursor {
            keeps.push((cursor, r.start_ms));
        }
        cursor = cursor.max(r.end_ms);
    }
    if cursor < total_ms {
        keeps.push((cursor, total_ms));
    }
    keeps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mono 16 kHz fixture: one second of speech-level noise, one
    /// second of silence, one second of speech. Expect exactly one silence
    /// region ~[1000 ms, 2000 ms].
    fn build_fixture_1s_speech_silence_speech() -> Vec<f32> {
        let sr = SAMPLE_RATE as usize;
        let mut out = Vec::with_capacity(sr * 3);
        // Segment A: sinusoidal "speech" at 200 Hz, amplitude 0.3.
        for i in 0..sr {
            let t = i as f32 / sr as f32;
            out.push((t * 2.0 * std::f32::consts::PI * 200.0).sin() * 0.3);
        }
        // Segment B: digital silence.
        out.extend(std::iter::repeat(0.0_f32).take(sr));
        // Segment C: sinusoidal speech again.
        for i in 0..sr {
            let t = i as f32 / sr as f32;
            out.push((t * 2.0 * std::f32::consts::PI * 300.0).sin() * 0.3);
        }
        out
    }

    #[test]
    fn detects_single_silence_region_in_fixture() {
        let samples = build_fixture_1s_speech_silence_speech();
        let cfg = SilenceDetectorConfig {
            // Manual threshold so the test is deterministic — auto would also
            // work but this documents the intent.
            use_auto_threshold: false,
            threshold: 0.01,
            minimum_duration_s: 0.3,
            padding_left_s: 0.0,
            padding_right_s: 0.0,
            remove_short_spikes_s: 0.0,
        };
        let result = detect_silence(&samples, &cfg, None);
        assert_eq!(
            result.regions.len(),
            1,
            "expected exactly one silence region, got {:?}",
            result.regions
        );
        let r = &result.regions[0];
        assert!(
            (r.start_ms - 1_000).abs() <= FRAME_SIZE_MS as i64,
            "silence start off: {}",
            r.start_ms
        );
        assert!(
            (r.end_ms - 2_000).abs() <= FRAME_SIZE_MS as i64,
            "silence end off: {}",
            r.end_ms
        );
    }

    #[test]
    fn precomputed_rms_skips_recomputation() {
        let samples = build_fixture_1s_speech_silence_speech();
        let cfg = SilenceDetectorConfig {
            use_auto_threshold: false,
            threshold: 0.01,
            ..SilenceDetectorConfig::default()
        };
        let first = detect_silence(&samples, &cfg, None);
        let second = detect_silence(&samples, &cfg, Some(&first.rms_values));
        assert_eq!(first.regions.len(), second.regions.len());
        for (a, b) in first.regions.iter().zip(second.regions.iter()) {
            assert_eq!(a.start_ms, b.start_ms);
            assert_eq!(a.end_ms, b.end_ms);
        }
    }

    #[test]
    fn invert_regions_produces_complementary_intervals() {
        let silence = vec![
            SilenceRegion::new(1_000, 2_000),
            SilenceRegion::new(4_000, 5_000),
        ];
        let keeps = invert_regions(&silence, 6_000);
        assert_eq!(keeps, vec![(0, 1_000), (2_000, 4_000), (5_000, 6_000)]);
    }

    #[test]
    fn invert_regions_handles_silence_at_edges() {
        let silence = vec![
            SilenceRegion::new(0, 500),
            SilenceRegion::new(2_500, 3_000),
        ];
        let keeps = invert_regions(&silence, 3_000);
        assert_eq!(keeps, vec![(500, 2_500)]);
    }

    #[test]
    fn padding_shrinks_regions_inward() {
        // Craft a 2s clean silence bounded by two 0.5s speech chunks so the
        // raw silence region is large enough for padding to matter.
        let sr = SAMPLE_RATE as usize;
        let mut s = Vec::with_capacity(sr * 3);
        for i in 0..(sr / 2) {
            let t = i as f32 / sr as f32;
            s.push((t * 2.0 * std::f32::consts::PI * 220.0).sin() * 0.3);
        }
        s.extend(std::iter::repeat(0.0_f32).take(sr * 2));
        for i in 0..(sr / 2) {
            let t = i as f32 / sr as f32;
            s.push((t * 2.0 * std::f32::consts::PI * 220.0).sin() * 0.3);
        }
        let cfg = SilenceDetectorConfig {
            use_auto_threshold: false,
            threshold: 0.01,
            minimum_duration_s: 0.5,
            padding_left_s: 0.2,
            padding_right_s: 0.2,
            remove_short_spikes_s: 0.0,
        };
        let result = detect_silence(&s, &cfg, None);
        assert_eq!(result.regions.len(), 1);
        let r = &result.regions[0];
        // Expect roughly [500+200, 2500-200] = [700, 2300]
        assert!((r.start_ms - 700).abs() <= FRAME_SIZE_MS as i64);
        assert!((r.end_ms - 2_300).abs() <= FRAME_SIZE_MS as i64);
    }
}
