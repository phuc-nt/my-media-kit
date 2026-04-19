//! My Media Kit — Tauri app entry point.
//!
//! Keeps top-level concerns (command registration, state setup) here.
//! Feature implementations live in `commands/` and the `crates/` under
//! src-tauri so the app boundary stays thin and everything else stays
//! testable outside Tauri.

mod commands;
mod state;

pub use state::{AppState, TranscriptEntry};

use commands::mlx_server::kill_server_pid;

/// Build the Tauri app and run it. Called from `main.rs` (desktop) and from
/// mobile entry points (if/when added).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|_app| {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .try_init()
                .ok();
            // Point media-kit at the bundled ffmpeg/ffprobe sidecars so the
            // user does not need a system install. Tauri places `externalBin`
            // entries next to the main executable (Contents/MacOS/ on macOS,
            // same dir as the .exe on Windows). If the bundled file is missing
            // (e.g. local `cargo run`), we fall back to PATH lookup.
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let resolve = |name: &str| {
                        let suffix = if cfg!(windows) { ".exe" } else { "" };
                        dir.join(format!("{name}{suffix}"))
                    };
                    let ffmpeg = resolve("ffmpeg");
                    let ffprobe = resolve("ffprobe");
                    if ffmpeg.exists() {
                        std::env::set_var("FFMPEG", ffmpeg);
                    }
                    if ffprobe.exists() {
                        std::env::set_var("FFPROBE", ffprobe);
                    }
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                use tauri::Manager;
                let state = window.app_handle().state::<AppState>();
                kill_server_pid(&state);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::platform_info,
            commands::media_probe,
            commands::ai_provider_status,
            commands::ai_has_api_key,
            commands::ai_set_api_key,
            commands::ai_delete_api_key,
            commands::ai_ping,
            commands::mlx_whisper_transcribe,
            commands::openai_whisper_transcribe,
            commands::content_summary,
            commands::content_chapters,
            commands::content_translate,
            commands::content_youtube_pack,
            commands::content_viral_clips,
            commands::content_clean_transcript,
            commands::get_cached_transcript,
            commands::clear_cache,
            commands::check_platform,
            commands::mlx_model_ready,
            commands::ensure_output_dir,
            commands::scan_output_status,
            commands::list_output_files,
            commands::load_transcript_from_output,
            commands::read_output_file,
            commands::save_text_file,
            commands::yt_dlp_download,
            commands::ensure_mlx_lm_server,
            commands::mlx_server_is_ready,
            commands::stop_mlx_lm_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
