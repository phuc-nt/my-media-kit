// Tiny formatting + DOM helpers shared across feature views.

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

// Derive a sibling path next to the source file.
// source="/a/b/clip.mp4", suffix=".transcript.srt" → "/a/b/clip.transcript.srt"
export function deriveSiblingPath(sourcePath, suffix) {
  const slashIdx = Math.max(
    sourcePath.lastIndexOf("/"),
    sourcePath.lastIndexOf("\\"),
  );
  const dir = slashIdx >= 0 ? sourcePath.slice(0, slashIdx) : "";
  const name = slashIdx >= 0 ? sourcePath.slice(slashIdx + 1) : sourcePath;
  const dotIdx = name.lastIndexOf(".");
  const stem = dotIdx > 0 ? name.slice(0, dotIdx) : name;
  const sep = sourcePath.includes("\\") ? "\\" : "/";
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
