// Summary view.
//
// Default summary is generated automatically during Transcribe and shown
// inline there. This tab is for re-summarizing with a different style or
// custom instructions. The button is disabled until a transcript exists.

import { getSource, getAiConfig, setSummary, subscribe, markOutputDone } from "../source-store.js";
import {
  deriveOutputPath,
  escapeHtml,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
  showToast,
} from "../util.js";

const { invoke } = window.__TAURI__.core;

export function initSummaryView() {
  const results   = document.getElementById("summary-results");
  const status    = document.getElementById("summary-status");
  const btn       = document.getElementById("btn-summary");
  const promptBox = document.getElementById("summary-custom-prompt");

  function renderCurrent(summary) {
    if (!summary?.text) {
      results.innerHTML = "";
      return;
    }
    results.innerHTML = `
      <p class="hint">language: <code>${escapeHtml(summary.language)}</code>${summary.custom ? " · custom" : " · auto-generated"}</p>
      <pre class="summary-text">${escapeHtml(summary.text)}</pre>
    `;
  }

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const customInstruction = promptBox.value.trim() || null;
    btn.disabled = true;

    const { provider, model, language } = getAiConfig();
    setStatus(status, "re-summarizing…", "running");

    try {
      const out = await invoke("content_summary", {
        request: {
          provider, model,
          segments: source.transcript.segments,
          language,
          customInstruction,
        },
      });
      const tagged = { ...out, custom: !!customInstruction };
      setSummary(tagged);
      renderCurrent(tagged);

      const target = deriveOutputPath(source.outputDir, "summary.md");
      if (target) {
        try {
          await invoke("save_text_file", { path: target, content: out.text ?? "" });
          markOutputDone("summary");
          showToast(`saved → ${target}`, "ok");
        } catch (_) {}
      }
      setStatus(status, "done", "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      btn.disabled = false;
    }
  });

  subscribe(async (state) => {
    const { mode } = getAiConfig();
    const aiOk = mode === "cloud" || state.aiReady === true;
    const hasTranscript = !!(state.path && state.transcript);
    btn.disabled       = !hasTranscript || !aiOk;
    promptBox.disabled = !hasTranscript;

    if (!hasTranscript) {
      results.innerHTML = state.path
        ? `<p class="hint">run <strong>Transcribe</strong> first — Summary needs a transcript</p>`
        : `<p class="hint">select a source file, then transcribe it</p>`;
      setStatus(status, "transcribe first");
      return;
    }

    // Show current summary (auto-generated or last re-summary).
    if (state.summary?.text) {
      renderCurrent(state.summary);
      setStatus(status, state.summary.custom ? "custom summary" : "auto-generated", "ok");
      return;
    }

    // Auto-load cached summary.md from disk on app restart / new session.
    if (state.outputStatus?.summary) {
      try {
        const text = await invoke("read_output_file", { sourcePath: state.path, filename: "summary.md" });
        if (text) {
          const cached = { text, language: getAiConfig().language || "English", custom: false };
          setSummary(cached);
          return; // setSummary triggers notify → this subscribe re-runs and renders
        }
      } catch (_) {}
    }
    results.innerHTML = `<p class="hint">summary will be generated during Transcribe — or click Re-summarize to run now</p>`;
    setStatus(status, "ready");
  });
}
