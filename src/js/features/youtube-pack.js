// YouTube Content Pack — title suggestions, description, SEO tags.

import { getSource, getAiConfig, getSummary, subscribe, markOutputDone } from "../source-store.js";
import {
  deriveOutputPath,
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

    btn.disabled = true;
    lastPack = null;

    const { provider, model, language } = getAiConfig();
    setStatus(status, "generating YT pack…", "running");
    results.innerHTML = "";

    try {
      const out = await invoke("content_youtube_pack", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
          summaryHint: getSummary()?.text ?? null,
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
    } finally {
      btn.disabled = false;
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

  subscribe(async (state) => {
    const { mode } = getAiConfig();
    const aiOk = mode === "cloud" || state.aiReady === true;
    const hasTranscript = !!(state.path && state.transcript);
    btn.disabled = !hasTranscript || !aiOk;
    copyBtn.disabled = !hasTranscript;
    if (!hasTranscript) {
      results.innerHTML = state.path
        ? `<p class="hint">run <strong>Transcribe</strong> first — YT Pack needs a transcript</p>`
        : `<p class="hint">select a source file, then transcribe it</p>`;
      lastPack = null;
      return;
    }
    if (!lastPack && state.outputStatus?.["youtube-pack"]) {
      try {
        const raw = await invoke("read_output_file", { sourcePath: state.path, filename: "youtube-pack.json" });
        if (raw) {
          lastPack = JSON.parse(raw);
          renderPack(lastPack, results);
          setStatus(status, `${lastPack.titles.length} titles · ${lastPack.tags.length} tags (cached)`, "ok");
        }
      } catch (_) {}
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
