<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="720">

# QuickTranslate

### Copy, and it's translated.

**See unfamiliar text, `Ctrl+C`. A translation popup appears next to your cursor.**

No window switching, no browser, no paste box. Your hands never leave the keyboard.

[中文](README.md) · [English](README.en.md)

[![Download](https://img.shields.io/badge/⬇_Download-Windows_%7C_macOS-2563eb?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)
[![Version](https://img.shields.io/github/v/release/Aswellle/quick-translate?style=for-the-badge&label=Version&color=555)](https://github.com/Aswellle/quick-translate/releases)
[![Platform](https://img.shields.io/badge/Platform-Windows_10/11_%7C_macOS-2563eb?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)

</div>

---

## Table of Contents

- [Introduction](#introduction)
- [Features](#features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Controls & Shortcuts](#controls--shortcuts)
- [Translation Provider Configuration](#translation-provider-configuration)
- [Reliability Mechanisms](#reliability-mechanisms)
- [Building from Source](#building-from-source)
- [Technical Architecture](#technical-architecture)
- [FAQ](#faq)
- [License](#license)

---

## Introduction

QuickTranslate is a **copy-to-translate** utility for Windows and macOS. It lives in your system tray, monitoring your clipboard — whenever you copy text, a translation popup appears near your cursor.

> It removes those ten-second cycles of "switch to browser → open translator → paste → wait → switch back → find where you were." Dozens of times per document. What's really consumed isn't time — it's the focus you just built up.

**Use cases**: Reading English documents and papers, handling foreign emails, browsing overseas content, terminal compiler errors, English terms in Figma/Jira... anywhere you can select and copy text.

---

## Features

### Core Experience

| Feature | Description |
|:--|:--|
| **Copy to Translate** | Clipboard monitoring triggers translation — no hotkeys, no window switching |
| **Smart Popup Positioning** | DPI-aware, multi-monitor support; positions by real window size and monitor work area; auto-flips to stay on screen |
| **Non-activating Popup** | Doesn't steal foreground focus; auto-switches away when you switch to another app |
| **Five Translation Providers** | DeepL / Tencent / Baidu / Youdao / Google — configure multiple, auto-fallback in order |
| **Multi-language** | 11 languages supported; auto-detect source, selectable target (default: Simplified Chinese) |

### Provider Comparison

| Provider | Requires Setup | Free Quota | Highlights |
|:--|:--|:--|:--|
| **Google Translate** | ❌ None | Unlimited | Works out of the box as fallback |
| **DeepL** | API Key | 500K chars/month | Best-in-class for European languages |
| **Tencent TMT** | SecretId + SecretKey | 5M chars/month | Excellent Chinese-English |
| **Baidu Translate** | APP ID + Secret Key | 1M chars/month | Strong with CJK languages |
| **Youdao** | App ID + App Secret | Trial credit for new users | Broad language support |

### Reliability

| Feature | Description |
|:--|:--|
| **Circuit Breaker & Auto-fallback** | Provider that fails consecutively gets circuit-broken — requests skip to the next one; auto-probes for recovery after cooldown |
| **Health Awareness** | Real-time availability tracking per provider; live status shown in tray menu |
| **Offline Cache Fallback** | When all providers are unreachable, served from local cache (labeled) if available; otherwise notifies network interruption |
| **Request Policy** | Per-provider rate limits and total budget to avoid throttling or quota exhaustion |
| **Clipboard Self-healing** | Monitor auto-restarts with backoff if the clipboard handle dies — monitoring never drops |

### Data & Security

| Feature | Description |
|:--|:--|
| **Local History** | SQLite storage with search, starring (permanently preserved), deletion, and export |
| **Encrypted Storage** | API keys encrypted with AES-256-GCM before disk write; key bound to current machine |
| **FIFO Auto-cleanup** | When limit exceeded, oldest un-starred records deleted first; starred records are never auto-deleted |

### Miscellaneous

| Feature | Description |
|:--|:--|
| **Themes** | Dark / Light / Follow system |
| **Silent Updates** | Auto-checks on startup; downloads and installs in background; one-click upgrade on notification |
| **Lightweight** | < 50MB idle RAM, ~0% CPU, ~5MB installer |
| **Auto-start** | Launch on system boot |
| **Onboarding Wizard** | First-run guide to select provider and target language; skippable to use Google fallback |

Other details: PDF line-join fix, one-click copy translation/source, same-language passthrough (no redundant translation), 5000-char single-translation limit, one-click credential verification, tray menu toggle for clipboard monitoring.

---

## Installation

### Windows

**System requirements**: Windows 10 / 11 (x64), WebView2 runtime (included with Win11; installer prompts if missing).

Download from the [Releases page](https://github.com/Aswellle/quick-translate/releases/latest):

- **`.msi`** — Recommended, standard installer wizard
- **`.exe`** — NSIS portable installer

~5MB installer; launch from Start menu or desktop shortcut after installation.

### macOS

**System requirements**: macOS 13+ (Apple Silicon).

Download from the [Releases page](https://github.com/Aswellle/quick-translate/releases/latest):

- **`.dmg`** — Disk image, drag into Applications folder.

---

## Quick Start

**1. First launch (30 seconds)**

The onboarding wizard appears on first run: choose target language. You can skip provider setup — the built-in Google provider works out of the box.

**2. Copy some text**

Select and copy foreign text in any application (`Ctrl+C`).

**3. Read the translation, then dismiss**

The popup appears near your cursor. Press `Space` or `Esc` to close it when done.

After that, QuickTranslate sits quietly in your system tray. Right-click the tray icon to open settings, browse history, or temporarily disable clipboard monitoring.

---

## Controls & Shortcuts

| Action | Effect |
|:--|:--|
| `Space` / `Esc` | Close popup |
| Click outside popup | Close popup |
| `Enter` | Collapse / expand popup |
| Red dot | Close |
| Yellow dot | Collapse to title bar (keep for later) |
| Green dot | Toggle standard / reading viewport width |

> Popup records clipboard state on close to avoid re-triggering; only a genuine re-copy of the same text triggers a new translation.

---

## Translation Provider Configuration

Five providers supported; configure multiple for automatic fallback. **Fallback order**: DeepL → Tencent → Baidu → Youdao → Google (last resort).

- Enter credentials in Settings → Providers; use the "Verify" button to validate instantly (no quota consumed).
- Credentials are stored locally only, encrypted in the database.
- Switch providers anytime; when the primary fails, it's automatically circuit-broken and falls back to the next — no manual intervention needed.

Step-by-step credential setup for each provider is available in the in-app settings panel.

---

## Reliability Mechanisms

QuickTranslate includes multi-layer reliability guarantees to keep translations stable and monitoring alive:

**Circuit Breaker**: Each provider tracks health independently. After consecutive failures exceed a threshold, the circuit opens (Open) — subsequent requests skip it. After cooldown, it enters half-open (HalfOpen) to probe; success restores it, failure extends cooldown. Cooldown follows exponential backoff (10s → 30s → 2min → capped at 10min) with ±20% jitter to avoid clustering.

**Request Policy**: Per-provider rate limits and total call budgets prevent high-frequency translations from triggering provider throttling or quota exhaustion.

**Translation Cache**: Results cached in local SQLite (max 2000 entries, 30-day TTL, 1000-char limit per entry). Cache is only consulted when all providers are unreachable — online results are always fresh.

**Request Coordinator**: A generation-gate mechanism prevents out-of-order result overwrites from concurrent translation requests. Each new request gets an incrementing generation ID; results verify they're still current before delivery, otherwise discarded.

**Clipboard Supervisor**: If the monitor thread exits due to clipboard handle errors, the supervisor automatically rebuilds and restarts it with backoff (250ms → 30s, ±20% jitter). Restart count is included in health snapshots viewable from the tray menu.

**Bounded Persistence**: History and cache writes go through a single bounded queue + one dedicated worker, fully decoupled from the translation critical path. Queue-full drops with alerting — never blocks the user's view of translations.

---

## Building from Source

### Prerequisites

- Node.js ≥ 18
- Rust stable ≥ 1.75
- Tauri CLI 2.x

### Development Mode

```bash
npm install          # Install dependencies
npm run tauri dev    # Start Tauri dev mode (with Vite HMR)
```

Frontend-only debugging (no Tauri APIs):

```bash
npm run dev
```

### Production Build

```bash
npm run tauri build  # Output to src-tauri/target/release/
```

### Code Checks

```bash
npx tsc --noEmit                              # TypeScript type check
cd src-tauri && cargo fmt --check             # Rust format check
cd src-tauri && cargo clippy -- -D warnings   # Rust lint
```

### Release

Push a `v*` tag to trigger the [release workflow](.github/workflows/release.yml), which builds and publishes a signed MSI installer to GitHub Releases and generates the auto-update manifest.

```bash
git tag v0.3.0
git push origin v0.3.0
```

---

## Technical Architecture

| Layer | Technology |
|:--|:--|
| Frontend | React 19 + TypeScript + Vite + Tailwind CSS + Zustand |
| Backend | Rust (Tauri 2) + Tokio async runtime |
| Database | SQLite (rusqlite, WAL mode, built-in FTS5) |
| Desktop | System tray, clipboard monitor, global updater, auto-start |

**Core flow**: Clipboard monitor detects copy → `ClipboardSupervisor` ensures liveness → `TranslationCoordinator` generates generation gate, cancels stale requests → orchestrates popup positioning and display → translation engine invokes fallback chain (`TranslationEngine` + Provider pattern + Circuit Breaker + Request Policy) → cache fallback on total failure → event pushed to frontend.

**Data flow**: Frontend invokes Tauri commands via `invoke()`; backend pushes results via Tauri events (`translation-result`, `translation-loading`, `translation-error`). Config and history are asynchronously persisted by the bounded persistence worker.

---

## FAQ

**Copied but no popup appeared?**
Right-click the tray icon to check if clipboard monitoring was disabled. Note: in many terminals, `Ctrl+C` sends an interrupt signal, not copy — in Windows Terminal, select text first.

**Can I use a hotkey trigger instead of auto-popup?**
Currently copy-triggered. Disable monitoring anytime from the tray menu when you don't want interruptions.

**Does it upload my data secretly?**
Only the text you copy is sent to your own configured translation provider (required for translation). History and API keys are stored locally; keys are encrypted.

**Resource usage?**
Under 50MB idle RAM, ~0% CPU, ~5MB installer.

**Translation inaccurate / provider down?**
Configure multiple providers with fallback enabled. When the primary fails, it's automatically circuit-broken and switches to the next. The tray menu shows real-time health status for each provider. On network interruption, previously translated content is served from local cache.

**Bug reports or feature suggestions?**
Welcome in [Issues](https://github.com/Aswellle/quick-translate/issues).

---

## License

This is **proprietary software** for personal, non-commercial use only. See [LICENSE](LICENSE) for full terms.

---

<div align="center">

**If it saved you those ten seconds, ⭐ it so more people can find it.**

[⬇ Download Latest](https://github.com/Aswellle/quick-translate/releases/latest) · [🐛 Report Issue](https://github.com/Aswellle/quick-translate/issues) · [📋 Changelog](https://github.com/Aswellle/quick-translate/releases)

<sub>QuickTranslate · Copyright © 2026 welle</sub>

</div>
