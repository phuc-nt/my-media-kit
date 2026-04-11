// Transcribe feature view. Runs mlx_whisper_transcribe via Tauri, caches
// the result in source-store so every other tab can read it.

import { getSource, setTranscript, subscribe } from "../source-store.js";
import { escapeHtml, formatMs, renderErrorBox, requireSource, setStatus } from "../util.js";

const { invoke } = window.__TAURI__.core;

export function initTranscribeView() {
  const results = document.getElementById("transcribe-results");
  const status = document.getElementById("transcribe-status");
  const btnRun = document.getElementById("btn-transcribe");
  const btnForce = document.getElementById("btn-transcribe-refresh");

  async function run(force) {
    const source = getSource();
    if (!requireSource(source, status)) return;

    setStatus(status, force ? "re-running whisper…" : "running whisper…");
    results.innerHTML = "";
    try {
      const model = document.getElementById("transcribe-model").value.trim();
      const langRaw = document.getElementById("transcribe-lang").value.trim();
      const out = await invoke("mlx_whisper_transcribe", {
        path: source.path,
        language: langRaw || null,
        model: model || null,
        force: !!force,
      });
      setTranscript(out);
      renderSegments(out, results);
      setStatus(
        status,
        `${out.segments.length} segments${out.fromCache ? " (cached)" : ""}`,
        "ok",
      );
    } catch (e) {
      console.error(e);
      renderErrorBox(results, String(e));
      setStatus(status, "failed", "err");
    }
  }

  btnRun.addEventListener("click", () => run(false));
  btnForce.addEventListener("click", () => run(true));

  // If user navigates back to this tab after picking a new source, refresh
  // the placeholder or load the cached transcript.
  subscribe(async (state) => {
    if (!state.path) {
      results.innerHTML = `<p class="hint">no source selected</p>`;
      return;
    }
    if (state.transcript) {
      renderSegments(
        { ...state.transcript, fromCache: true },
        results,
      );
    } else {
      try {
        const hit = await invoke("get_cached_transcript", { path: state.path });
        if (hit) {
          setTranscript(hit);
          renderSegments(hit, results);
          setStatus(status, `${hit.segments.length} segments (cached)`, "ok");
        } else {
          results.innerHTML = `<p class="hint">not transcribed yet — hit the button above</p>`;
        }
      } catch (e) {
        renderErrorBox(results, String(e));
      }
    }
  });
}

function renderSegments(out, container) {
  const lang = out.language ? `<p class="hint">language: <code>${escapeHtml(out.language)}</code></p>` : "";
  const rows = out.segments
    .map(
      (s) =>
        `<tr><td>${formatMs(s.start_ms)}</td><td>${formatMs(s.end_ms)}</td><td>${escapeHtml(s.text)}</td></tr>`,
    )
    .join("");
  container.innerHTML = `
    ${lang}
    <table>
      <thead><tr><th>Start</th><th>End</th><th>Text</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}
