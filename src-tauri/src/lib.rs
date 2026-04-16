//! CreatorUtils — Tauri app entry point.
//!
//! Keeps top-level concerns (command registration, state setup) here.
//! Feature implementations live in `commands/` and the `crates/` under
//! src-tauri so the app boundary stays thin and everything else stays
//! testable outside Tauri.

mod commands;
mod state;

pub use state::{AppState, TranscriptEntry};

/// Build the Tauri app and run it. Called from `main.rs` (desktop) and from
/// mobile entry points (if/when added).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .setup(|_app| {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .try_init()
                .ok();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::platform_info,
            commands::media_probe,
            commands::media_extract_pcm,
            commands::detect_silence_in_file,
            commands::ai_provider_status,
            commands::ai_has_api_key,
            commands::ai_set_api_key,
            commands::ai_delete_api_key,
            commands::ai_ping,
            commands::nle_export,
            commands::export_video_direct,
            commands::mlx_whisper_transcribe,
            commands::openai_whisper_transcribe,
            commands::content_filler_detect,
            commands::content_duplicate_detect,
            commands::content_prompt_cut,
            commands::content_summary,
            commands::content_chapters,
            commands::content_translate,
            commands::get_cached_transcript,
            commands::clear_cache,
            commands::save_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
