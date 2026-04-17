// YouTube Content Pack — title suggestions, description, SEO tags.

import { getSource, subscribe } from "../source-store.js";
import {
  escapeHtml,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
} from "../util.js";
import { wireProviderModelSync } from "./provider-model-defaults.js";

const { invoke } = window.__TAURI__.core;

let lastPack = null;

export function initYouTubePackView() {
  wireProviderModelSync("ytpack-provider", "ytpack-model");
  const results = document.getElementById("ytpack-results");
  const status = document.getElementById("ytpack-status");
  const btn = document.getElementById("btn-ytpack");
  const copyBtn = document.getElementById("btn-ytpack-copy");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("ytpack-provider").value;
    const model = document.getElementById("ytpack-model").value.trim();
    const language = document.getElementById("ytpack-language").value.trim() || "Vietnamese";

    setStatus(status, "generating…");
    results.innerHTML = "";
    lastPack = null;

    try {
      const out = await invoke("content_youtube_pack", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
        },
      });
      lastPack = out;
      renderPack(out, results);
      setStatus(status, `${out.titles.length} titles · ${out.tags.length} tags`, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    }
  });

  copyBtn.addEventListener("click", async () => {
    if (!lastPack) {
      setStatus(status, "nothing to copy yet", "err");
      return;
    }
    const text = formatPackText(lastPack);
    try {
      await navigator.clipboard.writeText(text);
      setStatus(status, "copied to clipboard", "ok");
    } catch (e) {
      setStatus(status, "copy failed: " + e, "err");
    }
  });

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then generate the pack</p>`;
      lastPack = null;
    }
  });
}

function renderPack(out, container) {
  const titlesHtml = out.titles
    .map((t, i) => `<li><strong>${i + 1}.</strong> ${escapeHtml(t)}</li>`)
    .join("");
  const tagsHtml = out.tags.map((t) => `<code>${escapeHtml(t)}</code>`).join(" ");
  container.innerHTML = `
    <h3>Title suggestions</h3>
    <ol>${titlesHtml}</ol>
    <h3>Description</h3>
    <pre class="wrapped">${escapeHtml(out.description)}</pre>
    <h3>Tags</h3>
    <p class="tag-cloud">${tagsHtml}</p>
  `;
}

function formatPackText(pack) {
  const titles = pack.titles.map((t, i) => `${i + 1}. ${t}`).join("\n");
  const tags = pack.tags.join(", ");
  return `=== TITLE SUGGESTIONS ===\n${titles}\n\n=== DESCRIPTION ===\n${pack.description}\n\n=== TAGS ===\n${tags}`;
}
