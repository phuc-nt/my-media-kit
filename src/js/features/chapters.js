// Chapters view. Runs content_chapters, renders the list, has a one-click
// copy button that formats the output for a YouTube video description.

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

let lastChapters = null;

export function initChaptersView() {
  const results = document.getElementById("chapters-results");
  const status = document.getElementById("chapters-status");
  const btn = document.getElementById("btn-chapters");
  const copyBtn = document.getElementById("btn-chapters-copy");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    btn.disabled = true;
    lastChapters = null;

    const { provider, model, language } = getAiConfig();
    setStatus(status, "generating chapters…", "running");
    results.innerHTML = "";

    try {
      const out = await invoke("content_chapters", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
          summaryHint: getSummary()?.text ?? null,
        },
      });
      lastChapters = out;
      renderChapters(out, results);
      const target = deriveOutputPath(source.outputDir, "chapters.json");
      if (target) {
        try {
          await invoke("save_text_file", { path: target, content: JSON.stringify(out, null, 2) });
          markOutputDone("chapters");
          showToast(`saved → ${target}`, "ok");
        } catch (_) {}
      }
      setStatus(status, `${out.chapters.length} chapters`, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      btn.disabled = false;
    }
  });

  copyBtn.addEventListener("click", async () => {
    if (!lastChapters) {
      setStatus(status, "nothing to copy yet", "err");
      return;
    }
    const text = formatYouTube(lastChapters.chapters);
    try {
      await navigator.clipboard.writeText(text);
      setStatus(status, "copied to clipboard", "ok");
    } catch (e) {
      setStatus(status, "copy failed: " + e, "err");
    }
  });

  subscribe(async (state) => {
    const { mode } = getAiConfig();
    const aiOk = mode === "cloud" || state.aiReady === true;
    const hasTranscript = !!(state.path && state.transcript);
    btn.disabled = !hasTranscript || !aiOk;
    copyBtn.disabled = !hasTranscript;
    if (!hasTranscript) {
      results.innerHTML = state.path
        ? `<p class="hint">run <strong>Transcribe</strong> first — Chapters needs a transcript</p>`
        : `<p class="hint">select a source file, then transcribe it</p>`;
      lastChapters = null;
      return;
    }
    // Auto-load cached chapters from disk if file exists.
    if (!lastChapters && state.outputStatus?.chapters) {
      try {
        const raw = await invoke("read_output_file", { sourcePath: state.path, filename: "chapters.json" });
        if (raw) {
          lastChapters = JSON.parse(raw);
          renderChapters(lastChapters, results);
          setStatus(status, `${lastChapters.chapters.length} chapters (cached)`, "ok");
        }
      } catch (_) {}
    }
  });
}

function renderChapters(out, container) {
  const rows = out.chapters
    .map(
      (c) =>
        `<tr><td>${formatMs(c.timestamp_ms)}</td><td>${escapeHtml(c.title)}</td></tr>`,
    )
    .join("");
  container.innerHTML = `
    <p class="hint">language: <code>${escapeHtml(out.language)}</code></p>
    <table>
      <thead><tr><th>Time</th><th>Title</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}

function formatYouTube(chapters) {
  return chapters
    .map((c) => `${formatMs(c.timestamp_ms)} ${c.title}`)
    .join("\n");
}
