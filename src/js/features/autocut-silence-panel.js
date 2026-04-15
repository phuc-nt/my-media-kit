// AutoCut silence detection panel — DSP-based silence detection UI.
// Reads config sliders, invokes detect_silence_in_file, stores results
// in the cut store, and notifies the export panel to refresh.

import { getSource } from "../source-store.js";
import { escapeHtml, formatMs, renderErrorBox, requireSource, setStatus } from "../util.js";
import { setSilenceCuts } from "./autocut-cut-store.js";

const { invoke } = window.__TAURI__.core;

const SLIDER_DEFS = [
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

// Invert silence regions to compute keep ranges (for stats display only;
// export uses the cut store's buildKeepRanges which merges all sources).
export function invertRegions(silenceRegions, totalMs) {
  const keeps = [];
  let cursor = 0;
  for (const r of silenceRegions) {
    if (r.start_ms > cursor) keeps.push({ start_ms: cursor, end_ms: r.start_ms });
    cursor = r.end_ms;
  }
  if (cursor < totalMs) keeps.push({ start_ms: cursor, end_ms: totalMs });
  return keeps;
}

function renderRegionTable(regions, frameCount, fromCache, container) {
  if (!regions.length) {
    container.innerHTML = `<p class="hint">No silence detected (${frameCount} frames, ${fromCache ? "cached" : "fresh"}).</p>`;
    return;
  }
  const rows = regions
    .map(
      (r, i) =>
        `<tr><td>${i + 1}</td><td>${formatMs(r.start_ms)}</td><td>${formatMs(r.end_ms)}</td>` +
        `<td>${((r.end_ms - r.start_ms) / 1000).toFixed(2)}s</td></tr>`,
    )
    .join("");
  container.innerHTML = `
    <table>
      <thead><tr><th>#</th><th>Start</th><th>End</th><th>Duration</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    <p class="hint" style="margin-top:8px">
      ${regions.length} region${regions.length > 1 ? "s" : ""} · ${frameCount} frames ·
      ${fromCache ? "cached PCM" : "fresh extract"}
    </p>`;
}

export function initSilencePanel(onChanged) {
  SLIDER_DEFS.forEach(([inputId, outputId, fmt]) => {
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
    const { path: source, probe } = getSource();
    if (!requireSource({ path: source }, status)) return;
    setStatus(status, "detecting…");
    try {
      const config = readConfig();
      const result = await invoke("detect_silence_in_file", { path: source, config });
      const totalMs = probe?.durationMs ?? (result.regions.at(-1)?.end_ms ?? 0);
      const frameCount = result.frameCount ?? result.frame_count ?? 0;
      const fromCache = !!(result.fromCache ?? result.from_cache);

      setSilenceCuts(result.regions);
      renderRegionTable(result.regions, frameCount, fromCache, container);
      setStatus(status, `${result.regions.length} regions`, "ok");
      onChanged();
    } catch (err) {
      console.error(err);
      renderErrorBox(container, String(err));
      setStatus(status, "failed", "err");
    }
  });
}
