//! Whisper model catalog. Mirrors the set of ggml models the original app
//! bundles / downloads, hosted under huggingface.co/ggerganov/whisper.cpp.
//!
//! The catalog is data-only so UI code can list models, show sizes, and
//! generate download URLs without linking whisper-rs.

use serde::{Deserialize, Serialize};

pub const HUGGINGFACE_BASE: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WhisperModelId {
    Tiny,
    TinyEn,
    Base,
    BaseEn,
    Small,
    SmallEn,
    Medium,
    MediumEn,
    LargeV1,
    LargeV2,
    LargeV3,
    LargeV3Turbo,
}

impl WhisperModelId {
    pub fn all() -> &'static [WhisperModelId] {
        use WhisperModelId::*;
        &[
            Tiny, TinyEn, Base, BaseEn, Small, SmallEn, Medium, MediumEn, LargeV1, LargeV2,
            LargeV3, LargeV3Turbo,
        ]
    }

    pub fn file_name(&self) -> &'static str {
        use WhisperModelId::*;
        match self {
            Tiny => "ggml-tiny.bin",
            TinyEn => "ggml-tiny.en.bin",
            Base => "ggml-base.bin",
            BaseEn => "ggml-base.en.bin",
            Small => "ggml-small.bin",
            SmallEn => "ggml-small.en.bin",
            Medium => "ggml-medium.bin",
            MediumEn => "ggml-medium.en.bin",
            LargeV1 => "ggml-large-v1.bin",
            LargeV2 => "ggml-large-v2.bin",
            LargeV3 => "ggml-large-v3.bin",
            LargeV3Turbo => "ggml-large-v3-turbo.bin",
        }
    }

    pub fn display_name(&self) -> &'static str {
        use WhisperModelId::*;
        match self {
            Tiny => "Tiny (multilingual)",
            TinyEn => "Tiny (English only)",
            Base => "Base (multilingual)",
            BaseEn => "Base (English only)",
            Small => "Small (multilingual)",
            SmallEn => "Small (English only)",
            Medium => "Medium (multilingual)",
            MediumEn => "Medium (English only)",
            LargeV1 => "Large v1",
            LargeV2 => "Large v2",
            LargeV3 => "Large v3",
            LargeV3Turbo => "Large v3 Turbo",
        }
    }

    /// Rough download size in megabytes; used for the model picker UI.
    pub fn size_mb(&self) -> u32 {
        use WhisperModelId::*;
        match self {
            Tiny | TinyEn => 39,
            Base | BaseEn => 74,
            Small | SmallEn => 244,
            Medium | MediumEn => 769,
            LargeV1 | LargeV2 | LargeV3 => 1550,
            LargeV3Turbo => 809,
        }
    }

    /// Multilingual if `false` means English-only.
    pub fn is_multilingual(&self) -> bool {
        use WhisperModelId::*;
        !matches!(self, TinyEn | BaseEn | SmallEn | MediumEn)
    }

    pub fn download_url(&self) -> String {
        format!("{}/{}", HUGGINGFACE_BASE, self.file_name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: WhisperModelId,
    pub display_name: &'static str,
    pub file_name: &'static str,
    pub size_mb: u32,
    pub multilingual: bool,
    pub download_url: String,
}

pub struct ModelCatalog;

impl ModelCatalog {
    pub fn entries() -> Vec<CatalogEntry> {
        WhisperModelId::all()
            .iter()
            .map(|id| CatalogEntry {
                id: *id,
                display_name: id.display_name(),
                file_name: id.file_name(),
                size_mb: id.size_mb(),
                multilingual: id.is_multilingual(),
                download_url: id.download_url(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_url_points_to_huggingface() {
        let url = WhisperModelId::BaseEn.download_url();
        assert!(url.starts_with(HUGGINGFACE_BASE));
        assert!(url.ends_with("ggml-base.en.bin"));
    }

    #[test]
    fn english_only_models_flagged() {
        assert!(!WhisperModelId::BaseEn.is_multilingual());
        assert!(WhisperModelId::Base.is_multilingual());
    }

    #[test]
    fn catalog_returns_all_models() {
        let entries = ModelCatalog::entries();
        assert_eq!(entries.len(), WhisperModelId::all().len());
        assert!(entries.iter().all(|e| !e.file_name.is_empty()));
        assert!(entries.iter().all(|e| e.size_mb > 0));
    }

    #[test]
    fn large_turbo_smaller_than_large_v3() {
        assert!(WhisperModelId::LargeV3Turbo.size_mb() < WhisperModelId::LargeV3.size_mb());
    }
}
