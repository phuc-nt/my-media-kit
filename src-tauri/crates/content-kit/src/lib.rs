//! content-kit — AI-driven content analysis pipelines.
//!
//! Each feature module builds prompts, defines output schemas, and exposes
//! an async `run` entry point that takes a transcript plus an `&dyn Provider`.
//! Batching lives here (not in ai-kit) because slicing a transcript is
//! content-specific — summary uses duration batches, translate slides context,
//! etc.

pub mod batch;
pub mod chapters;
pub mod summary;
pub mod transcript_filler_scan;
pub mod translate;
pub mod viral_clips;
pub mod youtube_pack;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
