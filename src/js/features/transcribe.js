// Transcribe feature view. Backend: mlx_whisper_transcribe (Apple Silicon,
// local, progress events). Results cached in source-store so every other
// tab reuses them without re-invoking whisper.

import { getSource, setTranscript, subscribe } from "../source-store.js";
import {
  deriveSiblingPath,
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

const DEFAULT_MLX_MODEL = "mlx-community/whisper-large-v3-turbo";

export function initTranscribeView() {
  const results = document.getElementById("transcribe-results");
  const status = document.getElementById("transcribe-status");
  const btnRun = document.getElementById("btn-transcribe");
  const btnForce = document.getElementById("btn-transcribe-refresh");
  const btnSaveSrt = document.getElementById("btn-transcribe-save");
  const btnSaveTxt = document.getElementById("btn-transcribe-save-txt");
  const modelInput = document.getElementById("transcribe-model");

  const progressBox = document.getElementById("transcribe-progress");
  const progressBar = document.getElementById("transcribe-progress-bar");
  const progressLabel = document.getElementById("transcribe-progress-label");
  const progressValue = document.getElementById("transcribe-progress-value");

  if (!modelInput.value) modelInput.value = DEFAULT_MLX_MODEL;

  let currentTranscript = null;
  let running = false;

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

  function setSaveButtons(enabled) {
    btnSaveSrt.disabled = !enabled;
    btnSaveTxt.disabled = !enabled;
  }

  function applyTranscript(out) {
    currentTranscript = out;
    setTranscript(out);
    renderSegments(out, results);
    setSaveButtons(out?.segments?.length > 0);
  }

  // Listen for progress events once. The backend always emits an initial
  // 0% event before spawning whisper, so we can rely on events to show the
  // bar even if the first segment takes a while to arrive.
  listen("mlx_whisper_progress", (event) => {
    if (!running) return;
    const { percent, current_ms, total_ms } = event.payload || {};
    if (typeof percent !== "number") return;
    progressBar.className = "progress-bar";
    progressBar.style.width = `${percent.toFixed(1)}%`;
    progressValue.textContent = `${percent.toFixed(1)}%`;
    const cur = formatMs(current_ms || 0);
    const tot = total_ms ? formatMs(total_ms) : "?";
    progressLabel.textContent = `running whisper… ${cur} / ${tot}`;
  });

  async function run(force) {
    const source = getSource();
    if (!requireSource(source, status)) return;

    results.innerHTML = "";
    setSaveButtons(false);

    const model = modelInput.value.trim();
    const langRaw = document.getElementById("transcribe-lang").value.trim();
    const label = force ? "re-running whisper…" : "running whisper…";
    setRunning(true, label);

    try {
      const out = await invoke("mlx_whisper_transcribe", {
        path: source.path,
        language: langRaw || null,
        model: model || null,
        force: !!force,
      });
      applyTranscript(out);
      setStatus(
        status,
        `${out.segments.length} segments${out.fromCache ? " (cached)" : ""}`,
        "ok",
      );
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    } finally {
      setRunning(false);
    }
  }

  async function save(format) {
    if (!currentTranscript?.segments?.length) return;
    const source = getSource();
    if (!source?.path) return;
    const isSrt = format === "srt";
    const suffix = isSrt ? ".transcript.srt" : ".transcript.txt";
    const target = deriveSiblingPath(source.path, suffix);
    const content = isSrt
      ? segmentsToSrt(currentTranscript.segments)
      : segmentsToPlainText(currentTranscript.segments);
    try {
      const written = await invoke("save_text_file", {
        path: target,
        content,
      });
      showToast(`saved → ${written}`, "ok");
    } catch (e) {
      console.error(e);
      showToast(`save failed: ${e}`, "err");
    }
  }

  btnRun.addEventListener("click", () => run(false));
  btnForce.addEventListener("click", () => run(true));
  btnSaveSrt.addEventListener("click", () => save("srt"));
  btnSaveTxt.addEventListener("click", () => save("txt"));

  // If user navigates back to this tab after picking a new source, refresh
  // the placeholder or load the cached transcript.
  subscribe(async (state) => {
    if (running) return;
    if (!state.path) {
      results.innerHTML = `<p class="hint">no source selected</p>`;
      currentTranscript = null;
      setSaveButtons(false);
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
        results.innerHTML = `<p class="hint">not transcribed yet — hit the button above</p>`;
        currentTranscript = null;
        setSaveButtons(false);
      }
    } catch (e) {
      renderErrorBox(results, String(e));
    }
  });
}

function renderSegments(out, container) {
  const lang = out.language ? `<p class="hint">language: <code>${escapeHtml(out.language)}</code></p>` : "";
  const rows = out.segments
    .map(
      (s) =>
        `<tr><td>${formatMs(s.start_ms)}</td><td>${formatMs(s.end_ms)}</td><td>${escapeHtml(s.text)}</td></tr>`,
    )
    .join("");
  container.innerHTML = `
    ${lang}
    <table>
      <thead><tr><th>Start</th><th>End</th><th>Text</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}
