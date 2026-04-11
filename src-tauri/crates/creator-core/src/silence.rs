//! Silence detection models. The `SilenceRegion` is the raw output of
//! silence-kit; the view-model later lifts them into `CutRegion` values
//! with `CutReason::Silence`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SilenceRegion {
    pub id: Uuid,
    pub start_ms: i64,
    pub end_ms: i64,
}

impl SilenceRegion {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_ms,
            end_ms,
        }
    }

    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }
}

/// Tunable parameters for silence detection. Defaults mirror the values
/// documented in the original `SilenceDetection.md` (bundled in v1 and
/// reverse-engineered in `docs/02-autocut-silence.md`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceDetectorConfig {
    /// Linear amplitude threshold in [0, 1]. Ignored when `use_auto_threshold`
    /// is true — the detector re-derives it from signal statistics.
    pub threshold: f32,
    /// When true, derive `threshold` from the audio itself (P15 noise floor
    /// + 0.15 × headroom).
    pub use_auto_threshold: bool,
    /// Minimum silence duration to emit a region, in seconds.
    pub minimum_duration_s: f64,
    /// Inward shrink from the left edge of each silence region, in seconds.
    pub padding_left_s: f64,
    /// Inward shrink from the right edge of each silence region, in seconds.
    pub padding_right_s: f64,
    /// Drop any non-silent gap shorter than this before grouping regions,
    /// in seconds. Prevents clicks / breath spikes from fragmenting silence.
    pub remove_short_spikes_s: f64,
}

impl Default for SilenceDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.03,
            use_auto_threshold: true,
            minimum_duration_s: 0.5,
            padding_left_s: 0.1,
            padding_right_s: 0.1,
            remove_short_spikes_s: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_spec() {
        let cfg = SilenceDetectorConfig::default();
        assert!(cfg.use_auto_threshold);
        assert_eq!(cfg.minimum_duration_s, 0.5);
        assert_eq!(cfg.padding_left_s, 0.1);
        assert_eq!(cfg.padding_right_s, 0.1);
        assert_eq!(cfg.remove_short_spikes_s, 0.2);
    }

    #[test]
    fn config_serialises_camel_case() {
        let cfg = SilenceDetectorConfig::default();
        let json = serde_json::to_value(&cfg).unwrap();
        assert!(json.get("useAutoThreshold").is_some());
        assert!(json.get("minimumDurationS").is_some());
    }
}
