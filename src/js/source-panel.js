// Sidebar source-picker wiring. Single source of truth for which file the
// user is operating on. Pushes to `source-store` so every feature view
// can subscribe and auto-refresh its "no source selected" placeholder.

import { getSource, setSourcePath, setTranscript, subscribe } from "./source-store.js";

const { invoke } = window.__TAURI__.core;

export function initSourcePanel() {
  const input = document.getElementById("source-path");
  const meta = document.getElementById("source-meta");
  const clearBtn = document.getElementById("btn-source-clear");

  input.addEventListener("change", () => setSourcePath(input.value));
  input.addEventListener("blur", () => setSourcePath(input.value));

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
    const tBit = state.transcript
      ? ` · transcript cached (${state.transcript.segments.length} segs)`
      : "";
    meta.textContent = `${basename(state.path)}${tBit}`;
  });
}

function basename(p) {
  if (!p) return "";
  const sep = p.includes("/") ? "/" : "\\";
  const parts = p.split(sep);
  return parts[parts.length - 1] || p;
}
