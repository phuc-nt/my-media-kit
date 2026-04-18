// Source Manager — top-half of the horizontal split.
//
// Handles: file input, browse button, drag-drop, AI config wiring,
// output folder creation, status badge rendering.

import {
  getSource, setSourcePath, setTranscript, setProbe,
  subscribe, setAiConfig, setOutputDir, setOutputStatus,
} from "./source-store.js";

const { invoke } = window.__TAURI__.core;
const { listen }  = window.__TAURI__.event;

const YT_URL_RE = /^https?:\/\/(www\.)?(youtube\.com\/watch|youtu\.be\/)/;

const MEDIA_EXTENSIONS = [
  "mp4","mov","mkv","avi","webm","flv","m4v","ts",
  "mp3","wav","m4a","aac","ogg","flac","wma",
];

// Status badge definitions: key → label shown in UI.
const BADGE_KEYS = [
  ["transcript", "Transcript"],
  ["translate", "Translate"],
  ["summary", "Summary"],
  ["chapters", "Chapters"],
  ["youtube-pack", "YT Pack"],
  ["viral-clips", "Viral Clips"],
  ["blog", "Blog"],
  ["clean", "Clean SRT"],
];

export function initSourceManager() {
  const input     = document.getElementById("source-path");
  const meta      = document.getElementById("source-meta");
  const browseBtn = document.getElementById("btn-source-browse");
  const clearBtn  = document.getElementById("btn-source-clear");
  const smEl      = document.getElementById("source-manager");
  const badgesEl  = document.getElementById("status-badges");

  // ── YouTube download progress ────────────────────────────────────────
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
  smEl.querySelector(".sm-toolbar").appendChild(progressBox);

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
  function hideProgress() { progressBox.hidden = true; ytBar.style.width = "0%"; }

  listen("yt_dlp_progress", (event) => {
    const { percent, label } = event.payload || {};
    showProgress(label || "downloading…", percent);
  });

  // ── Commit source path (probe + output dir + status scan) ─────────
  async function commitLocalPath(path) {
    setSourcePath(path);
    if (!path) return;
    try {
      const p = await invoke("media_probe", { path });
      setProbe({
        durationMs: p.duration_ms, width: p.width, height: p.height,
        frameRate: p.frame_rate, audioChannels: p.audio_channels,
      });
      meta.dataset.error = "";

      // Create output dir + scan existing outputs.
      const outDir = await invoke("ensure_output_dir", { sourcePath: path });
      setOutputDir(outDir);
      const status = await invoke("scan_output_status", { sourcePath: path });
      setOutputStatus(status);

      // Auto-load transcript: try in-memory cache first, then SRT from output folder.
      try {
        const cached = await invoke("get_cached_transcript", { path });
        if (cached) {
          setTranscript(cached);
        } else if (status.transcript) {
          // transcript.srt exists on disk — parse and load it.
          const segments = await invoke("load_transcript_from_output", { sourcePath: path });
          if (segments) setTranscript({ language: null, segments });
        }
      } catch (_) {}
    } catch (err) {
      setProbe(null);
      const msg = String(err);
      if (/file not found|No such file|FileNotFound/i.test(msg)) {
        meta.dataset.error = msg;
        meta.textContent = msg;
      } else {
        meta.dataset.error = "";
      }
    }
  }

  async function commitYouTubeUrl(url) {
    input.disabled = true;
    showProgress("resolving video ID…", null);
    meta.dataset.error = ""; meta.textContent = "downloading from YouTube…";
    try {
      const localPath = await invoke("yt_dlp_download", { url });
      input.value = localPath;
      hideProgress();
      await commitLocalPath(localPath);
    } catch (err) {
      hideProgress();
      meta.dataset.error = String(err);
      meta.textContent = `download failed: ${err}`;
    } finally { input.disabled = false; }
  }

  async function commitSource() {
    const val = input.value.trim();
    if (!val) return;
    YT_URL_RE.test(val) ? await commitYouTubeUrl(val) : await commitLocalPath(val);
  }

  input.addEventListener("change", commitSource);
  input.addEventListener("blur", commitSource);

  // ── Drag-and-drop (Tauri v2 webview API) ─────────────────────────────
  const webview = window.__TAURI__.webview?.getCurrentWebview?.();
  if (webview?.onDragDropEvent) {
    webview.onDragDropEvent((event) => {
      const { type } = event.payload;
      if (type === "enter" || type === "over") {
        smEl.classList.add("drag-active");
      } else if (type === "leave") {
        smEl.classList.remove("drag-active");
      } else if (type === "drop") {
        smEl.classList.remove("drag-active");
        const paths = event.payload.paths ?? [];
        if (!paths.length) return;
        const picked = paths.find(isMediaPath) ?? paths[0];
        input.value = picked;
        commitSource();
      }
    });
  }

  // ── Browse button ────────────────────────────────────────────────────
  browseBtn.addEventListener("click", async () => {
    try {
      const selected = await window.__TAURI__.dialog.open({
        multiple: false, title: "Select a video or audio file",
        filters: [
          { name: "Media files", extensions: MEDIA_EXTENSIONS },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (selected) {
        input.value = typeof selected === "string" ? selected : selected.path;
        commitSource();
      }
    } catch (err) { console.error("file dialog error:", err); }
  });

  // ── Clear cache ──────────────────────────────────────────────────────
  clearBtn.addEventListener("click", async () => {
    const { path } = getSource();
    try {
      await invoke("clear_cache", { path: path || null });
      setTranscript(null);
      meta.textContent = path ? `cleared cache for ${basename(path)}` : "cache cleared";
    } catch (e) { meta.textContent = "clear failed: " + e; }
  });

  // ── AI config ────────────────────────────────────────────────────────
  const aiModeSel   = document.getElementById("ai-mode-global");
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
    setAiConfig({ mode: aiModeSel.value, language: aiLangInput.value.trim() || "Vietnamese" });
  }
  aiModeSel.addEventListener("change", syncAiConfig);
  aiLangInput.addEventListener("change", syncAiConfig);

  // ── Status badges (reactive) ─────────────────────────────────────────
  function renderBadges(outputStatus) {
    badgesEl.innerHTML = BADGE_KEYS.map(([key, label]) => {
      const done = outputStatus[key];
      return `<span class="status-badge ${done ? "done" : ""}"><span class="dot"></span>${label}</span>`;
    }).join("");
  }

  // ── Output files list ───────────────────────────────────────────────
  async function renderOutputFiles(sourcePath, outputDir) {
    if (!sourcePath || !outputDir) {
      outputDirEl.innerHTML = "";
      return;
    }
    try {
      const files = await invoke("list_output_files", { sourcePath });
      if (!files.length) {
        outputDirEl.innerHTML = `<a href="#" class="sm-open-folder" data-dir="${escapeHtml(outputDir)}">Open output folder</a><p class="sm-file-list-empty">No outputs yet</p>`;
      } else {
        const listHtml = files.map(f => {
          const sizeStr = f.size < 1024 ? `${f.size} B`
            : f.size < 1048576 ? `${(f.size / 1024).toFixed(1)} KB`
            : `${(f.size / 1048576).toFixed(1)} MB`;
          return `<div class="sm-output-file"><span class="sm-output-name">${escapeHtml(f.name)}</span><span class="sm-output-size">${sizeStr}</span></div>`;
        }).join("");
        outputDirEl.innerHTML = `<a href="#" class="sm-open-folder" data-dir="${escapeHtml(outputDir)}">📂 Open output folder</a>${listHtml}`;
      }
      // Wire up "open folder" link.
      const link = outputDirEl.querySelector(".sm-open-folder");
      if (link) {
        link.addEventListener("click", (e) => {
          e.preventDefault();
          const dir = link.dataset.dir;
          try { window.__TAURI__.opener.revealItemInDir(dir); } catch (_) {}
        });
      }
    } catch (_) {
      outputDirEl.textContent = "";
    }
  }

  // ── Tab status dots ──────────────────────────────────────────────────
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

  // ── Render source info panel + badges ─────────────────────────────────
  const previewEl   = document.getElementById("source-preview");
  const outputDirEl = document.getElementById("output-dir-info");

  subscribe((state) => {
    // No source loaded — show placeholder.
    if (!state.path) {
      meta.textContent = ""; meta.dataset.error = "";
      previewEl.innerHTML = `<div class="sm-no-source"><span class="sm-icon">🎬</span><p>Drop a video file here or click Browse</p></div>`;
      renderBadges({});
      outputDirEl.innerHTML = "";
      return;
    }

    // Error state.
    if (meta.dataset.error) {
      previewEl.innerHTML = `<div class="sm-no-source"><span class="sm-icon">⚠️</span><p>${meta.dataset.error}</p></div>`;
      return;
    }

    // Source loaded — render rich info.
    const name = basename(state.path);
    const ext = name.split(".").pop()?.toUpperCase() ?? "";
    const p = state.probe;
    let propsHtml = "";
    if (p) {
      const pairs = [
        ["Duration", fmtDuration(p.durationMs)],
        ["Format", ext],
      ];
      if (p.width) pairs.push(["Resolution", `${p.width}×${p.height}`]);
      if (p.frameRate) pairs.push(["FPS", String(p.frameRate)]);
      if (p.audioChannels) pairs.push(["Audio", `${p.audioChannels}ch`]);
      if (state.transcript) pairs.push(["Transcript", `${state.transcript.segments.length} segments`]);
      propsHtml = pairs.map(([l, v]) =>
        `<span class="sm-prop-label">${l}</span><span class="sm-prop-value">${v}</span>`
      ).join("");
    }

    previewEl.innerHTML = `
      <div class="sm-source-loaded">
        <div class="sm-filename">${escapeHtml(name)}</div>
        ${propsHtml ? `<div class="sm-props">${propsHtml}</div>` : ""}
      </div>
    `;

    meta.textContent = state.path;
    renderBadges(state.outputStatus || {});
    renderOutputFiles(state.path, state.outputDir);
  });
}

function escapeHtml(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function isMediaPath(p) {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  return MEDIA_EXTENSIONS.includes(ext);
}

function basename(p) {
  if (!p) return "";
  const sep = p.includes("/") ? "/" : "\\";
  return p.split(sep).pop() || p;
}

function fmtDuration(ms) {
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}h${String(m % 60).padStart(2, "0")}m`;
  return `${m}m${String(s % 60).padStart(2, "0")}s`;
}
