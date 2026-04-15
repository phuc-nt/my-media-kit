// Sidebar source-picker wiring. Single source of truth for which file the
// user is operating on. Pushes to `source-store` so every feature view
// can subscribe and auto-refresh its "no source selected" placeholder.

import { getSource, setSourcePath, setTranscript, setProbe, subscribe } from "./source-store.js";

const { invoke } = window.__TAURI__.core;

export function initSourcePanel() {
  const input = document.getElementById("source-path");
  const meta = document.getElementById("source-meta");
  const clearBtn = document.getElementById("btn-source-clear");
  const pickerArea = document.getElementById("source-picker-area");

  async function commitSource() {
    setSourcePath(input.value);
    const { path } = getSource();
    if (!path) return;
    try {
      const p = await invoke("media_probe", { path });
      // Normalise to camelCase keys for consistent frontend use
      setProbe({
        durationMs: p.duration_ms,
        width: p.width,
        height: p.height,
        frameRate: p.frame_rate,
        audioChannels: p.audio_channels,
      });
    } catch (_) {
      // Non-fatal: probe failure just means NLE export uses defaults
      setProbe(null);
    }
  }

  input.addEventListener("change", commitSource);
  input.addEventListener("blur", commitSource);

  // Drag-and-drop: accept files dropped anywhere on the sidebar picker area.
  // Tauri extends the File object with a `path` property (absolute OS path).
  pickerArea.addEventListener("dragover", (e) => {
    e.preventDefault();
    pickerArea.classList.add("drag-active");
  });
  pickerArea.addEventListener("dragleave", () => {
    pickerArea.classList.remove("drag-active");
  });
  pickerArea.addEventListener("drop", (e) => {
    e.preventDefault();
    pickerArea.classList.remove("drag-active");
    const file = e.dataTransfer?.files?.[0];
    if (!file) return;
    // Tauri adds .path to File objects dropped from the OS file manager.
    const absPath = file.path || file.name;
    if (absPath) {
      input.value = absPath;
      commitSource();
    }
  });

  clearBtn.addEventListener("click", async () => {
    const { path } = getSource();
    try {
      await invoke("clear_cache", { path: path || null });
      setTranscript(null);
      meta.textContent = path ? `cleared cache for ${basename(path)}` : "cache cleared";
    } catch (e) {
      meta.textContent = "clear failed: " + e;
    }
  });

  subscribe((state) => {
    if (!state.path) {
      meta.textContent = "no file selected";
      return;
    }
    const probeBit = state.probe
      ? ` · ${fmtDuration(state.probe.durationMs)} · ${state.probe.width}×${state.probe.height}`
      : "";
    const tBit = state.transcript
      ? ` · transcript (${state.transcript.segments.length} segs)`
      : "";
    meta.textContent = `${basename(state.path)}${probeBit}${tBit}`;
  });
}

function basename(p) {
  if (!p) return "";
  const sep = p.includes("/") ? "/" : "\\";
  const parts = p.split(sep);
  return parts[parts.length - 1] || p;
}

function fmtDuration(ms) {
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}h${String(m % 60).padStart(2, "0")}m`;
  return `${m}m${String(s % 60).padStart(2, "0")}s`;
}
