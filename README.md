<p align="center">
  <img src="assets/logo/logo.png" width="96" alt="My Media Kit logo" />
</p>

<h1 align="center">My Media Kit</h1>

<p align="center">
  All-in-one video toolkit for content creators.<br/>
  Transcribe, translate, summarize, generate chapters, find viral clips — locally or via cloud APIs.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" />
  <img src="https://img.shields.io/badge/license-MIT-green" />
  <img src="https://img.shields.io/badge/version-0.1.0-orange" />
</p>

---

## What it does

Drop a video file (or paste a YouTube URL) and get:

| Feature | Description |
|---------|-------------|
| **YouTube Downloader** | Paste any YouTube URL — app fetches the video to `~/Downloads/MyMediaKit/{title} [{id}].mp4` and feeds it straight into the pipeline. `yt-dlp` ships inside the app, no install needed. |
| **Transcribe** | Speech-to-text with word-level timestamps (MLX Whisper local or OpenAI cloud) |
| **Translate** | Translate transcript to any language, auto-skips if source matches target |
| **Summary** | Brief narrative, key points, or action items |
| **Chapters** | YouTube-ready chapter markers (first pinned to 0:00) |
| **YouTube Pack** | 5 title suggestions + full description + SEO tags in one shot |
| **Viral Clips** | Best 15-60s moments for Shorts/Reels/TikTok with hooks and captions |
| **Clean Transcript** | Rule-based filler word removal (no AI needed) |

## How it works

```
Video / YouTube URL
        |
    Transcribe (MLX local or OpenAI cloud)
        |
    ┌───┴────────────────────────────┐
    |   All features share the       |
    |   cached transcript — set      |
    |   provider + model once in     |
    |   the sidebar, click any tab   |
    └────────────────────────────────┘
```

One source. One config. Every feature is one click away.

## Quick start

### Prerequisites

- [Rust](https://rustup.rs/) 1.80+
- [Node.js](https://nodejs.org/) 20+ with npm
- `ffmpeg` + `ffprobe` on PATH

**Apple Silicon (local AI):**
```bash
pip install mlx-lm mlx-whisper
```

**Any platform (cloud AI):**
Set an API key in Settings (OpenAI recommended — covers both transcription and all AI features).

### Run

```bash
npm install
npm run dev
```

See [Getting Started](user-docs/getting-started.md) for detailed setup.

## AI Providers

| Provider | Transcription | AI Features | Setup |
|----------|:---:|:---:|-------|
| MLX (local) | Yes | Yes | Apple Silicon + pip install |
| OpenAI | Yes (Whisper) | Yes (GPT-4o) | API key |
| Claude | - | Yes | API key |
| Gemini | - | Yes | API key |
| Ollama | - | Yes | Local install |
| OpenRouter | - | Yes | API key |

## Documentation

- [Getting Started](user-docs/getting-started.md) — install, configure, first run
- [Features Guide](user-docs/features.md) — what each feature does and how to use it

## Tech stack

Built with [Tauri v2](https://tauri.app/) (Rust backend + HTML/JS frontend). Seven independent Rust crates keep the architecture modular and testable.

## License

[MIT](LICENSE)
