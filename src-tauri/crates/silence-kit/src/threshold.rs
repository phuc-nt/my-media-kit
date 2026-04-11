//! Auto-threshold derivation. Ported verbatim from the Swift reference:
//!
//!   noise_floor  = P15 of RMS values
//!   speech_level = P75
//!   raw          = noise_floor + 0.15 * (speech_level - noise_floor)
//!   clamped      = clamp(raw, 0.005, 0.5)
//!
//! Rationale (from the bundled dev doc): noise floor at P15 reliably captures
//! the quiet baseline without getting fooled by dropouts; speech level at
//! P75 covers typical voice energy without letting shouts skew the result.
//! A 15 % headroom above the noise floor is enough to ignore breath and
//! room tone while keeping soft speech audible.

pub const AUTO_FALLBACK: f32 = 0.03;
pub const MIN_CLAMP: f32 = 0.005;
pub const MAX_CLAMP: f32 = 0.5;
pub const SPEECH_HEADROOM: f32 = 0.15;

pub fn auto_threshold(rms_values: &[f32]) -> f32 {
    if rms_values.is_empty() {
        return AUTO_FALLBACK;
    }
    // Sort a clone so the input buffer stays untouched; caller usually keeps
    // the RMS vector for slider re-runs.
    let mut sorted = rms_values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();
    let noise_floor = sorted[(n * 15) / 100];
    let speech_level = sorted[(n * 75) / 100];

    let raw = noise_floor + (speech_level - noise_floor) * SPEECH_HEADROOM;
    raw.clamp(MIN_CLAMP, MAX_CLAMP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_fallback() {
        assert_eq!(auto_threshold(&[]), AUTO_FALLBACK);
    }

    #[test]
    fn clamps_below_min() {
        let rms = vec![0.0_f32; 100];
        let t = auto_threshold(&rms);
        assert!((t - MIN_CLAMP).abs() < 1e-9);
    }

    #[test]
    fn clamps_above_max() {
        let rms = vec![10.0_f32; 100];
        let t = auto_threshold(&rms);
        assert!((t - MAX_CLAMP).abs() < 1e-9);
    }

    #[test]
    fn derives_threshold_between_noise_floor_and_speech_level() {
        // 70 frames of noise (0.01) + 30 frames of speech (0.2).
        // Sorted: index 15 (P15) = 0.01, index 75 (P75) = 0.2.
        // Expect ~ 0.01 + 0.15 * 0.19 ≈ 0.0385.
        let mut rms = vec![0.01_f32; 100];
        for v in rms.iter_mut().skip(70) {
            *v = 0.2;
        }
        let t = auto_threshold(&rms);
        assert!(t > 0.03 && t < 0.05, "got {t}");
    }

    #[test]
    fn does_not_mutate_input() {
        let rms = vec![0.5_f32, 0.1, 0.3, 0.2, 0.4];
        let orig = rms.clone();
        let _ = auto_threshold(&rms);
        assert_eq!(rms, orig);
    }
}
