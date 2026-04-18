// Transcribe feature view.
//
// Backend (MLX vs OpenAI) is derived from the global AI engine setting.
// Models are fixed: mlx → whisper-large-v3-turbo, cloud → whisper-1.
// No per-feature config — everything comes from the source manager.

import { getSource, getAiConfig, setTranscript, setSummary, subscribe, markOutputDone } from "../source-store.js";
import {
  deriveOutputPath,
  escapeHtml,
  formatMs,
  renderErrorBox,
  requireSource,
  segmentsToPlainText,
  segmentsToSrt,
  setStatus,
  showToast,
} from "../util.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const BACKEND_MODEL = {
  mlx: "mlx-community/whisper-large-v3-turbo",
  openai: "whisper-1",
};

export function initTranscribeView() {
  const results = document.getElementById("transcribe-results");
  const status = document.getElementById("transcribe-status");
  const btnRun = document.getElementById("btn-transcribe");
  const btnForce = document.getElementById("btn-transcribe-refresh");
  const btnSaveClean = document.getElementById("btn-transcribe-save-clean");

  const progressBox = document.getElementById("transcribe-progress");
  const progressBar = document.getElementById("transcribe-progress-bar");
  const progressLabel = document.getElementById("transcribe-progress-label");
  const progressValue = document.getElementById("transcribe-progress-value");

  let currentTranscript = null;
  let running = false;

  function getBackend() {
    const { mode } = getAiConfig();
    return mode === "cloud" ? "openai" : "mlx";
  }

  function setRunning(on, label = "running whisper…") {
    running = on;
    btnRun.disabled = on;
    btnForce.disabled = on;
    if (on) {
      progressBox.hidden = false;
      progressLabel.textContent = label;
      progressBar.className = "progress-bar indeterminate";
      progressValue.textContent = "…";
      setStatus(status, label, "running");
    } else {
      progressBox.hidden = true;
      progressBar.className = "progress-bar";
      progressBar.style.width = "0%";
    }
  }

  function applyTranscript(out) {
    currentTranscript = out;
    setTranscript(out);
    renderSegments(out, results);
    btnSaveClean.disabled = !(out?.segments?.length > 0);
  }

  // MLX streams segment-level progress events.
  listen("mlx_whisper_progress", (event) => {
    if (!running || getBackend() !== "mlx") return;
    const { percent, current_ms, total_ms } = event.payload || {};
    if (typeof percent !== "number") return;
    progressBar.className = "progress-bar";
    progressBar.style.width = `${percent.toFixed(1)}%`;
    progressValue.textContent = `${percent.toFixed(1)}%`;
    const cur = formatMs(current_ms || 0);
    const tot = total_ms ? formatMs(total_ms) : "?";
    progressLabel.textContent = `transcribing… ${cur} / ${tot}`;
  });

  async function run(force) {
    const source = getSource();
    if (!requireSource(source, status)) return;

    results.innerHTML = "";
    setSaveButtons(false);

    const backend = getBackend();
    const model = BACKEND_MODEL[backend];

    let label;
    if (backend === "openai") {
      label = force ? "re-running OpenAI Whisper…" : "running OpenAI Whisper…";
    } else {
      const ready = await invoke("mlx_model_ready").catch(() => true);
      label = ready
        ? (force ? "re-running whisper…" : "running whisper…")
        : "downloading model (~1 GB) + transcribing…";
    }

    setRunning(true, label);

    try {
      let out;
      if (backend === "openai") {
        out = await invoke("openai_whisper_transcribe", {
          path: source.path, language: null, model, force: !!force,
        });
      } else {
        out = await invoke("mlx_whisper_transcribe", {
          path: source.path, language: null, model, force: !!force,
        });
      }
      applyTranscript(out);
      setStatus(status, `${out.segments.length} segments${out.fromCache ? " (cached)" : ""}`, "ok");

      // Auto-save SRT and TXT to output directory.
      const srtPath = deriveOutputPath(source.outputDir, "transcript.srt");
      const txtPath = deriveOutputPath(source.outputDir, "transcript.txt");
      const srtContent = segmentsToSrt(out.segments);
      const txtContent = segmentsToPlainText(out.segments);
      await Promise.all([
        srtPath ? invoke("save_text_file", { path: srtPath, content: srtContent }).catch(() => {}) : Promise.resolve(),
        txtPath ? invoke("save_text_file", { path: txtPath, content: txtContent }).catch(() => {}) : Promise.resolve(),
      ]);
      markOutputDone("transcript");

      // Auto-generate summary in background for use as translation context hint.
      const { provider, model: aiModel, language: aiLang } = getAiConfig();
      invoke("content_summary", {
        request: { provider, model: aiModel, segments: out.segments, language: aiLang || "English" },
      }).then((summary) => setSummary(summary)).catch(() => {});
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      setRunning(false);
    }
  }

  btnRun.addEventListener("click", () => run(false));
  btnForce.addEventListener("click", () => run(true));
  btnSaveClean.addEventListener("click", async () => {
    if (!currentTranscript?.segments?.length) return;
    const source = getSource();
    if (!source?.path) return;
    try {
      const cleaned = await invoke("content_clean_transcript", {
        segments: currentTranscript.segments,
      });
      const target = deriveOutputPath(source.outputDir, "clean.srt");
      const content = segmentsToSrt(cleaned);
      const written = await invoke("save_text_file", { path: target, content });
      markOutputDone("clean");
      showToast(`saved clean → ${written}`, "ok");
    } catch (e) {
      console.error(e);
      showToast(`clean save failed: ${e}`, "err");
    }
  });

  subscribe(async (state) => {
    if (running) return;
    if (!state.path) {
      results.innerHTML = `<p class="hint">no source selected</p>`;
      currentTranscript = null;
      btnSaveClean.disabled = true;
      return;
    }
    if (state.transcript) {
      applyTranscript({ ...state.transcript, fromCache: true });
      return;
    }
    try {
      const hit = await invoke("get_cached_transcript", { path: state.path });
      if (hit) {
        applyTranscript(hit);
        setStatus(status, `${hit.segments.length} segments (cached)`, "ok");
      } else {
        results.innerHTML = `<p class="hint">not transcribed yet — hit Transcribe above</p>`;
        currentTranscript = null;
        btnSaveClean.disabled = true;
      }
    } catch (e) {
      renderErrorBox(results, String(e));
    }
  });
}

function renderSegments(out, container) {
  const lang = out.language
    ? `<p class="hint">language: <code>${escapeHtml(out.language)}</code></p>`
    : "";
  const rows = out.segments
    .map((s) =>
      `<tr><td>${formatMs(s.start_ms)}</td><td>${formatMs(s.end_ms)}</td><td>${escapeHtml(s.text)}</td></tr>`)
    .join("");
  container.innerHTML = `
    ${lang}
    <table>
      <thead><tr><th>Start</th><th>End</th><th>Text</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}
