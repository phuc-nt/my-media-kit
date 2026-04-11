// AutoCut view — silence detection. Reads the shared source path from
// source-store; backend cache keeps slider tweaks fast after the first
// run.

import { getSource, subscribe } from "../source-store.js";
import {
  escapeHtml,
  formatMs,
  renderErrorBox,
  requireSource,
  setStatus,
} from "../util.js";

const { invoke } = window.__TAURI__.core;

const sliderIds = [
  ["cfg-threshold", "cfg-threshold-val", (v) => v.toFixed(3)],
  ["cfg-min-duration", "cfg-min-duration-val", (v) => v.toFixed(1)],
  ["cfg-pad-left", "cfg-pad-left-val", (v) => v.toFixed(2)],
  ["cfg-pad-right", "cfg-pad-right-val", (v) => v.toFixed(2)],
  ["cfg-spike", "cfg-spike-val", (v) => v.toFixed(2)],
];

function readConfig() {
  return {
    threshold: parseFloat(document.getElementById("cfg-threshold").value),
    useAutoThreshold: document.getElementById("cfg-auto").checked,
    minimumDurationS: parseFloat(document.getElementById("cfg-min-duration").value),
    paddingLeftS: parseFloat(document.getElementById("cfg-pad-left").value),
    paddingRightS: parseFloat(document.getElementById("cfg-pad-right").value),
    removeShortSpikesS: parseFloat(document.getElementById("cfg-spike").value),
  };
}

function renderResults(regions, frameCount, fromCache, container) {
  if (!regions.length) {
    container.innerHTML = `<p class="hint">No silence regions detected (${frameCount} frames, ${fromCache ? "cached" : "fresh"}).</p>`;
    return;
  }
  const rows = regions
    .map(
      (r, i) =>
        `<tr><td>${i + 1}</td><td>${formatMs(r.start_ms)}</td><td>${formatMs(r.end_ms)}</td><td>${((r.end_ms - r.start_ms) / 1000).toFixed(2)}s</td></tr>`,
    )
    .join("");
  container.innerHTML = `
    <table>
      <thead><tr><th>#</th><th>Start</th><th>End</th><th>Duration</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    <p class="hint" style="margin-top:8px">
      ${regions.length} region${regions.length > 1 ? "s" : ""} · ${frameCount} frames · ${fromCache ? "cached PCM" : "fresh extract"}
    </p>
  `;
}

export function initAutoCutView() {
  sliderIds.forEach(([inputId, outputId, fmt]) => {
    const input = document.getElementById(inputId);
    const output = document.getElementById(outputId);
    if (!input || !output) return;
    const update = () => (output.textContent = fmt(parseFloat(input.value)));
    input.addEventListener("input", update);
    update();
  });

  const btn = document.getElementById("btn-detect");
  const status = document.getElementById("detect-status");
  const container = document.getElementById("silence-results");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    setStatus(status, "detecting…");
    try {
      const config = readConfig();
      const result = await invoke("detect_silence_in_file", {
        path: source.path,
        config,
      });
      renderResults(
        result.regions,
        result.frameCount ?? result.frame_count ?? 0,
        !!(result.fromCache ?? result.from_cache),
        container,
      );
      setStatus(status, `${result.regions.length} regions`, "ok");
    } catch (err) {
      console.error(err);
      renderErrorBox(container, String(err));
      setStatus(status, "failed", "err");
    }
  });

  subscribe((state) => {
    if (!state.path) {
      container.innerHTML = `<p class="hint">pick a source video in the sidebar</p>`;
    }
  });
}
