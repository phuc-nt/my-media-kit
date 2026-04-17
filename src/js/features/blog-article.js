// Blog Article — convert transcript into a structured article.

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

let lastArticle = null;

export function initBlogArticleView() {
  wireProviderModelSync("blog-provider", "blog-model");
  const results = document.getElementById("blog-results");
  const status = document.getElementById("blog-status");
  const btn = document.getElementById("btn-blog");
  const copyBtn = document.getElementById("btn-blog-copy");

  btn.addEventListener("click", async () => {
    const source = getSource();
    if (!requireSource(source, status)) return;
    if (!requireTranscript(source.transcript, status)) return;

    const provider = document.getElementById("blog-provider").value;
    const model = document.getElementById("blog-model").value.trim();
    const language = document.getElementById("blog-language").value.trim() || "Vietnamese";

    setStatus(status, "generating article…");
    results.innerHTML = "";
    lastArticle = null;

    try {
      const out = await invoke("content_blog_article", {
        request: {
          provider,
          model,
          segments: source.transcript.segments,
          language,
        },
      });
      lastArticle = out;
      renderArticle(out, results);
      setStatus(status, `${out.sections.length} sections`, "ok");
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    }
  });

  copyBtn.addEventListener("click", async () => {
    if (!lastArticle) {
      setStatus(status, "nothing to copy yet", "err");
      return;
    }
    const md = formatMarkdown(lastArticle);
    try {
      await navigator.clipboard.writeText(md);
      setStatus(status, "copied markdown", "ok");
    } catch (e) {
      setStatus(status, "copy failed: " + e, "err");
    }
  });

  subscribe((state) => {
    if (!state.path || !state.transcript) {
      results.innerHTML = `<p class="hint">transcribe a source first, then generate a blog article</p>`;
      lastArticle = null;
    }
  });
}

function renderArticle(out, container) {
  const sectionsHtml = out.sections
    .map(
      (s) => `
      <div class="article-section">
        <h3>${escapeHtml(s.heading)}</h3>
        <p>${escapeHtml(s.content)}</p>
      </div>`,
    )
    .join("");
  container.innerHTML = `
    <h2 class="article-title">${escapeHtml(out.title)}</h2>
    ${sectionsHtml}
  `;
}

function formatMarkdown(article) {
  let md = `# ${article.title}\n\n`;
  for (const s of article.sections) {
    md += `## ${s.heading}\n\n${s.content}\n\n`;
  }
  return md.trimEnd();
}
