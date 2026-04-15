// AutoCut AI detection panel — filler words, duplicate/re-takes, AI prompt cut.
// All three detectors require a loaded transcript from the source store.
// Results are stored in the cut store and the onChanged callback triggers
// the export panel to refresh.

import { getSource } from "../source-store.js";
import { escapeHtml, formatMs, setStatus } from "../util.js";
import { setFillerCuts, setDuplicateCuts, setAiPromptCuts } from "./autocut-cut-store.js";

const { invoke } = window.__TAURI__.core;

function readAiConfig() {
  return {
    provider: document.getElementById("autocut-ai-provider").value,
    model: document.getElementById("autocut-ai-model").value.trim(),
  };
}

function getSegments() {
  const { transcript } = getSource();
  return transcript?.segments ?? [];
}

// Generic detection list renderer. toRow maps a detection to an HTML string.
function renderDetectionList(detections, container, toRow) {
  if (!detections.length) {
    container.innerHTML = `<p class="hint">No detections found.</p>`;
    return;
  }
  const rows = detections.map(toRow).join("");
  container.innerHTML = `<table><tbody>${rows}</tbody></table>`;
}

function fillerRow(d, i) {
  const words = d.fillerWords?.join(", ") ?? d.text;
  return (
    `<tr><td>${i + 1}</td>` +
    `<td>${formatMs(d.cutStartMs)}</td>` +
    `<td>${formatMs(d.cutEndMs)}</td>` +
    `<td><em>${escapeHtml(words)}</em></td></tr>`
  );
}

function duplicateRow(d, i) {
  return (
    `<tr><td>${i + 1}</td>` +
    `<td>${formatMs(d.cutStartMs)}</td>` +
    `<td>${formatMs(d.cutEndMs)}</td>` +
    `<td>${escapeHtml(d.reason)}</td></tr>`
  );
}

function aiPromptRow(d, i) {
  return (
    `<tr><td>${i + 1}</td>` +
    `<td>${formatMs(d.cutStartMs)}</td>` +
    `<td>${formatMs(d.cutEndMs)}</td>` +
    `<td>${escapeHtml(d.reason)}</td></tr>`
  );
}

// Normalize camelCase response fields to the cut store's expected shape.
// Backend returns camelCase (serde rename_all = "camelCase").
function normalizeToCutRange(d) {
  return { cut_start_ms: d.cutStartMs, cut_end_ms: d.cutEndMs };
}

export function initAiPanel(onChanged) {
  const panel = document.getElementById("autocut-ai");

  // Show/hide AI panel based on transcript availability — wired externally
  // via the source-store subscriber in autocut.js.

  // ── Filler detection ──────────────────────────────────────────────────────
  const fillerStatus = document.getElementById("filler-status");
  const fillerResults = document.getElementById("filler-results");

  document.getElementById("btn-detect-fillers").addEventListener("click", async () => {
    const segments = getSegments();
    if (!segments.length) {
      setStatus(fillerStatus, "no transcript", "err");
      return;
    }
    const { provider, model } = readAiConfig();
    setStatus(fillerStatus, "detecting…");
    try {
      const result = await invoke("content_filler_detect", {
        request: { provider, model, segments },
      });
      setFillerCuts(result.detections.map(normalizeToCutRange));
      renderDetectionList(result.detections, fillerResults, fillerRow);
      setStatus(fillerStatus, `${result.detections.length} detections`, "ok");
      onChanged();
    } catch (err) {
      console.error(err);
      setStatus(fillerStatus, "failed", "err");
      fillerResults.innerHTML = `<p class="hint error">${escapeHtml(String(err))}</p>`;
    }
  });

  // ── Duplicate / re-take detection ─────────────────────────────────────────
  const dupStatus = document.getElementById("duplicate-status");
  const dupResults = document.getElementById("duplicate-results");

  document.getElementById("btn-detect-duplicates").addEventListener("click", async () => {
    const segments = getSegments();
    if (!segments.length) {
      setStatus(dupStatus, "no transcript", "err");
      return;
    }
    const { provider, model } = readAiConfig();
    setStatus(dupStatus, "detecting…");
    try {
      const result = await invoke("content_duplicate_detect", {
        request: { provider, model, segments },
      });
      setDuplicateCuts(result.detections.map(normalizeToCutRange));
      renderDetectionList(result.detections, dupResults, duplicateRow);
      setStatus(dupStatus, `${result.detections.length} detections`, "ok");
      onChanged();
    } catch (err) {
      console.error(err);
      setStatus(dupStatus, "failed", "err");
      dupResults.innerHTML = `<p class="hint error">${escapeHtml(String(err))}</p>`;
    }
  });

  // ── AI Prompt cut ─────────────────────────────────────────────────────────
  const promptStatus = document.getElementById("ai-prompt-status");
  const promptResults = document.getElementById("ai-prompt-results");

  document.getElementById("btn-ai-prompt-cut").addEventListener("click", async () => {
    const segments = getSegments();
    if (!segments.length) {
      setStatus(promptStatus, "no transcript", "err");
      return;
    }
    const instruction = document.getElementById("autocut-ai-instruction").value.trim();
    if (!instruction) {
      setStatus(promptStatus, "enter instruction", "err");
      return;
    }
    const { provider, model } = readAiConfig();
    setStatus(promptStatus, "analyzing…");
    try {
      const result = await invoke("content_prompt_cut", {
        request: { provider, model, segments, instruction },
      });
      setAiPromptCuts(result.detections.map(normalizeToCutRange));
      renderDetectionList(result.detections, promptResults, aiPromptRow);
      setStatus(promptStatus, `${result.detections.length} cuts`, "ok");
      onChanged();
    } catch (err) {
      console.error(err);
      setStatus(promptStatus, "failed", "err");
      promptResults.innerHTML = `<p class="hint error">${escapeHtml(String(err))}</p>`;
    }
  });
}

// Called by the source-store subscriber to show/hide the AI panel.
export function setAiPanelVisible(hasTranscript) {
  const panel = document.getElementById("autocut-ai");
  panel.hidden = !hasTranscript;
}
