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
      document.getElementById("silence-results").innerHTML = "";
      document.getElementById("autocut-export").hidden = true;
      document.getElementById("filler-results").innerHTML = "";
      document.getElementById("duplicate-results").innerHTML = "";
      document.getElementById("ai-prompt-results").innerHTML = "";
      // Reset step badges to initial state
      const s1Badge = document.getElementById("step1-badge");
      const s1Num = document.getElementById("step1-num");
      if (s1Badge) { s1Badge.textContent = "offline · DSP"; s1Badge.className = "step-badge"; }
      if (s1Num) s1Num.className = "step-num active";
      clearAll();
      refreshExportPanel(); // resets step3 badge
    }
    // Update speech mask whenever transcript changes so silence cuts
    // are clipped against actual speech regions at export time.
    const segments = state.transcript?.segments ?? [];
    setSpeechMask(segments);

    // AI panel is only usable once a transcript is loaded.
    setAiPanelVisible(!!state.transcript);
  });
}
