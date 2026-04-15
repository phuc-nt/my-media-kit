// AutoCut export panel — shows cut stats and wires MP4 / NLE export buttons.
// Reads merged keep ranges from the cut store at export time.

import { getSource } from "../source-store.js";
import { deriveSiblingPath, formatMs, setStatus, showToast } from "../util.js";
import { buildKeepRanges, getTotalCutMs, getCutCounts, hasCuts } from "./autocut-cut-store.js";

const { invoke } = window.__TAURI__.core;

function formatBytes(n) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)} MB`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)} KB`;
  return `${n} B`;
}

function basename(p) {
  if (!p) return "";
  const sep = p.includes("/") ? "/" : "\\";
  const parts = p.split(sep);
  return parts[parts.length - 1] || p;
}

async function runExport(kind) {
  const { path: source, probe } = getSource();
  if (!source) return;

  const totalMs = probe?.durationMs ?? 0;
  const keepRanges = buildKeepRanges(totalMs);
  if (!keepRanges.length) {
    showToast("No keep ranges — run detection first", "err");
    return;
  }

  const statusEl = document.getElementById("export-status");
  setStatus(statusEl, "exporting…");
  try {
    let result;
    if (kind === "mp4") {
      result = await invoke("export_video_direct", {
        request: {
          sourcePath: source,
          outputPath: deriveSiblingPath(source, ".autocut.mp4"),
          keepRangesMs: keepRanges.map((r) => [r.start_ms, r.end_ms]),
          videoCodec: "libx264",
          audioCodec: "aac",
        },
      });
    } else {
      const targetMap = { fcp: "finalCutPro", premiere: "premiere", resolve: "davinciResolve" };
      const extMap = {
        fcp: ".autocut.fcpxml",
        premiere: ".autocut.premiere.xml",
        resolve: ".autocut.resolve.xml",
      };
      result = await invoke("nle_export", {
        request: {
          source_path: source,
          output_path: deriveSiblingPath(source, extMap[kind]),
          asset_name: basename(source),
          project_name: basename(source).replace(/\.[^.]+$/, ""),
          total_duration_ms: totalMs,
          frame_rate: probe?.frameRate ?? 30.0,
          width: probe?.width ?? 1920,
          height: probe?.height ?? 1080,
          audio_channels: probe?.audioChannels ?? 2,
          keep_ranges_ms: keepRanges.map((r) => [r.start_ms, r.end_ms]),
          target: targetMap[kind],
        },
      });
    }
    showToast(`Saved ${formatBytes(result.size_bytes)} → ${basename(result.output_path)}`, "ok");
    setStatus(statusEl, "done", "ok");
  } catch (err) {
    console.error(err);
    setStatus(statusEl, "export failed", "err");
    showToast(String(err), "err");
  }
}

// Refresh the stats line and show/hide the panel based on cut store state.
export function refreshExportPanel() {
  const panel = document.getElementById("autocut-export");
  if (!hasCuts()) {
    panel.hidden = true;
    return;
  }
  const { path: source, probe } = getSource();
  if (!source) { panel.hidden = true; return; }

  const totalMs = probe?.durationMs ?? 0;
  const keepRanges = buildKeepRanges(totalMs);
  const keepMs = keepRanges.reduce((s, r) => s + r.end_ms - r.start_ms, 0);
  const counts = getCutCounts();
  const cutMs = getTotalCutMs();

  const parts = [];
  if (counts.silence) parts.push(`${counts.silence} silence`);
  if (counts.filler) parts.push(`${counts.filler} filler`);
  if (counts.duplicate) parts.push(`${counts.duplicate} re-take`);
  if (counts.aiPrompt) parts.push(`${counts.aiPrompt} AI`);

  document.getElementById("autocut-export-stats").textContent =
    `Cut: ${formatMs(cutMs)} (${parts.join(", ")}) · Keep: ${formatMs(keepMs)} (${keepRanges.length} segments)`;
  panel.hidden = false;
}

export function initExportPanel() {
  document.getElementById("btn-export-mp4").addEventListener("click", () => runExport("mp4"));
  document.getElementById("btn-export-fcp").addEventListener("click", () => runExport("fcp"));
  document.getElementById("btn-export-premiere").addEventListener("click", () => runExport("premiere"));
  document.getElementById("btn-export-resolve").addEventListener("click", () => runExport("resolve"));
}
