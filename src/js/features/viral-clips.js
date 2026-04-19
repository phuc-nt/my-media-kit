// Viral Clips — find the best short-form moments for Shorts/Reels/TikTok.

import { getSource, getAiConfig, getSummary, subscribe, markOutputDone } from "../source-store.js";
import {
  deriveOutputPath,
  escapeHtml,
  formatMs,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
  showToast,
} from "../util.js";

const { invoke } = window.__TAURI__.core;

export function initViralClipsView() {
  const results = document.getElementById("viral-results");
  const status = document.getElementById("viral-status");
  const btn = document.getElementById("btn-viral");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    btn.disabled = true;

    const { provider, model, language } = getAiConfig();
    setStatus(status, "scanning for viral moments…", "running");
    results.innerHTML = "";

    try {
      const out = await invoke("content_viral_clips", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
          summaryHint: getSummary()?.text ?? null,
        },
      });
      renderClips(out, results);
      const target = deriveOutputPath(source.outputDir, "viral-clips.json");
      if (target) {
        try {
          await invoke("save_text_file", { path: target, content: JSON.stringify(out, null, 2) });
          markOutputDone("viral-clips");
          showToast(`saved → ${target}`, "ok");
        } catch (_) {}
      }
      setStatus(status, `${out.clips.length} clips found`, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      btn.disabled = false;
    }
  });

  let lastClips = null;
  subscribe(async (state) => {
    const { mode } = getAiConfig();
    const aiOk = mode === "cloud" || state.aiReady === true;
    const hasTranscript = !!(state.path && state.transcript);
    btn.disabled = !hasTranscript || !aiOk;
    if (!hasTranscript) {
      results.innerHTML = state.path
        ? `<p class="hint">run <strong>Transcribe</strong> first — Viral Clips needs a transcript</p>`
        : `<p class="hint">select a source file, then transcribe it</p>`;
      lastClips = null;
      return;
    }
    if (!lastClips && state.outputStatus?.["viral-clips"]) {
      try {
        const raw = await invoke("read_output_file", { sourcePath: state.path, filename: "viral-clips.json" });
        if (raw) {
          lastClips = JSON.parse(raw);
          renderClips(lastClips, results);
          setStatus(status, `${lastClips.clips.length} clips (cached)`, "ok");
        }
      } catch (_) {}
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
