# Features Guide

All features read from the same cached transcript. Transcribe once, use everywhere.

---

## Transcribe

Converts video audio to text with timestamps.

**Backends:**
- **MLX Whisper** — runs locally on Apple Silicon, shows real-time progress
- **OpenAI Whisper** — cloud API, works on any platform

**Output:** timestamped segments displayed in a table.

**Save options:**
- **Save .srt** — standard subtitle file
- **Save .txt** — plain text, one line per segment
- **Save clean .srt** — SRT with filler words (um, uh, ờ, à) automatically removed. No AI needed — uses rule-based detection on word-level timestamps.

---

## Translate

Translates the transcript to a target language.

- Set **Target language** (BCP-47 code, e.g., `vi`, `en`, `ja`)
- If the source language already matches the target, translation is skipped automatically
- Save as `.srt` or `.txt` in the target language

---

## Summary

Generates a summary from the transcript.

**Styles:**
- **Brief narrative** — 2-3 paragraph overview
- **Key points** — 5-8 bullet points
- **Action items** — concrete takeaways

Output language follows the global setting in the sidebar.

---

## Chapters

Generates YouTube-style chapter markers.

- First chapter is always pinned to `0:00`
- Click **Copy YouTube format** to paste directly into a YouTube description:
  ```
  0:00 Introduction
  1:23 Main topic
  4:56 Key insight
  ```

---

## YouTube Content Pack

One-shot generation of everything you need for a YouTube upload:

- **5 title suggestions** — hook-style, under 70 characters
- **Full description** — intro + content overview + call to action
- **15-20 SEO tags** — relevant keywords

Click **Copy all** to get everything in a ready-to-paste format.

---

## Viral Clips

Finds the best 15-60 second segments for short-form content (YouTube Shorts, TikTok, Reels).

Each clip includes:
- **Timestamp range** — precise start and end times
- **Hook** — why this moment is engaging
- **Caption** — suggested social media caption

---

## Blog Article

Converts the transcript into a structured written article.

- Title + 3-7 sections with headings and prose paragraphs
- Removes verbal fillers, repetitions, and spoken-language artifacts
- Click **Copy markdown** for a ready-to-publish format

---

## Settings

Manage API keys for cloud providers.

- Keys are stored in the **OS keychain** (macOS Keychain, Windows Credential Manager)
- The status grid shows which providers are configured and available
- **Refresh status** to re-check provider availability

---

## Tips

- **YouTube URLs** — paste any YouTube link in the source picker. The video is downloaded via yt-dlp and cached by video ID.
- **Returning to a file** — if you've transcribed a file before, the transcript loads automatically from cache when you select it again.
- **Provider switching** — changing the sidebar AI provider instantly switches all features. Try MLX for free local processing, or OpenAI for higher quality.
- **Language flexibility** — set the output language in the sidebar. All AI features respect it. Translate has its own target language field for BCP-47 codes.
