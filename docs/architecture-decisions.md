# Architecture Decisions (v2 — rebuild scope)

> Living ADR log for the revised rebuild. Original reverse-engineered docs (`00-overview.md` … `12-ui-structure.md`) describe v1 (the reverse-engineered app) and remain the implementation reference for feature behavior. This file captures where v2 intentionally diverges.

## Context

v1 (reverse-engineered from `/Applications/My Media Kit`) targets macOS 26+, ships as a monolithic app bundle, uses `@AppStorage` for API keys, bundles `whisper.framework`, supports 4 AI providers including Apple Intelligence, and exports to 4 NLEs including CapCut. Build effort ≈ 33 dev-days.

v2 goal: rebuild the **high-value features** with better architecture + broader OS reach + testability without Xcode, accepting tech/scope changes.

---

## ADR-001 — SPM-first monorepo instead of Xcode project

**Decision:** `Package.swift` is the source of truth. Libraries are SPM targets; the app is an `executableTarget`. No `.xcodeproj` checked in.

**Why:**
- All pure-logic kits (`CreatorCore`, `SilenceKit`, `NLEKit`, `AIKit`) build and test via `swift build` / `swift test` without Xcode — no 15 GB install required for CI or agent-driven development.
- Xcode still opens `Package.swift` natively for UI work on the app target.
- Removes `.pbxproj` merge-conflict class entirely.

**How to apply:** When adding a module, add a `.target(...)` entry + `Sources/<Name>/`. When adding a test, add a `.testTarget(...)` + `Tests/<Name>Tests/`.

**Trade-off:** Executable SPM target is not a proper `.app` bundle (no Info.plist, no entitlements, no code signing). For notarized release we still need an Xcode app wrapper — add in Phase 9 or later.

---

## ADR-002 — Drop minimum OS from macOS 26 to macOS 14

**Decision:** `platforms: [.macOS(.v14)]`.

**Why:**
- macOS 26 is barely shipped; most creators run 14 (Sonoma) or 15 (Sequoia).
- Only `FoundationModels` (Apple Intelligence) needs 26+. Everything else — AVFoundation, Accelerate/vDSP, SwiftUI `@Observable`, Swift Concurrency, WhisperKit — ships on 14.
- Apple Intelligence is one of 4 providers; losing it is acceptable trade.

**How to apply:** Use `#available(macOS 26, *)` guards if/when the Apple Intelligence provider is added (see ADR-006).

---

## ADR-003 — WhisperKit (Core ML) instead of bundled whisper.cpp

**Decision:** `TranscriptionKit` depends on `argmaxinc/WhisperKit` via SPM.

**Why:**
- Zero build-time dependency (no cmake, no xcframework dance).
- Core ML backend auto-uses Metal; no manual `WHISPER_METAL=1`.
- Model downloads + caching handled by WhisperKit.
- v1 shipped its own `whisper.framework` because it predates WhisperKit being production-ready; not the case today.

**Trade-off:** WhisperKit API differs from raw `whisper_full` — word-timestamp extraction uses WhisperKit's `DecodingResult.segments[].words[]` instead of walking token data ourselves. The word-timestamp accuracy should be equivalent; verify in Phase 4.

**How to apply:** `TranscriptionKit.transcribe(url:)` returns `[TranscriptionSegment]` with `WordTimestamp` — identical public shape as v1, WhisperKit hidden behind the facade.

---

## ADR-004 — Keychain for API keys, not @AppStorage

**Decision:** `AIKit.SecretStore` wraps Security framework Keychain APIs. `AIServiceManager` reads/writes keys via `SecretStore`. `@AppStorage` only holds non-sensitive settings (model name, temperature, batch duration).

**Why:**
- v1 ships API keys in `~/Library/Preferences/*.plist` in plaintext. User-flagged security issue in doc audit.
- Keychain is per-app, encrypted, synchronized across devices if iCloud Keychain is on.
- API surface is trivial: `set(_:forKey:)`, `get(_:)`, `delete(_:)`.

**How to apply:** Provider init pulls key by `SecretStore.get("ai.provider.claude.apiKey")`. Settings UI writes via `SecretStore.set(...)`.

---

## ADR-005 — Modular kits for extensibility

