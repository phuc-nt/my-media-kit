# LLM Tasks — Feature Catalog + Model Choices

Inventory of CreatorUtils features that **require** an LLM call, what they ask the model to do, and which concrete model options are viable for each. Used when configuring providers, writing prompts, or deciding which default ships with the app.

> **Default per ADR-012:** on Apple Silicon, MVP uses local **MLX** backends (`mlx-whisper` for transcription, `mlx_lm.server` for LLM). Cloud providers (Claude/OpenAI/Gemini) remain implemented and tested, but are post-MVP / BYOK options. Sections below list MLX first where applicable.

> Features not in this doc (silence detection, media probe, NLE export, direct video cut) are **100 % offline** — no LLM, no network. Whisper transcription is local ML (whisper.cpp / MLX) not an LLM, handled in `reverse-engineering/04-whisper-transcription.md` + the transcription-kit MLX wrapper.

## Legend

- **Latency budget** — rough per-call wall clock target. Affects whether a batch pipeline needs streaming / progress UI.
- **Context need** — how much transcript the model must hold at once (tokens ≈ 4 chars).
- **Quality floor** — minimum capability we accept; smaller models below this consistently hallucinate or drift.
- **Structured output** — whether the call requires JSON schema enforcement.

---

## 1. Filler detection (AutoCut → Content)

**Input:** transcript batch (`[start_ms - end_ms] text` lines, ~30-60 s per batch).
**Output:** list of `FillerDetection { segmentIndex, cutStartMs, cutEndMs, text, fillerWords }`.
**Why LLM:** simple regex fails on context-dependent fillers ("like I said" vs "I like it"). Bilingual (EN + VN) filler list is embedded in the system prompt.

- Latency budget: 2-5 s per batch, parallelisable
- Context need: ~2-4 k tokens
- Quality floor: Claude Haiku / GPT-4o-mini / Gemini 2.0 Flash / Llama 3.1 8B
- Structured output: **required**

### Viable models

| Provider | Model | Notes |
|---|---|---|
| Claude | `claude-haiku-4-5-20251001` | Recommended default. Fast + accurate on structured cuts. |
| Claude | `claude-sonnet-4-5-20250929` | Overkill; use when Haiku misses subtle fillers. |
| OpenAI | `gpt-4o-mini` | Cheap default. JSON schema strict mode works well. |
| OpenAI | `gpt-4o` | Upgrade when mini over-cuts. |
| Gemini | `gemini-2.0-flash` | Fast + cheap. VN coverage good. |
| Gemini | `gemini-2.5-flash` | Slightly better VN context; not always available. |
| Ollama | `llama3.2:3b` | Works for EN-only. VN filler detection shaky. |
| Ollama | `qwen2.5:7b` | Better VN than llama. 4.7 GB. |
| **MLX (Apple Silicon)** | **`mlx-community/Qwen2.5-3B-Instruct-4bit`** | **Default.** 1.6 GB, fast, bilingual. |
| MLX | `mlx-community/Qwen2.5-7B-Instruct-4bit` | Upgrade when 3B misses subtle fillers. 4 GB. |
| Apple Intelligence | `LanguageModelSession` (on-device) | macOS 26+ only. Small context (~4k); needs per-batch chunking. |

---

## 2. AI Prompt cut (free-form instruction)

**Input:** transcript batch + user instruction like "remove the intro and any sponsor mentions".
**Output:** list of `AiPromptDetection { segmentIndex, cutStartMs, cutEndMs, text, reason }`.
**Why LLM:** user instruction is natural language; cannot be captured by regex / templates.

- Latency budget: 3-6 s per batch
- Context need: ~2-8 k tokens
- Quality floor: needs **reasoning** — Haiku/Flash minimum, Sonnet/GPT-4o preferred for subtle instructions
- Structured output: **required**

### Viable models

| Provider | Model | Notes |
|---|---|---|
| Claude | `claude-sonnet-4-5-20250929` | **Recommended.** Best at understanding nuanced editing intent. |
| Claude | `claude-haiku-4-5-20251001` | Works for simple instructions ("remove all English parts"). |
| OpenAI | `gpt-4o` | Recommended when Claude unavailable. |
| OpenAI | `gpt-4o-mini` | Budget option; misses context-dependent cuts ~20 % of the time. |
| Gemini | `gemini-2.5-flash` | Good middle ground, supports long transcripts. |
| Gemini | `gemini-2.0-flash` | Works but mixes up ranges sometimes. |
| Ollama | `qwen2.5:14b` | Borderline acceptable; `7b` is too unreliable for free-form intent. |
| **MLX (Apple Silicon)** | **`mlx-community/Qwen2.5-14B-Instruct-4bit`** | **Default.** 7.7 GB, best local reasoning. |
| MLX | `mlx-community/Qwen3-8B-4bit` | Faster alternative; occasional tool-call drift. |
| Apple Intelligence | — | Not recommended; context too small for free-form analysis. |

