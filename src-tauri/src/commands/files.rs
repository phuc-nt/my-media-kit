//! Simple file-save command used by the Transcribe / Translate views to
//! persist results next to the source video. Kept dumb on purpose: the
//! frontend formats the content (SRT, TXT, JSON) and hands over a path +
//! string body. We just create the parent directory if needed and write.
//!
//! Security: we resolve the target to an absolute path and reject anything
//! whose parent does not already exist or whose name is empty. We do NOT
//! sandbox to a specific root — the user is expected to pass a path the
//! frontend derived from their own source file.

use std::path::PathBuf;

use tauri::command;

#[command]
pub async fn save_text_file(path: String, content: String) -> Result<String, String> {
    let target = PathBuf::from(&path);
    if target.as_os_str().is_empty() {
        return Err("save target path is empty".into());
    }
    let file_name = target
        .file_name()
        .ok_or_else(|| "save target has no file name".to_string())?;
    if file_name.is_empty() {
        return Err("save target has no file name".into());
    }

    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create parent dir {}: {e}", parent.display()))?;
        }
    }

    std::fs::write(&target, content.as_bytes())
        .map_err(|e| format!("write {}: {e}", target.display()))?;

    Ok(target.display().to_string())
}
