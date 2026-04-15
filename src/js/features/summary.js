// Summary view. Single-call summary with brief / key points / action items
// styles. Reads the cached transcript from source-store.

import { getSource, subscribe } from "../source-store.js";
import {
  escapeHtml,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
} from "../util.js";
import { wireProviderModelSync } from "./provider-model-defaults.js";

const { invoke } = window.__TAURI__.core;

export function initSummaryView() {
  wireProviderModelSync("summary-provider", "summary-model");
  const results = document.getElementById("summary-results");
  const status = document.getElementById("summary-status");
  const btn = document.getElementById("btn-summary");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("summary-provider").value;
    const model = document.getElementById("summary-model").value.trim();
    const style = document.getElementById("summary-style").value;
    const language = document.getElementById("summary-language").value.trim() || "English";

    setStatus(status, "running summary…");
    results.innerHTML = "";

    try {
      const out = await invoke("content_summary", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
          style,
        },
      });
      renderSummary(out, results);
      setStatus(status, "done", "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    }
  });

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then summarize</p>`;
    }
  });
}

function renderSummary(out, container) {
  container.innerHTML = `
    <p class="hint">style: <code>${escapeHtml(String(out.style))}</code> · language: <code>${escapeHtml(out.language)}</code></p>
    <pre class="summary-text">${escapeHtml(out.text ?? "")}</pre>
  `;
}
