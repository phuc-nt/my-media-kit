# Getting Started

## Download & Install

### macOS
1. Download `CreatorUtils_x.x.x_aarch64.dmg` (Apple Silicon) or `CreatorUtils_x.x.x_x64.dmg` (Intel) from [Releases](https://github.com/phuc-nt/my-media-kit/releases)
2. Open the `.dmg` and drag **CreatorUtils** to Applications
3. First launch: right-click → Open (macOS Gatekeeper requires this for the first run)

### Windows
1. Download `CreatorUtils_x.x.x_x64-setup.exe` from [Releases](https://github.com/phuc-nt/my-media-kit/releases)
2. Run the installer and follow the prompts

### Auto-update
The app checks for updates automatically. When a new version is available, you'll be prompted to install it.

---

## System dependencies

You need `ffmpeg` on your system for video processing:

| OS | Install |
|----|---------|
| macOS | `brew install ffmpeg` |
| Windows | Download from [ffmpeg.org](https://ffmpeg.org/download.html) and add to PATH |

Optional: install `yt-dlp` for YouTube URL support (`pip install yt-dlp`).

---

## Build from source (developers)

### 1. System dependencies

| Dependency | Required | Install |
|------------|:---:|---------|
| Rust 1.80+ | Yes | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js 20+ | Yes | [nodejs.org](https://nodejs.org/) |
| ffmpeg + ffprobe | Yes | `brew install ffmpeg` / [ffmpeg.org](https://ffmpeg.org/download.html) |
| yt-dlp | For YouTube URLs | `pip install yt-dlp` |

### 2. AI backend (choose one)

**Option A: Local (Apple Silicon only)**
```bash
pip install mlx-lm mlx-whisper
```
Start the LLM server before using AI features:
```bash
mlx_lm.server --model mlx-community/Qwen2.5-7B-Instruct-4bit --port 8080
```

**Option B: Cloud API (any platform)**
No installation needed. Get an API key from your preferred provider:
- [OpenAI](https://platform.openai.com/api-keys) — recommended, covers both transcription (Whisper) and all AI features
- [Anthropic Claude](https://console.anthropic.com/)
- [Google Gemini](https://aistudio.google.com/apikey)

### 3. Build and run

```bash
git clone https://github.com/phuc-nt/my-media-kit.git
cd my-media-kit
npm install
npm run dev
```

The app window opens automatically.

## First-time setup

### Save your API key

1. Click **Settings** in the sidebar
2. Select your provider (e.g., OpenAI)
3. Paste your API key
4. Click **Save key**

Keys are stored in the OS keychain — never in plain text files.

### Configure AI defaults

In the sidebar, set:
- **AI provider** — which service to use for AI features
- **Model** — auto-filled with a recommended default when you switch providers
- **Output language** — language for summaries, chapters, etc. (default: Vietnamese)

These settings apply globally to all AI features.

## Typical workflow

```
1. Paste video path or YouTube URL in source picker
2. Go to Transcribe → click "Transcribe"
3. Switch to any tab (Summary, Chapters, YT Pack, etc.) → click the button
4. Copy or save results
```

Once transcribed, the transcript is cached — switching between features is instant. Reopening the same video auto-loads the cached transcript.

## Keyboard tips

- Drag and drop video files directly onto the source picker area
- Paste YouTube URLs — the app downloads and caches the video automatically
- Use "Force re-run" in Transcribe to override the cache
