// Sidebar source-picker wiring. Single source of truth for which file the
// user is operating on. Pushes to `source-store` so every feature view
// can subscribe and auto-refresh its "no source selected" placeholder.
//
// URL support: if the user pastes a YouTube URL, the panel downloads it via
// yt_dlp_download (cached by video ID) and then treats the local path as the
// source, identical to a dragged-in file.
//
// Drag-and-drop uses Tauri v2's webview.onDragDropEvent() API.
// File dialog uses window.__TAURI__.dialog.open() (no bundler needed).

import { getSource, setSourcePath, setTranscript, setProbe, subscribe, setAiConfig } from "./source-store.js";

const { invoke } = window.__TAURI__.core;
const { listen }  = window.__TAURI__.event;

// Patterns that identify a YouTube URL typed/pasted into the input.
const YT_URL_RE = /^https?:\/\/(www\.)?(youtube\.com\/watch|youtu\.be\/)/;

// Video/audio extensions accepted by the file dialog and drag-drop.
const MEDIA_EXTENSIONS = [
  "mp4", "mov", "mkv", "avi", "webm", "flv", "m4v", "ts",
  "mp3", "wav", "m4a", "aac", "ogg", "flac", "wma",
];

export function initSourcePanel() {
  const input      = document.getElementById("source-path");
  const meta       = document.getElementById("source-meta");
  const clearBtn   = document.getElementById("btn-source-clear");
  const browseBtn  = document.getElementById("btn-source-browse");
  const pickerArea = document.getElementById("source-picker-area");

  // ── Download progress bar ──────────────────────────────────────────────
  const progressBox = document.createElement("div");
  progressBox.className = "progress-box";
  progressBox.hidden = true;
  progressBox.innerHTML = `
    <div class="progress-head">
      <span id="yt-progress-label">downloading…</span>
      <span id="yt-progress-value">0%</span>
    </div>
    <div class="progress-track"><div id="yt-progress-bar" class="progress-bar indeterminate"></div></div>
  `;
  pickerArea.appendChild(progressBox);

  const ytBar   = progressBox.querySelector("#yt-progress-bar");
  const ytLabel = progressBox.querySelector("#yt-progress-label");
  const ytValue = progressBox.querySelector("#yt-progress-value");

  function showProgress(label, percent) {
    progressBox.hidden = false;
    ytLabel.textContent = label;
    if (typeof percent === "number") {
      ytBar.className = "progress-bar";
      ytBar.style.width = `${percent.toFixed(1)}%`;
      ytValue.textContent = `${percent.toFixed(1)}%`;
    } else {
      ytBar.className = "progress-bar indeterminate";
      ytValue.textContent = "…";
    }
  }

  function hideProgress() {
    progressBox.hidden = true;
    ytBar.style.width = "0%";
  }

  listen("yt_dlp_progress", (event) => {
    const { percent, label } = event.payload || {};
    showProgress(label || "downloading…", percent);
  });

  // ── Probe a local file path and push to source-store ────────────────────
  async function commitLocalPath(path) {
    setSourcePath(path);
    if (!path) return;
    try {
      const p = await invoke("media_probe", { path });
      setProbe({
        durationMs:    p.duration_ms,
        width:         p.width,
        height:        p.height,
        frameRate:     p.frame_rate,
        audioChannels: p.audio_channels,
      });
      meta.dataset.error = "";
      try {
        const cached = await invoke("get_cached_transcript", { path });
        if (cached) setTranscript(cached);
      } catch (_) { /* no cache */ }
    } catch (err) {
      setProbe(null);
      const msg = String(err);
      if (/file not found|No such file|FileNotFound/i.test(msg)) {
        meta.dataset.error = msg;
        meta.textContent   = msg;
      } else {
        meta.dataset.error = "";
      }
    }
  }

  // ── Handle a YouTube URL ────────────────────────────────────────────────
  async function commitYouTubeUrl(url) {
    input.disabled = true;
    showProgress("resolving video ID…", null);
    meta.dataset.error = "";
    meta.textContent   = "downloading from YouTube…";

    try {
      const localPath = await invoke("yt_dlp_download", { url });
      input.value = localPath;
      hideProgress();
      await commitLocalPath(localPath);
    } catch (err) {
      hideProgress();
      meta.dataset.error = String(err);
      meta.textContent   = `download failed: ${err}`;
    } finally {
      input.disabled = false;
    }
  }

  // ── Entry point ──────────────────────────────────────────────────────────
  async function commitSource() {
    const val = input.value.trim();
    if (!val) return;
    if (YT_URL_RE.test(val)) {
      await commitYouTubeUrl(val);
    } else {
      await commitLocalPath(val);
    }
  }

  input.addEventListener("change", commitSource);
  input.addEventListener("blur",   commitSource);

  // ── Drag-and-drop via Tauri v2 webview API ───────────────────────────────
  const webview = window.__TAURI__.webview?.getCurrentWebview?.();
  if (webview?.onDragDropEvent) {
    webview.onDragDropEvent((event) => {
      const { type } = event.payload;
      if (type === "enter" || type === "over") {
        pickerArea.classList.add("drag-active");
      } else if (type === "leave") {
        pickerArea.classList.remove("drag-active");
      } else if (type === "drop") {
        pickerArea.classList.remove("drag-active");
        const paths = event.payload.paths ?? [];
        if (!paths.length) return;
        const picked = paths.find(isMediaPath) ?? paths[0];
        input.value = picked;
        commitSource();
      }
    });
  }

  // ── File browser button (uses window.__TAURI__.dialog — no bundler) ─────
  browseBtn.addEventListener("click", async () => {
    try {
      const selected = await window.__TAURI__.dialog.open({
        multiple: false,
        title: "Select a video or audio file",
        filters: [
          { name: "Media files", extensions: MEDIA_EXTENSIONS },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (selected) {
        input.value = typeof selected === "string" ? selected : selected.path;
        commitSource();
      }
    } catch (err) {
      console.error("file dialog error:", err);
    }
  });

  // ── Clear cache ──────────────────────────────────────────────────────────
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

  // ── Global AI config wiring ──────────────────────────────────────────
  const aiModeSel  = document.getElementById("ai-mode-global");
  const aiLangInput = document.getElementById("ai-language-global");

  invoke("check_platform").then(({ is_apple_silicon }) => {
    if (!is_apple_silicon) {
      const mlxOpt = aiModeSel.querySelector('option[value="local"]');
      if (mlxOpt) mlxOpt.disabled = true;
      aiModeSel.value = "cloud";
      setAiConfig({ mode: "cloud" });
    }
  }).catch(() => {});

  function syncAiConfig() {
    setAiConfig({
      mode: aiModeSel.value,
      language: aiLangInput.value.trim() || "Vietnamese",
    });
  }

  aiModeSel.addEventListener("change", syncAiConfig);
  aiLangInput.addEventListener("change", syncAiConfig);

  // ── Status dots on sidebar tabs ─────────────────────────────────────
  const featureTabs = document.querySelectorAll(".feature-item[data-feature]");
  const transcriptFeatures = new Set([
    "translate", "summary", "chapters", "youtube-pack", "viral-clips", "blog-article",
  ]);

  subscribe((state) => {
    const hasTranscript = !!(state.transcript?.segments?.length);
    featureTabs.forEach((tab) => {
      const feat = tab.dataset.feature;
      if (transcriptFeatures.has(feat)) {
        tab.classList.toggle("locked", !hasTranscript && !!state.path);
        tab.classList.toggle("ready", hasTranscript);
      }
    });
  });

  // ── Sidebar meta line ─────────────────────────────────────────────
  subscribe((state) => {
    if (!state.path) {
      meta.textContent   = "no file selected";
      meta.dataset.error = "";
      return;
    }
    if (meta.dataset.error) {
      meta.textContent = meta.dataset.error;
      return;
    }
    const probeBit = state.probe
      ? ` · ${fmtDuration(state.probe.durationMs)}${state.probe.width ? ` · ${state.probe.width}×${state.probe.height}` : ""}`
      : "";
    const tBit = state.transcript
      ? ` · transcript (${state.transcript.segments.length} segs)`
      : "";
    meta.textContent = `${basename(state.path)}${probeBit}${tBit}`;
  });
}

function isMediaPath(p) {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  return [
    "mp4","mov","mkv","avi","webm","flv","m4v","ts",
    "mp3","wav","m4a","aac","ogg","flac","wma",
  ].includes(ext);
}

function basename(p) {
  if (!p) return "";
  const sep   = p.includes("/") ? "/" : "\\";
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