**Decision:** Split into 6 libraries + 1 executable:

| Module | Pure Swift | Depends on | Role |
|---|---|---|---|
| `CreatorCore` | ✓ | — | Domain models (CutRegion, TranscriptionSegment, etc.) |
| `SilenceKit` | ✓ | CreatorCore | vDSP DSP + region building |
| `NLEKit` | ✓ | CreatorCore | FCPXML + xmeml builders |
| `AIKit` | ✓ | CreatorCore | Provider protocol + Claude/OpenAI/Gemini + SecretStore |
| `CoreMediaKit` | ✗ (AVFoundation) | CreatorCore | Audio extract, composition build, export |
| `TranscriptionKit` | ✗ (WhisperKit) | CreatorCore, CoreMediaKit | Whisper wrapper |
| `My Media KitApp` | ✗ (SwiftUI) | all kits | UI, ViewModels, assembly |

**Why:** Adding a feature = new file in correct kit. Adding a provider = new file in AIKit conforming to protocol. Adding an NLE format = new builder in NLEKit. Zero cross-kit churn.

**Testability:** Pure kits run under `swift test` on any Mac without Xcode. Platform kits compile without running (integration tests require fixtures).

---

## ADR-006 — Apple Intelligence: deferred, not blocked

**Decision:** Do not implement in initial build. Design `AIKit` to accept it later as `AppleIntelligenceProvider` gated by `@available(macOS 26, *)`.

**Why:**
- Niche availability (macOS 26+).
- Cloud providers (Claude, OpenAI, Gemini) cover 100% of functionality with better model quality.
- Zero architectural cost to add later: one file in `Sources/AIKit/Providers/AppleIntelligenceProvider.swift`, one case in the provider enum, one conditional in the settings picker.

**How to apply later:**
```swift
@available(macOS 26, *)
final class AppleIntelligenceProvider: AIProvider {
    // FoundationModels.LanguageModelSession.respond(to:generating:)
}
```
Register in `AIServiceManager` init with runtime check. UI picker hides the option on older OS.

---

## ADR-007 — Drop CapCut and Ad-Compliance VN from initial scope

**Decision:** Both removed from the v2 feature list.

**Why:**
- **CapCut export** writes into a private, undocumented draft folder schema. v1 reverse-engineered it field-by-field with no stability guarantee; schema breaks across CapCut versions. High maintenance, niche audience.
- **Ad Compliance VN** is specific to Vietnamese advertising law (Luật Quảng cáo 2025), requires constantly updated legal prompts, and serves a narrow market. Better as an optional plugin if demand emerges.

**How to apply:** Features not implemented. `NLEKit` ships FCPXML + xmeml only (covers Final Cut Pro + Premiere + Resolve — 95% of pro workflows).

---

## ADR-008 — Translate: feature-level, not new module

**Decision:** When Translate is added, implement as a ViewModel + View inside `My Media KitApp`, reusing existing `AIKit` for prompt execution. `CreatorCore` gains `TranslateLanguage` enum and `TranslatedSegment` struct.

**Why:**
- Translate is architecturally identical to Summary/Chapter/Metadata — structured output from an AI provider. No new kit needed; the AIKit protocol already handles it.
- Batching with context-overflow fallback (split in half on token limit) belongs in the feature's ViewModel, not in a shared kit.

**How to apply later:**
1. `CreatorCore/TranslateModels.swift` — language enum + segment struct.
2. `My Media KitApp/Features/Translate/TranslateViewModel.swift` — runs `provider.complete(...)` with `TranslatedBatch` schema.
3. `My Media KitApp/Features/Translate/TranslateView.swift` — 2-column SwiftUI view.
4. No Package.swift change.

**Alternative rejected:** Dedicated `TranslateKit` — considered, rejected because it would be a 1-file module that only depends on what a feature ViewModel already has.

---

## ADR-009 — Testing strategy

**Decision:**
- Pure kits (`CreatorCore`, `SilenceKit`, `NLEKit`, `AIKit`) use Swift Testing framework (`import Testing`, `@Test`) with 80%+ coverage goal.
- Platform kits (`CoreMediaKit`, `TranscriptionKit`) have compile tests + integration tests gated behind an env var (skipped in default `swift test` runs).
- UI is smoke-tested manually; no snapshot tests initially.

