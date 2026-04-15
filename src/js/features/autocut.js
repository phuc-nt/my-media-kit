// AutoCut view — orchestrates silence detection, AI content analysis, and export.
//
// Pipeline (mirrors original Swift app):
//   1. Silence detection  → DSP-based, fully offline, results cached after first run
//   2. Filler detection   → AI, requires transcript (ờ, ừm, um, uh, …)
//   3. Re-take detection  → AI, requires transcript (false starts, duplicates)
//   4. AI Prompt cut      → AI, requires transcript + free-form user instruction
//   5. Export             → merges all cut regions → MP4 or NLE (FCPXML/Premiere/Resolve)
//
// Sub-modules:
//   autocut-cut-store.js    — shared state for all cut region sets
//   autocut-silence-panel.js — silence detection UI
//   autocut-ai-panel.js     — filler / duplicate / AI-prompt UI
//   autocut-export-panel.js — export stats + buttons

import { subscribe } from "../source-store.js";
import { clearAll, setSpeechMask } from "./autocut-cut-store.js";
import { initSilencePanel } from "./autocut-silence-panel.js";
import { initAiPanel, setAiPanelVisible } from "./autocut-ai-panel.js";
import { initExportPanel, refreshExportPanel } from "./autocut-export-panel.js";

export function initAutoCutView() {
  // onChanged is called by any detector when its results change.
  // It re-renders the export panel stats and shows/hides the panel.
  function onChanged() {
    refreshExportPanel();
  }

  initSilencePanel(onChanged);
  initAiPanel(onChanged);
  initExportPanel();

  // React to source changes: clear all cut state and reset UI.
  subscribe((state) => {
    if (!state.path) {
      document.getElementById("silence-results").innerHTML =
        `<p class="hint">pick a source video in the sidebar</p>`;
      document.getElementById("autocut-export").hidden = true;
      document.getElementById("filler-results").innerHTML = "";
      document.getElementById("duplicate-results").innerHTML = "";
      document.getElementById("ai-prompt-results").innerHTML = "";
      clearAll();
    }
    // Update speech mask whenever transcript changes so silence cuts
    // are clipped against actual speech regions at export time.
    const segments = state.transcript?.segments ?? [];
    setSpeechMask(segments);

    // AI panel is only usable once a transcript is loaded.
    setAiPanelVisible(!!state.transcript);
  });
}
