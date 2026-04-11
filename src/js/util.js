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
