# Backlog

Living priority list for CreatorUtils v2. Updated per session. Priority score = rough `value × ease × (6 - risk)` heuristic; use milestones as the authoritative sequencing.

## Scoring legend

- **Value** 1-5 — how much a user actually gains
- **Effort** 1-5 — **higher = cheaper**, not costlier (so the arithmetic goes the right way)
- **Risk** 1-5 — lower is safer
- **Score** — rough shorthand, not gospel; milestones win when they conflict

---

## Milestone A — Demo chạy được end-to-end (1-2 sessions)

Goal: open the app, drop a video, hit a button, see transcript + translate + summary + chapters + export. No polish. **Must chạy** trên máy Apple Silicon với MLX default.

| # | Item | Value | Effort | Risk | Notes |
|---|---|---|---|---|---|
| A1 | Transcription → AutoCut pipe with shared Tauri state (PCM + transcript cache keyed by source path) | 5 | 2 | 2 | Everything else reads from this cache — top of the chain. |
| A2 | Frontend tab wiring for Transcribe / Translate / Summary / Chapters views (plain HTML/JS, no bundler) | 5 | 3 | 1 | Commands already exist; this is just DOM + invoke plumbing. |
| A3 | Tauri state cache so slider re-detect skips ffmpeg | 4 | 2 | 2 | Folded into A1 in practice. |

After Milestone A: user drops a video, the app probes it, extracts PCM, runs whisper, renders transcript, one-click run summary / chapters / translate.

## Milestone B — Chất lượng output dùng được thật (1 session)

Goal: the output is usable in production, not just demo material.

| # | Item | Value | Effort | Risk | Notes |
|---|---|---|---|---|---|
| B1 | Filler prompt tuning (fix false positives on "Today/First/Then/Finally" or bump default to Qwen 7B) | 4 | 1 | 1 | Low effort; huge quality delta. Try prompt first, then raise model. |
| B2 | Length-mismatch repair for translate (retry missing indices) | 3 | 2 | 2 | Hardens local-model JSON drift; affects translate reliability. |
| B3 | Direct export wiring to AutoCut UI (Export to MP4 button using `export_video_direct`) | 4 | 2 | 1 | Backend command ready; just UI + flow. |
| B4 | NLE export wiring (FCPXML / Premiere / Resolve selector) | 4 | 2 | 1 | Same — backend ready, frontend missing. |

After Milestone B: creators can actually ship cuts with this tool.

## Milestone C — Polish, reach, shipping (multiple sessions later)

| # | Item | Value | Effort | Risk | Notes |
|---|---|---|---|---|---|
| C1 | Auto-spawn `mlx_lm.server` from the app with lifecycle management | 3 | 3 | 3 | Removes manual setup step; needs port-busy + model-load handling. |
| C2 | Cloud provider UX — collapse to "Advanced" when MLX is default on Apple Silicon | 2 | 2 | 1 | Settings polish. |
| C3 | Model switcher (preload Qwen 3B + 7B + 14B on separate ports) | 2 | 3 | 3 | RAM-heavy; only helps if we actually need multi-model. |
| C4 | `whisper-rs` backend for Windows / Intel Mac (cmake + cfg feature) | 2 | 4 | 3 | Gates cross-platform release. |
| C5 | Python venv bundling via `uv` | 3 | 4 | 4 | Removes pip install papercut; risky (bundle size, signing). |
| C6 | App bundling (`.app` / `.msi`, code sign, notarize) | 3 | 5 | 4 | Needed for distribution; last step. |

## Done (session log)

- 2026-04-10 — Tauri pivot + 7 crates + 105 unit tests (see `dev-log/2026-04-10.md`)
- 2026-04-11 session 1 — Non-LLM pipeline on real media, docs restructure
- 2026-04-11 session 2 — MLX-first default (whisper + lm) + 126 tests
- 2026-04-11 session 3 — Translate feature + VN skip rule + 137 tests
- 2026-04-11 session 4 — Git init + public repo on GitHub

## Open questions for next session start

- Filler default: stick with Qwen 3B + prompt tune, or bump to Qwen 7B?
- Frontend approach: stay with plain HTML/JS or introduce Vite + Svelte when views grow?
- Cloud provider visibility: hide when MLX default is healthy, or always show as "BYOK"?