---

## 3. Summary (brief / key points / action items)

**Input:** whole transcript or batches of it.
**Output:** `SummaryResult { style, language, text }`.
**Flow:**
- Single pass if transcript fits model context.
- Two-pass (batch summaries → consolidation) when it doesn't.

**Why LLM:** summarisation is the canonical LLM job.

- Latency budget: 4-8 s per batch + 4-8 s consolidation
- Context need: transcript-sized (up to ~100 k tokens for hour-long videos)
- Quality floor: Haiku / GPT-4o-mini / Flash
- Structured output: light (`{ text: string }`); can fall back to freeform

### Viable models

| Provider | Model | Context | Notes |
|---|---|---|---|
| Claude | `claude-sonnet-4-5-20250929` | 200 k | **Recommended**; handles hour-long videos single-pass. |
| Claude | `claude-haiku-4-5-20251001` | 200 k | Cheap default. |
| OpenAI | `gpt-4o-mini` | 128 k | Good default. |
| OpenAI | `gpt-4o` | 128 k | Better consolidation pass. |
| Gemini | `gemini-2.5-pro` | 1 M | **Best** for very long videos — single-pass even for multi-hour content. |
| Gemini | `gemini-2.0-flash` | 1 M | Fast + cheap alternative. |
| Ollama | `llama3.2:3b` | 128 k | Acceptable for key-points; weak on action items. |
| Ollama | `qwen2.5:7b` | 128 k | Best local balance. |
| **MLX (Apple Silicon)** | **`mlx-community/Qwen2.5-7B-Instruct-4bit`** | 32 k | **Default.** Fast, handles 15-20 min videos single-pass. |
| MLX | `mlx-community/Qwen2.5-14B-Instruct-4bit` | 32 k | Better quality for longer videos. |
| MLX | `mlx-community/gemma-3-4b-it-qat-4bit` | 8 k | Fallback; smaller context. |
| Apple Intelligence | — | ~4 k | Needs heavy batching; only viable for short clips. |

---

## 4. Chapters (YouTube-style description)

**Input:** transcript with word timestamps.
**Output:** `ChapterList { language, chapters: [{ timestampMs, title }] }`, first chapter pinned to `00:00`.
**Why LLM:** needs semantic topic-boundary detection; not a sliding-window heuristic.

- Latency budget: 5-10 s
- Context need: full transcript (up to ~100 k tokens)
- Quality floor: Haiku / Flash
- Structured output: **required** (enforces first-at-zero + monotonic order)

### Viable models

| Provider | Model | Notes |
|---|---|---|
| Claude | `claude-sonnet-4-5-20250929` | **Recommended**; consistent 5-10 chapter granularity. |
| Claude | `claude-haiku-4-5-20251001` | Acceptable; sometimes too granular (15+ chapters). |
| OpenAI | `gpt-4o` | Good. |
| OpenAI | `gpt-4o-mini` | Budget; occasional over-segmentation. |
| Gemini | `gemini-2.5-pro` | **Best for long videos** thanks to 1 M context. |
| Gemini | `gemini-2.0-flash` | Fast default. |
| Ollama | `qwen2.5:7b` | Viable locally; weaker title quality. |
| **MLX (Apple Silicon)** | **`mlx-community/Qwen2.5-7B-Instruct-4bit`** | **Default.** Good titles for 5-20 min videos. |
| MLX | `mlx-community/Qwen2.5-14B-Instruct-4bit` | Upgrade when 7B over-segments. |
| Apple Intelligence | — | Context too small for a whole transcript. |

---

## 5. Duplicate / abandoned-phrase detection *(deferred)*

**Input:** transcript batch.
**Output:** list of `DuplicateGroup { keepSegmentIndex, removeSegments: [...] }`.
**Why LLM:** needs to decide which take of a repeated phrase is "cleaner".

**Status:** not implemented in v2 MVP (ADR-010). Filler + AI-Prompt detection cover ~80 % of real use cases without this feature.

### Viable models (for when it lands)

| Provider | Model |
|---|---|
| Claude | `claude-sonnet-4-5-20250929` (recommended; fewer false positives) |
| OpenAI | `gpt-4o` |
| Gemini | `gemini-2.5-flash` |

