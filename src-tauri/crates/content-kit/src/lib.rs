//! content-kit — AI-driven content analysis pipelines.
//!
//! Each feature (filler detection, AI-prompt cuts, summary, chapters) is
//! a module with:
//!   - A `prompts` submodule: pure-string prompt builders, unit-tested.
//!   - A `schema` submodule: JSON schema values + the Rust types they decode
//!     into.
//!   - A `run_*` async entry point: takes a transcript + an `&dyn Provider`,
//!     batches if needed, and returns structured output.
//!
//! Batching lives here (not in ai-kit) because the decision of how to slice
//! a transcript is content-specific — summary uses duration-based batches
//! while chapters use a 3-pass consolidation flow.

pub mod batch;
pub mod blog_article;
pub mod chapters;
pub mod duplicate;
pub mod filler;
pub mod prompt_cut;
pub mod summary;
pub mod transcript_filler_scan;
pub mod translate;
pub mod viral_clips;
pub mod youtube_pack;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
