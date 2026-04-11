// Translate view. Reads the cached transcript from source-store, calls
// content_translate, renders originals + translations side-by-side.

import { getSource, subscribe } from "../source-store.js";
import {
  escapeHtml,
  formatMs,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
} from "../util.js";

const { invoke } = window.__TAURI__.core;

export function initTranslateView() {
  const results = document.getElementById("translate-results");
  const status = document.getElementById("translate-status");
  const btn = document.getElementById("btn-translate");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("translate-provider").value;
    const model = document.getElementById("translate-model").value.trim();
    const target = document.getElementById("translate-target").value.trim() || "vi";

    setStatus(status, "translating…");
    results.innerHTML = "";

    try {
      const out = await invoke("content_translate", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          sourceLanguage: source.transcript.language ?? null,
          targetLanguage: target,
        },
      });
      renderTranslation(out, source.transcript.segments, results);
      const tag = out.skipped ? "skipped (source already matches target)" : `translated to ${out.target_language}`;
      setStatus(status, tag, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    }
  });

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then translate</p>`;
    }
  });
}

function renderTranslation(out, originals, container) {
  const rows = originals
    .map((orig, i) => {
      const tr = out.segments?.[i];
      const originalText = escapeHtml(orig.text);
      const translatedText = tr
        ? escapeHtml(tr.text)
        : `<span class="hint">—</span>`;
      return `
        <tr>
          <td class="ts">${formatMs(orig.start_ms)}</td>
          <td>${originalText}</td>
          <td>${translatedText}</td>
        </tr>
      `;
    })
    .join("");

  container.innerHTML = `
    <p class="hint">
      source: <code>${escapeHtml(out.source_language ?? "?")}</code> → target: <code>${escapeHtml(out.target_language)}</code>
      ${out.skipped ? " · <strong>skipped</strong>" : ""}
    </p>
    <table>
      <thead><tr><th>Time</th><th>Original</th><th>Translated</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}