**Why:** Enables `swift test` to run in under 5s on any Mac, making TDD practical for DSP and prompt logic. Integration tests that need real audio/video files or network calls are opt-in.

**How to apply:** `Tests/SilenceKitTests/` with fixture signals generated in code (silent, speech, mixed). `Tests/NLEKitTests/` with golden-file comparison against sample XMLs in `docs/_raw/`.

---

## ADR-010 — Feature priority (what ships in v2 MVP vs later)

**P0 (MVP):**
1. Whisper transcription (WhisperKit, word timestamps, TXT/SRT export)
2. Silence detection (vDSP, live preview)
3. AutoCut timeline + manual edit + direct export (passthrough + re-encode)

**P1 (MVP+):**
4. AI providers (Claude, OpenAI, Gemini) + Keychain
5. Filler detection + AI free-form prompt
6. FCPXML export (single format, validated)

**P2 (post-MVP):**
7. AI Summary + Chapters for YouTube creators
8. Translate (see ADR-008)
9. xmeml export (Premiere / Resolve)

**Deferred / dropped:**
- Apple Intelligence provider (ADR-006)
- CapCut export (ADR-007)
- Ad Compliance VN (ADR-007)
- Duplicate detection (secondary; Filler + AI Prompt covers 80%)
- Thumbnail / Audio extractors (trivial utilities, add on demand)

---

## ADR-011 — Pivot to Tauri v2 (Rust + HTML/JS) cross-platform

**Decision:** Abandon the Swift/SPM scaffold after the first phase. Rebuild on **Tauri v2** with a Cargo workspace backend (`src-tauri/crates/*`) and a plain HTML/CSS/JS frontend.

**Why:**
- v2 must ship on **macOS ARM + macOS Intel + Windows** from a single codebase. Swift/SwiftUI is macOS-only; porting later would cost more than pivoting now.
- SPM pure-kit tests still run offline, but the app target forces Xcode for any UI work. Tauri's Rust backend runs anywhere with `cargo`.
- Everything v1 relied on macOS frameworks (AVFoundation, Accelerate/vDSP, Core ML) has a cross-platform substitute: `ffmpeg` sidecar, plain Rust f32 loops, and the provider abstraction hides LLM backends.
- Single stack shared with a sibling project (`my-translator`) → shared knowledge, shared build conventions.

**Scope changes it invalidates:**
- ADR-001 (SPM monorepo) → replaced by Cargo workspace under `src-tauri/`.
- ADR-003 (WhisperKit SPM) → replaced by `mlx-whisper` sidecar (macOS) and `whisper-rs` behind a cargo feature flag (Windows/Linux).
- ADR-004 (Swift Keychain) → replaced by the `keyring` crate (Apple Keychain / Windows Credential Manager / Linux Secret Service).
- ADR-005 (SwiftUI kits) → replaced by 5 Cargo crates: `creator-core`, `media-kit`, `transcription-kit`, `ai-kit`, `content-kit`. (Earlier `silence-kit` + `nle-kit` removed when AutoCut was scoped out for v1.)
- ADR-009 (Swift Testing) → replaced by Rust `#[test]` + `cargo test`.

**Kept as-is:**
- ADR-002 — broad OS reach (now macOS 13+ via Tauri + Windows 10+ / Linux modern).
- ADR-006 — Apple Intelligence still deferred. Position tightened by ADR-012 below.
- ADR-007 — CapCut / Ad Compliance still dropped.
- ADR-008 — Translate still a feature, not a kit.
- ADR-010 — feature priority list still valid.

**How to apply:** all new work lands under `src-tauri/` (Rust) or `src/` (frontend). Swift references in older ADRs are historical only.

---

## ADR-012 — MLX-first on Apple Silicon; cloud APIs deferred to post-MVP

**Decision:** On Apple Silicon (`target_os = "macos" && target_arch = "aarch64"`), the **default transcription backend is `mlx-whisper`** and the **default LLM backend is `mlx-lm.server`** — both already installed on the primary dev machine. Cloud provider integrations (Claude, OpenAI, Gemini) become **post-MVP** features and will ship only after the MLX path is fully validated end-to-end.

