//! Millisecond-based time ranges. We use i64 ms throughout the domain layer
//! so (a) serialization is compact, (b) equality is exact (no float drift),
//! and (c) frame-aligning for NLE export is straightforward.
//!
//! Floating-point seconds are only used when crossing a platform boundary
//! that insists on it (ffmpeg CLI, JS `Date.now()`).

use serde::{Deserialize, Serialize};

/// Half-open millisecond range `[start_ms, end_ms)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeRangeMs {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl TimeRangeMs {
    pub fn new(start_ms: i64, end_ms: i64) -> Self {
        Self { start_ms, end_ms }
    }

    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }

    pub fn duration_seconds(&self) -> f64 {
        ms_to_seconds(self.duration_ms())
    }

    pub fn is_empty(&self) -> bool {
        self.end_ms <= self.start_ms
    }

    pub fn intersects(&self, other: &TimeRangeMs) -> bool {
        self.start_ms < other.end_ms && other.start_ms < self.end_ms
    }

    pub fn contains_ms(&self, t_ms: i64) -> bool {
        t_ms >= self.start_ms && t_ms < self.end_ms
    }
}

pub fn seconds_to_ms(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

pub fn ms_to_seconds(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_matches() {
        let r = TimeRangeMs::new(1_000, 2_500);
        assert_eq!(r.duration_ms(), 1_500);
        assert!((r.duration_seconds() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn intersects_when_overlapping() {
        let a = TimeRangeMs::new(0, 1_000);
        let b = TimeRangeMs::new(500, 1_500);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn disjoint_ranges_do_not_intersect() {
        let a = TimeRangeMs::new(0, 1_000);
        let b = TimeRangeMs::new(1_000, 2_000);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn seconds_roundtrip_is_exact_for_whole_ms() {
        assert_eq!(seconds_to_ms(1.234), 1_234);
        assert!((ms_to_seconds(1_234) - 1.234).abs() < 1e-9);
    }
}
