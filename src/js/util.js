// Tiny formatting + DOM helpers shared across feature views.

const { invoke: _invoke } = window.__TAURI__.core;
const { listen: _listen } = window.__TAURI__.event;

// ── MLX server management ────────────────────────────────────────────────────
// Ensures mlx_lm.server is running before any LLM feature. Shows status in
// the provided statusEl. Returns true on success, false on failure.
export async function ensureMlxServer(statusEl) {
  try {
    setStatus(statusEl, "starting AI engine…", "running");
    const unlisten = await _listen("mlx_server_status", (event) => {
      const { status, message } = event.payload || {};
      if (status === "downloading") {
        setStatus(statusEl, message || "downloading AI model (~3 GB, first run)…", "running");
      } else if (status === "starting") {
        setStatus(statusEl, message || "starting AI engine…", "running");
      }
    });
    await _invoke("ensure_mlx_lm_server");
    unlisten();
    return true;
  } catch (e) {
    setStatus(statusEl, `AI engine error: ${e}`, "err");
    return false;
  }
}

// Ensures AI engine is ready for the given mode. Pass getAiConfig().mode.
// Returns true for cloud (no server needed) or when MLX server is up.
export async function ensureAiReady(mode, statusEl) {
  if (mode !== "local") return true;
  return ensureMlxServer(statusEl);
}

export function formatMs(ms) {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${pad(m)}:${pad(s)}`;
  return `${m}:${pad(s)}`;
}

function pad(n) {
  return String(n).padStart(2, "0");
}

export function msToYouTubeTimestamp(ms) {
  return formatMs(ms);
}

export function setStatus(el, text, kind = "") {
  if (!el) return;
  el.textContent = text;
  el.className = "status" + (kind ? " " + kind : "");
}

export function requireSource(source, statusEl) {
  if (!source?.path) {
    setStatus(statusEl, "pick a source video first", "err");
    return false;
  }
  return true;
}

export function requireTranscript(transcript, statusEl) {
  if (!transcript || !transcript.segments?.length) {
    setStatus(
      statusEl,
      "no transcript yet — run the Transcribe tab first",
      "err",
    );
    return false;
  }
  return true;
}

export function renderErrorBox(container, message) {
  container.innerHTML = `<p class="hint" style="color:var(--danger)">${escapeHtml(message)}</p>`;
}

export function escapeHtml(str) {
  return String(str)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// `HH:MM:SS,mmm` — SRT timestamp format.
export function msToSrtTimestamp(ms) {
  const total = Math.max(0, ms);
  const h = Math.floor(total / 3_600_000);
  const m = Math.floor((total % 3_600_000) / 60_000);
  const s = Math.floor((total % 60_000) / 1000);
  const millis = total % 1000;
  return `${pad(h)}:${pad(m)}:${pad(s)},${String(millis).padStart(3, "0")}`;
}

export function segmentsToSrt(segments) {
  return segments
    .map((seg, i) => {
      const start = msToSrtTimestamp(seg.start_ms);
      const end = msToSrtTimestamp(seg.end_ms);
      const text = String(seg.text || "").trim();
      return `${i + 1}\n${start} --> ${end}\n${text}\n`;
    })
    .join("\n");
}

export function segmentsToPlainText(segments) {
  return segments.map((s) => String(s.text || "").trim()).join("\n");
}

// Build a path inside the source's `{stem}_output/` folder.
// Requires outputDir from source-store (set by ensure_output_dir).
export function deriveOutputPath(outputDir, filename) {
  if (!outputDir) return filename;
  const sep = outputDir.includes("\\") ? "\\" : "/";
  return `${outputDir}${sep}${filename}`;
}

// DEPRECATED — use deriveOutputPath instead.
// Derive an output path for a given source file.
// For local files: saves next to source (most convenient).
// For cache/temp files (e.g. YouTube downloads): saves to ~/Downloads/MyMediaKit/.
export function deriveSiblingPath(sourcePath, suffix) {
  const sep = sourcePath.includes("\\") ? "\\" : "/";
  const slashIdx = Math.max(
    sourcePath.lastIndexOf("/"),
    sourcePath.lastIndexOf("\\"),
  );
  const dir = slashIdx >= 0 ? sourcePath.slice(0, slashIdx) : "";
  const name = slashIdx >= 0 ? sourcePath.slice(slashIdx + 1) : sourcePath;
  const dotIdx = name.lastIndexOf(".");
  const stem = dotIdx > 0 ? name.slice(0, dotIdx) : name;

  // Detect cache/temp dirs — redirect output to ~/Downloads/MyMediaKit/
  const isCacheDir = /[/\\](Caches|cache|tmp|temp)[/\\]/i.test(sourcePath)
    || /[/\\]AppData[/\\]Local[/\\]/i.test(sourcePath);

  if (isCacheDir) {
    const home = sourcePath.includes("\\")
      ? sourcePath.split("\\AppData")[0] || sourcePath.split("\\")[0]
      : (sourcePath.match(/^(\/Users\/[^/]+)/)?.[1] ?? "");
    if (home) {
      const dlDir = `${home}${sep}Downloads${sep}MyMediaKit`;
      return `${dlDir}${sep}${stem}${suffix}`;
    }
  }

  return dir ? `${dir}${sep}${stem}${suffix}` : `${stem}${suffix}`;
}

let toastEl = null;
let toastTimer = 0;
export function showToast(message, kind = "ok", ms = 3500) {
  if (!toastEl) {
    toastEl = document.createElement("div");
    toastEl.className = "toast";
    document.body.appendChild(toastEl);
  }
  toastEl.textContent = message;
  toastEl.className = "toast show " + kind;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    if (toastEl) toastEl.className = "toast " + kind;
  }, ms);
}