---

## 6. Ad compliance (VN Luật Quảng cáo) *(deferred)*

Mentioned for completeness. v1 shipped a 4-pass Vietnamese compliance check. v2 drops the feature (ADR-007). If re-added, it would require Sonnet-level reasoning + large context + Vietnamese-first model.

**Viable:** Claude Sonnet 4.5, GPT-4o, Gemini 2.5 Pro. Flash-tier models hallucinate citations.

---

## 7. Translation *(deferred)*

Mentioned for completeness. v1 translates segment-by-segment with context overflow fallback. Any modern LLM handles this trivially.

**Viable:** any provider listed above. Prefer Haiku / mini / Flash — high latency is the main user complaint.

---

## Defaults v2 ships with

Per **ADR-012**, Apple Silicon ships **MLX-first**. Cloud providers are post-MVP.

### Apple Silicon (primary target for MVP)

| Feature | Default provider | Default model | Reason |
|---|---|---|---|
| Filler detection | MLX (local) | `mlx-community/Qwen2.5-3B-Instruct-4bit` | Fast, good VN/EN bilingual |
| AI prompt cut | MLX (local) | `mlx-community/Qwen2.5-14B-Instruct-4bit` | Needs real reasoning |
| Summary | MLX (local) | `mlx-community/Qwen2.5-7B-Instruct-4bit` | Balance quality/speed |
| Chapters | MLX (local) | `mlx-community/Qwen2.5-7B-Instruct-4bit` | Same balance point |

**Serving:** `mlx_lm.server --model <path> --port 8080`. Exposes OpenAI-compatible `/v1/chat/completions`. Our existing `OpenAiProvider` is reused with `base_url = http://127.0.0.1:8080/v1`.

**Whisper:** `mlx-whisper` CLI with `--model mlx-community/whisper-large-v3-turbo --word-timestamps True -f json`.

**Pre-downloaded on dev machine:**
- `mlx-community/Qwen2.5-3B-Instruct-4bit` (1.6 GB)
- `mlx-community/Qwen2.5-7B-Instruct-4bit` (4.0 GB)
- `mlx-community/Qwen2.5-14B-Instruct-4bit` (7.7 GB)
- `mlx-community/Qwen3-8B-4bit` (4.3 GB)
- `mlx-community/gemma-3-4b-it-qat-4bit` (2.8 GB)
- `mlx-community/gemma-4-E4B-it-4bit` (4.9 GB)
- `mlx-community/whisper-large-v3-turbo` (1.5 GB)

### Windows / Intel Mac / Linux (post-MVP)

| Feature | Default provider | Default model |
|---|---|---|
| Filler detection | Claude | `claude-haiku-4-5-20251001` |
| AI prompt cut | Claude | `claude-sonnet-4-5-20250929` |
| Summary | Claude | `claude-sonnet-4-5-20250929` |
| Chapters | Claude | `claude-sonnet-4-5-20250929` |

Fallbacks: `gpt-4o-mini` (filler) → `gpt-4o` (rest) → `gemini-2.0-flash`. User may also run Ollama locally.

User can override per-feature in Settings → AI tab (planned Phase 10).

---

## Test credentials you need

**Apple Silicon developers:** nothing. MLX runs fully local. `mlx_lm.server --model mlx-community/Qwen2.5-7B-Instruct-4bit --port 8080` and you're testing.

For Windows / Intel Mac / Linux, **one** of:

| Provider | Minimum spend for full test suite | Sign-up URL |
|---|---|---|
| Claude | ~$0.50 (Haiku) or ~$3 (Sonnet) | https://console.anthropic.com/settings/keys |
| OpenAI | ~$0.30 (gpt-4o-mini) or ~$2 (gpt-4o) | https://platform.openai.com/api-keys |
| Gemini | **Free** (generous free tier) | https://aistudio.google.com/apikey |
| Ollama | **Free** (local) | `brew install ollama && ollama pull qwen2.5:7b` |

For smoke testing prompt shapes without spending: **Ollama + `qwen2.5:7b`** on non-Mac, **MLX** on Apple Silicon.

## Open questions

1. Should the Filler detector auto-fall-back to Haiku/mini/flash when Sonnet rate-limits, or surface the error?
2. Does Chapter extraction benefit from a second "pin-title" pass, or is one-shot enough at Sonnet level?
3. For Ollama, which base install do we recommend in docs? `llama3.2:3b` (fast, weak VN) vs `qwen2.5:7b` (slower, better VN)?
4. Apple Intelligence integration: worth the effort given the ~4 k context cap for summary/chapter workflows?
