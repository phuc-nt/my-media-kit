//! Auto-manage `mlx_lm.server` lifecycle.
//!
//! On first use of any LLM feature the frontend calls `ensure_mlx_lm_server`.
//! If the server is not running we spawn it; it auto-downloads the model from
//! HuggingFace on first run (~5 GB). Progress events are emitted so the UI
//! can show a meaningful status instead of a spinner.
//!
//! The spawned PID is stored in AppState and killed when the app exits.

use std::time::Duration;

use serde_json::json;
use tauri::{command, AppHandle, Emitter, State};

use ai_kit::providers::mlx_lm::DEFAULT_MODEL;

use crate::state::AppState;

pub const MLX_SERVER_EVENT: &str = "mlx_server_status";
const MLX_SERVER_ADDR: &str = "127.0.0.1:8080";
const STARTUP_TIMEOUT_SECS: u64 = 1800; // allow up to 30 min for first-run download

/// Check if mlx_lm.server is listening. Uses a raw TCP connect so we don't
/// need reqwest in the main crate — lighter and plenty fast enough.
async fn is_server_ready() -> bool {
    tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(MLX_SERVER_ADDR),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Ensure `mlx_lm.server` is running with the configured model.
/// - If already running: returns immediately.
/// - If not running: spawns it and waits (emitting status events).
/// - If `mlx_lm` is not installed: returns a helpful error.
#[command]
pub async fn ensure_mlx_lm_server(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if is_server_ready().await {
        let _ = app.emit(MLX_SERVER_EVENT, json!({"status": "ready"}));
        return Ok(());
    }

    // Resolve binary — try PATH first, then common pip locations.
    let binary = which_mlx_lm_server().ok_or_else(|| {
        "mlx_lm not found. Install with: pip install mlx-lm".to_string()
    })?;

    let _ = app.emit(MLX_SERVER_EVENT, json!({
        "status": "starting",
        "message": format!("Starting AI engine ({DEFAULT_MODEL})…")
    }));

    let child = std::process::Command::new(&binary)
        .args(["--model", DEFAULT_MODEL, "--port", "8080"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn mlx_lm.server: {e}"))?;

    let pid = child.id();
    if let Ok(mut guard) = state.mlx_server_pid.lock() {
        *guard = Some(pid);
    }

    // Poll until ready — first run downloads the model from HuggingFace.
    let mut elapsed = 0u64;
    let mut download_notified = false;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        elapsed += 2;

        if is_server_ready().await {
            let _ = app.emit(MLX_SERVER_EVENT, json!({"status": "ready"}));
            return Ok(());
        }

        if !download_notified && elapsed > 10 {
            download_notified = true;
            let _ = app.emit(MLX_SERVER_EVENT, json!({
                "status": "downloading",
                "message": format!("Downloading AI model ({DEFAULT_MODEL}, ~9 GB — first run only)…")
            }));
        }

        if elapsed >= STARTUP_TIMEOUT_SECS {
            return Err(format!(
                "mlx_lm.server did not become ready within {STARTUP_TIMEOUT_SECS}s"
            ));
        }
    }
}

/// Kill the managed mlx_lm.server process (called on app exit).
#[command]
pub async fn stop_mlx_lm_server(state: State<'_, AppState>) -> Result<(), String> {
    kill_server_pid(&state);
    Ok(())
}

pub fn kill_server_pid(state: &AppState) {
    if let Ok(mut guard) = state.mlx_server_pid.lock() {
        if let Some(pid) = guard.take() {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .status();
            }
        }
    }
}

/// Search common locations for the `mlx_lm.server` binary.
fn which_mlx_lm_server() -> Option<String> {
    // Try PATH first via `which`.
    if let Ok(out) = std::process::Command::new("which")
        .arg("mlx_lm.server")
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }
    // Common pip install locations on macOS.
    let candidates = [
        "/usr/local/bin/mlx_lm.server",
        "/opt/homebrew/bin/mlx_lm.server",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(c.to_string());
        }
    }
    None
}
