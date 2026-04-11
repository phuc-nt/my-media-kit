// AutoCut view — wire sliders to the detect_silence_in_file command.

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
    minimumDurationS: parseFloat(
      document.getElementById("cfg-min-duration").value,
    ),
    paddingLeftS: parseFloat(document.getElementById("cfg-pad-left").value),
    paddingRightS: parseFloat(document.getElementById("cfg-pad-right").value),
    removeShortSpikesS: parseFloat(document.getElementById("cfg-spike").value),
  };
}

function formatMs(ms) {
  const s = ms / 1000;
  const mm = Math.floor(s / 60);
  const ss = (s - mm * 60).toFixed(2);
  return `${mm}:${ss.padStart(5, "0")}`;
}

function renderResults(regions, frameCount) {
  const container = document.getElementById("silence-results");
  if (!regions.length) {
    container.innerHTML = `<p class="hint">No silence regions detected (${frameCount} frames analyzed).</p>`;
    return;
  }
  const rows = regions
    .map(
      (r, i) =>
        `<tr><td>${i + 1}</td><td>${formatMs(r.startMs)}</td><td>${formatMs(r.endMs)}</td><td>${((r.endMs - r.startMs) / 1000).toFixed(2)}s</td></tr>`,
    )
    .join("");
  container.innerHTML = `
    <table>
      <thead><tr><th>#</th><th>Start</th><th>End</th><th>Duration</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    <p class="hint" style="margin-top:8px">
      ${regions.length} region${regions.length > 1 ? "s" : ""} · ${frameCount} frames analyzed
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
  btn.addEventListener("click", async () => {
    const path = document.getElementById("autocut-path").value.trim();
    if (!path) {
      status.textContent = "pick a file first";
      status.className = "status err";
      return;
    }
    status.textContent = "detecting…";
    status.className = "status";
    try {
      const config = readConfig();
      const result = await invoke("detect_silence_in_file", { path, config });
      renderResults(result.regions, result.frameCount);
      status.textContent = `${result.regions.length} regions found`;
      status.className = "status ok";
    } catch (err) {
      console.error(err);
      status.textContent = String(err).slice(0, 120);
      status.className = "status err";
    }
  });
}
