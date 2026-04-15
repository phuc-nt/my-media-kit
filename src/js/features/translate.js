// Translate view. Reads the cached transcript from source-store, calls
// content_translate, renders originals + translations side-by-side.

import { getSource, subscribe } from "../source-store.js";
import {
  deriveSiblingPath,
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
import { wireProviderModelSync } from "./provider-model-defaults.js";

const { invoke } = window.__TAURI__.core;

export function initTranslateView() {
  wireProviderModelSync("translate-provider", "translate-model");
  const results = document.getElementById("translate-results");
  const status = document.getElementById("translate-status");
  const btn = document.getElementById("btn-translate");
  const btnSaveSrt = document.getElementById("btn-translate-save");
  const btnSaveTxt = document.getElementById("btn-translate-save-txt");

  let lastResult = null;

  function setSaveButtons(enabled) {
    btnSaveSrt.disabled = !enabled;
    btnSaveTxt.disabled = !enabled;
  }

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("translate-provider").value;
    const model = document.getElementById("translate-model").value.trim();
    const target = document.getElementById("translate-target").value.trim() || "vi";

    setStatus(status, "translating…", "running");
    btn.disabled = true;
    setSaveButtons(false);
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
      lastResult = out;
      renderTranslation(out, source.transcript.segments, results);
      setSaveButtons(out?.segments?.length > 0);
      const tag = out.skipped ? "skipped (source already matches target)" : `translated to ${out.target_language}`;
      setStatus(status, tag, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      btn.disabled = false;
    }
  });

  async function save(format) {
    if (!lastResult?.segments?.length) return;
    const source = getSource();
    if (!source?.path) return;
    const isSrt = format === "srt";
    const lang = (lastResult.target_language || "vi").replace(/[^a-z0-9-]/gi, "");
    const suffix = isSrt ? `.${lang}.srt` : `.${lang}.txt`;
    const target = deriveSiblingPath(source.path, suffix);
    const content = isSrt
      ? segmentsToSrt(lastResult.segments)
      : segmentsToPlainText(lastResult.segments);
    try {
      const written = await invoke("save_text_file", { path: target, content });
      showToast(`saved → ${written}`, "ok");
    } catch (e) {
      console.error(e);
      showToast(`save failed: ${e}`, "err");
    }
  }

  btnSaveSrt.addEventListener("click", () => save("srt"));
  btnSaveTxt.addEventListener("click", () => save("txt"));

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then translate</p>`;
      lastResult = null;
      setSaveButtons(false);
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
