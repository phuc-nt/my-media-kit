// Chapters view. Runs content_chapters, renders the list, has a one-click
// copy button that formats the output for a YouTube video description.

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

let lastChapters = null;

export function initChaptersView() {
  wireProviderModelSync("chapters-provider", "chapters-model");
  const results = document.getElementById("chapters-results");
  const status = document.getElementById("chapters-status");
  const btn = document.getElementById("btn-chapters");
  const copyBtn = document.getElementById("btn-chapters-copy");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("chapters-provider").value;
    const model = document.getElementById("chapters-model").value.trim();
    const language = document.getElementById("chapters-language").value.trim() || "English";

    setStatus(status, "generating chapters…");
    results.innerHTML = "";
    lastChapters = null;

    try {
      const out = await invoke("content_chapters", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
        },
      });
      lastChapters = out;
      renderChapters(out, results);
      setStatus(status, `${out.chapters.length} chapters`, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
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

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then generate chapters</p>`;
      lastChapters = null;
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
