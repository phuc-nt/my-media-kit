# my-media-kit

Cross-platform creator video utilities — transcription, silence detection, auto-cut, AI summaries, and NLE round-trip export. Built on **Tauri v2 (Rust backend + HTML/JS frontend)**, with an MLX-first default path on Apple Silicon.

> Status: pre-release scaffolding. Core pipelines (silence detection, ffmpeg I/O, NLE XML export, AI provider protocol, MLX transcription + LLM) work end-to-end against real media and a local `mlx_lm.server` / `mlx_whisper`. Tauri UI is minimal. Not yet packaged for distribution.

## Features

### Non-LLM (100 % offline)

- **Silence detection** — RMS + auto-threshold (P15 noise floor, P75 speech level) + spike removal + configurable padding. Sub-100 ms slider re-runs via cached RMS.
- **Direct video export** — ffmpeg cut-and-concat pipeline to a final `.mp4` / `.mov` from a list of keep ranges.
- **NLE export** — FCPXML 1.11 (Final Cut Pro) + xmeml v5 (Premiere / DaVinci Resolve). Non-destructive: the output references your source media.
- **Media probe + PCM extraction** — ffmpeg / ffprobe sidecar wrappers; 16 kHz mono f32 suitable for whisper.

### LLM-driven (Apple Silicon ships local MLX first)

- **Transcription** — `mlx-whisper` with `whisper-large-v3-turbo`, word-level timestamps, bilingual EN / VN.
- **Filler detection** — identify and cut filler words (um, uh, ờ, à, thì, …) via a structured-output prompt.
- **AI free-form cut** — user instruction like *"remove the intro and sponsor mentions"* becomes a list of time ranges.
- **Summary** — brief / key points / action items, single-pass or two-pass consolidated for long videos.
- **Chapters** — YouTube-style description with timestamps, first chapter pinned to `00:00`.

All LLM features also work with **Claude**, **OpenAI**, **Gemini**, or **Ollama** when configured — just not the default on Apple Silicon. See `docs/architecture-decisions.md` (ADR-012).

## Architecture at a glance

```
src-tauri/
├── src/
│   ├── lib.rs                 # Tauri builder + command registry
│   ├── main.rs
│   └── commands/              # thin command wrappers; no business logic here
│       ├── meta.rs
│       ├── media.rs
│       ├── silence.rs
│       ├── transcription.rs   # Apple Silicon: mlx_whisper sidecar
│       ├── content.rs         # filler / summary / chapters via ai-kit
│       ├── ai.rs              # provider status + keyring helpers
│       ├── nle.rs             # FCPXML / xmeml export
│       └── export.rs          # direct video cut+concat
└── crates/
    ├── creator-core/          # domain types, errors, abort flag
    ├── silence-kit/           # pure DSP silence detection
    ├── media-kit/             # ffmpeg sidecar + WAV parser
    ├── transcription-kit/     # Transcriber trait + mlx_whisper backend
    ├── ai-kit/                # Claude / OpenAI / Gemini / Ollama / MLX
    ├── content-kit/           # filler, ai-prompt, summary, chapters
    └── nle-kit/               # FCPXML + xmeml builders

src/                           # frontend (HTML / CSS / JS)
├── index.html
├── styles/main.css
└── js/
    ├── main.js
    ├── sidebar.js
    ├── header.js
    └── features/
        ├── autocut.js
        └── settings.js
```

Every Rust crate is independently testable via `cargo test -p <crate>`. Platform-bound backends (ffmpeg, mlx_whisper, mlx_lm.server, whisper-rs) live behind traits so most code paths run without them.

## Getting started

### Prerequisites

- Rust ≥ 1.80 (`rustup install stable`)
- Node ≥ 20 + npm (Tauri CLI + frontend tooling)
- `ffmpeg` + `ffprobe` on PATH (or via `FFMPEG` / `FFPROBE` env vars)
- **Apple Silicon (recommended):** `pip install mlx-lm mlx-whisper` for the default local stack
- **Other platforms:** `brew install ollama` or an API key for Claude / OpenAI / Gemini

### Build + dev

```bash
# 1. Install frontend deps
npm install

# 2. Run tests
cd src-tauri
cargo test --workspace

# 3. Dev window
cd ..
npm run dev     # spins up tauri dev
```

To exercise the real-media integration tests:

```bash
CREATOR_UTILS_TEST_MEDIA=/absolute/path/to/video.mov cargo test --workspace
```

To try the MLX LLM path, start a local server before running content-feature commands:

```bash
mlx_lm.server \
  --model mlx-community/Qwen2.5-7B-Instruct-4bit \
  --port 8080 \
  --host 127.0.0.1
```

## Documentation

- **[docs/architecture-decisions.md](docs/architecture-decisions.md)** — ADRs explaining the tech stack, scope, and defaults.
- **[docs/llm-tasks.md](docs/llm-tasks.md)** — catalog of features that call an LLM and the viable model choices per feature.
- **[docs/dev-log/](docs/dev-log/)** — daily log of what was actually shipped, per session.

## Platform support

| Platform | Status | Default transcription | Default LLM |
|---|---|---|---|
| macOS Apple Silicon | Primary target | `mlx-whisper` | `mlx_lm.server` (Qwen2.5) |
| macOS Intel | Works, cloud-only | whisper-rs (TBD) | cloud (BYOK) |
| Windows | Works, cloud-only | whisper-rs (TBD) | cloud (BYOK) |
| Linux | Works, cloud-only | whisper-rs (TBD) | cloud (BYOK) |

## License

TBD. This is a personal project; no license committed yet.
