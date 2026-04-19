// Translate view. Reads the cached transcript from source-store, calls
// content_translate, renders originals + translations side-by-side.

import { getSource, getAiConfig, getSummary, subscribe, markOutputDone } from "../source-store.js";
import {
  deriveOutputPath,
  escapeHtml,
  formatMs,
  renderErrorBox,
  requireSource,
  requireTranscript,
  segmentsToPlainText,
  segmentsToSrt,
  setStatus,
  showToast,
} from "../util.js";

const { invoke } = window.__TAURI__.core;
const { listen }  = window.__TAURI__.event;

export function initTranslateView() {
  const results = document.getElementById("translate-results");
  const status = document.getElementById("translate-status");
  const btn = document.getElementById("btn-translate");

  // ── Progress bar (created inline, same pattern as YT download) ──────
  const progressBox = document.createElement("div");
  progressBox.className = "progress-box";
  progressBox.hidden = true;
  progressBox.innerHTML = `
    <div class="progress-head">
      <span id="translate-progress-label">translating…</span>
      <span id="translate-progress-value">0%</span>
    </div>
    <div class="progress-track"><div id="translate-progress-bar" class="progress-bar indeterminate"></div></div>
  `;
  // Insert right after the action buttons row (.actions div)
  (btn.closest(".actions") ?? btn.parentElement).insertAdjacentElement("afterend", progressBox);

  const progBar   = progressBox.querySelector("#translate-progress-bar");
  const progLabel = progressBox.querySelector("#translate-progress-label");
  const progValue = progressBox.querySelector("#translate-progress-value");

  function showProgress(batch, total) {
    progressBox.hidden = false;
    if (total > 0) {
      const pct = ((batch - 1) / total) * 100;
      progBar.className = "progress-bar";
      progBar.style.width = `${pct.toFixed(1)}%`;
      progLabel.textContent = `translating batch ${batch} / ${total}…`;
      progValue.textContent = `${pct.toFixed(0)}%`;
    } else {
      progBar.className = "progress-bar indeterminate";
      progValue.textContent = "…";
    }
  }
  function hideProgress() {
    progressBox.hidden = true;
    progBar.style.width = "0%";
    progBar.className = "progress-bar indeterminate";
  }

  listen("translate_progress", (event) => {
    const { batch, total } = event.payload || {};
    showProgress(batch, total);
  });

  let lastResult = null;

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    btn.disabled = true;
    showProgress(0, 0);

    const { provider, model, language } = getAiConfig();
    const target = language || "Vietnamese";

    setStatus(status, "translating…", "running");
    results.innerHTML = "";

    try {
      const summary = getSummary();
      const out = await invoke("content_translate", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          sourceLanguage: source.transcript.language ?? null,
          targetLanguage: target,
          summaryHint: summary?.text ?? null,
        },
      });
      lastResult = out;
      renderTranslation(out, source.transcript.segments, results);
      const tag = out.skipped ? "skipped (source already matches target)" : `translated to ${out.target_language}`;
      setStatus(status, tag, "ok");

      // Auto-save SRT and TXT.
      if (out?.segments?.length) {
        const lang = (out.target_language || "vi").replace(/[^a-z0-9-]/gi, "");
        const srtPath = deriveOutputPath(source.outputDir, `translate.${lang}.srt`);
        const txtPath = deriveOutputPath(source.outputDir, `translate.${lang}.txt`);
        const srtContent = segmentsToSrt(out.segments);
        const txtContent = segmentsToPlainText(out.segments);
        await Promise.all([
          srtPath ? invoke("save_text_file", { path: srtPath, content: srtContent }).catch(() => {}) : Promise.resolve(),
          txtPath ? invoke("save_text_file", { path: txtPath, content: txtContent }).catch(() => {}) : Promise.resolve(),
        ]);
        markOutputDone("translate");
        showToast(`saved → ${srtPath}`, "ok");
      }
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      btn.disabled = false;
      hideProgress();
    }
  });

  subscribe((state) => {
    const { mode } = getAiConfig();
    const aiOk = mode === "cloud" || state.aiReady === true;
    const hasTranscript = !!(state.path && state.transcript);
    btn.disabled = !hasTranscript || !aiOk;
    if (!hasTranscript) {
      results.innerHTML = state.path
        ? `<p class="hint">run <strong>Transcribe</strong> first — Translate needs a transcript</p>`
        : `<p class="hint">select a source file, then transcribe it</p>`;
      lastResult = null;
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