**Why:**
- User has an Apple Silicon dev/test machine with 6 MLX LLMs (1.6 GB → 7.7 GB) + `mlx-whisper-large-v3-turbo` already downloaded. Full feature test costs **zero dollars** and no network.
- MLX native Metal backend beats `whisper-rs` + whisper.cpp on Apple Silicon and unblocks Phase 4 without touching cmake.
- `mlx-lm.server` exposes an **OpenAI-compatible HTTP API** on `127.0.0.1:8080`. Our existing `OpenAiProvider` already speaks this protocol — we literally just point it at localhost. Zero new provider implementation needed on the happy path.
- Focusing on MLX first lets the UI / feature surface solidify before we burn tokens on cloud providers for quality comparisons.
- Cloud providers remain in the codebase (already implemented + tested) but are moved out of the "required for MVP" column until the MLX path ships.

**Scope implications:**
- Transcription MVP backend = `mlx-whisper` via subprocess, model default `mlx-community/whisper-large-v3-turbo`.
- LLM MVP backend = `mlx-lm.server` reusing `OpenAiProvider` with `base_url = http://127.0.0.1:8080/v1`.
- Per-feature default models (on MLX):
  - Filler detection → `Qwen2.5-3B-Instruct-4bit` (fast, bilingual VN/EN)
  - AI Prompt cut → `Qwen2.5-14B-Instruct-4bit` (needs reasoning)
  - Summary → `Qwen2.5-7B-Instruct-4bit` (balance)
  - Chapters → `Qwen2.5-7B-Instruct-4bit` or `Qwen3-8B-4bit`
- Cloud providers stay in `ai-kit` (Claude / OpenAI / Gemini / Ollama) but are **not** the default; they ship as "BYOK" options in a later phase.
- Windows / Intel Mac path: MLX not available → fall back to cloud APIs OR `whisper-rs` (both already in code). Non-Apple-Silicon users need at least one API key or a local whisper model file.

**How to apply:**
- New sidecar wrapper lives under `transcription-kit` as `MlxWhisperTranscriber`, behind `cfg(all(target_os = "macos", target_arch = "aarch64"))`. Falls back to `NullTranscriber` on other platforms until `whisper-rs` wiring lands.
- Local MLX LLM provider lives under `ai-kit` as `MlxLmProvider` — a thin wrapper that spawns `mlx_lm.server` if not already running and points `OpenAiProvider` at it.
- `ProviderRegistry` registers `MlxLm` first on Apple Silicon, before cloud providers.
- Settings UI: on Apple Silicon, show "MLX (local)" as the default provider with a green dot as soon as mlx_lm server is reachable. Cloud API key fields collapsed into "Advanced".

**Trade-offs:**
- Requires Python env with `mlx-lm` + `mlx-whisper` installed. Not bundled — user must install (single `pip install` each).
- Python sidecar is heavier than a pure Rust lib (~200 MB resident per server instance). Acceptable given "local, private, free" value proposition.
- `mlx_lm.server` does not support native structured output the way Claude tool-use does. Our schema enforcement falls back to "prompt begs for JSON" — less reliable than cloud strict mode. Needs JSON-repair logic in content-kit (already partially implemented for Ollama).
- Qwen3-8B newer generation; occasional tool-calling drift noted in community. Fall back to Qwen2.5 family when issues surface.

**Open-ended:**
- Whether to auto-spawn `mlx_lm.server` from the app or require user to start it manually. MVP: auto-spawn on first LLM call, kill on app exit.
- Model switching UX: swap requires restarting the server. Possibly pre-load multiple models as separate port instances if RAM allows.
- How to handle Windows / Intel users in the installer flow when MLX is unavailable — probably gate the whole "AI features" section until they configure cloud or local whisper.

---

## Open questions

- Whether the first public build ships with cloud providers visible in Settings at all (MLX-only MVP) or hidden behind a "Show advanced" toggle.
- Whether we ever bring Apple Intelligence into scope given it shares the macOS-only gate and MLX already covers local inference well.
- Bundling strategy for non-Apple-Silicon users: ship a whisper.cpp binary sidecar, ship a cmake-free `whisper-rs` crate, or require cloud-only on those platforms.
- Installer experience: Python dep for MLX is a papercut; consider shipping a self-contained `uv` venv alongside the app.
