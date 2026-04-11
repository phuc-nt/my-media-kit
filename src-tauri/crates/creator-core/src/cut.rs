//! CutRegion + CutReason — the shared model fed into timeline rendering and
//! media export. Every detector (silence, filler, ai-prompt) emits `CutRegion`
//! values and the view model merges + sorts them before export.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::time::TimeRangeMs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CutReason {
    Silence,
    Filler,
    Duplicate,
    Manual,
    AiPrompt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutRegion {
    pub id: Uuid,
    pub start_ms: i64,
    pub end_ms: i64,
    pub reason: CutReason,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl CutRegion {
    pub fn new(start_ms: i64, end_ms: i64, reason: CutReason) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_ms,
            end_ms,
            reason,
            enabled: true,
        }
    }

    pub fn range(&self) -> TimeRangeMs {
        TimeRangeMs::new(self.start_ms, self.end_ms)
    }

    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_enabled() {
        let c = CutRegion::new(0, 1_000, CutReason::Silence);
        assert!(c.enabled);
        assert_eq!(c.duration_ms(), 1_000);
    }

    #[test]
    fn serde_roundtrip_preserves_fields() {
        let c = CutRegion::new(100, 500, CutReason::Filler);
        let json = serde_json::to_string(&c).unwrap();
        let back: CutRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn camel_case_reason_serialization() {
        let c = CutRegion::new(0, 100, CutReason::AiPrompt);
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["reason"], "aiPrompt");
    }
}
