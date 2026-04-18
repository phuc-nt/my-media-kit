// YouTube Content Pack — title suggestions, description, SEO tags.

import { getSource, getAiConfig, subscribe, markOutputDone } from "../source-store.js";
import {
  deriveOutputPath,
  ensureAiReady,
  escapeHtml,
  renderErrorBox,
  requireSource,
  requireTranscript,
  setStatus,
  showToast,
} from "../util.js";

const { invoke } = window.__TAURI__.core;

let lastPack = null;

export function initYouTubePackView() {
  const results = document.getElementById("ytpack-results");
  const status = document.getElementById("ytpack-status");
  const btn = document.getElementById("btn-ytpack");
  const copyBtn = document.getElementById("btn-ytpack-copy");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const { provider, model, language, mode } = getAiConfig();
    if (!await ensureAiReady(mode, status)) return;

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
      const target = deriveOutputPath(source.outputDir, "youtube-pack.json");
      if (target) {
        try {
          await invoke("save_text_file", { path: target, content: JSON.stringify(out, null, 2) });
          markOutputDone("youtube-pack");
          showToast(`saved → ${target}`, "ok");
        } catch (_) {}
      }
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
