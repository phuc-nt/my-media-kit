// Viral Clips — find the best short-form moments for Shorts/Reels/TikTok.

import { getSource, subscribe } from "../source-store.js";
import {
  escapeHtml,
  formatMs,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
} from "../util.js";
import { wireProviderModelSync } from "./provider-model-defaults.js";

const { invoke } = window.__TAURI__.core;

export function initViralClipsView() {
  wireProviderModelSync("viral-provider", "viral-model");
  const results = document.getElementById("viral-results");
  const status = document.getElementById("viral-status");
  const btn = document.getElementById("btn-viral");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("viral-provider").value;
    const model = document.getElementById("viral-model").value.trim();
    const language = document.getElementById("viral-language").value.trim() || "Vietnamese";

    setStatus(status, "scanning for viral moments…");
    results.innerHTML = "";

    try {
      const out = await invoke("content_viral_clips", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
        },
      });
      renderClips(out, results);
      setStatus(status, `${out.clips.length} clips found`, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    }
  });

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then find viral clips</p>`;
    }
  });
}

function renderClips(out, container) {
  if (!out.clips.length) {
    container.innerHTML = `<p class="hint">no clips found</p>`;
    return;
  }
  const cards = out.clips
    .map((c, i) => {
      const duration = Math.round((c.end_ms - c.start_ms) / 1000);
      return `
      <div class="clip-card">
        <div class="clip-header">
          <strong>#${i + 1}</strong>
          <span class="clip-time">${formatMs(c.start_ms)} – ${formatMs(c.end_ms)} (${duration}s)</span>
        </div>
        <p class="clip-hook"><em>${escapeHtml(c.hook)}</em></p>
        <p class="clip-caption">${escapeHtml(c.caption)}</p>
      </div>`;
    })
    .join("");
  container.innerHTML = cards;
}
