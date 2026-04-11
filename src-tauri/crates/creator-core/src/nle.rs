//! NLE export targets. v1 shipped FCPXML + xmeml + CapCut; v2 drops CapCut
//! (see ADR-007) and exposes the two XML formats that cover Final Cut Pro,
//! Premiere, and DaVinci Resolve.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NleExportTarget {
    FinalCutPro,
    Premiere,
    DavinciResolve,
}

impl NleExportTarget {
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::FinalCutPro => "fcpxml",
            Self::Premiere | Self::DavinciResolve => "xml",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::FinalCutPro => "Final Cut Pro",
            Self::Premiere => "Adobe Premiere Pro",
            Self::DavinciResolve => "DaVinci Resolve",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_match_format() {
        assert_eq!(NleExportTarget::FinalCutPro.file_extension(), "fcpxml");
        assert_eq!(NleExportTarget::Premiere.file_extension(), "xml");
        assert_eq!(NleExportTarget::DavinciResolve.file_extension(), "xml");
    }

    #[test]
    fn serialises_camel_case() {
        let v = serde_json::to_value(NleExportTarget::FinalCutPro).unwrap();
        assert_eq!(v, "finalCutPro");
    }
}
