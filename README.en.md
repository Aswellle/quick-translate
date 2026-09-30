<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="720">

# QuickTranslate

### Copy, and it's translated.

**See unfamiliar text, `Ctrl+C`. A translation popup appears next to your cursor.**

No window switching, no browser, no paste box. Your hands never leave the keyboard.

[中文](README.md) · [English](README.en.md)

[![CI](https://github.com/Aswellle/quick-translate/actions/workflows/ci.yml/badge.svg)](https://github.com/Aswellle/quick-translate/actions/workflows/ci.yml)
[![Release](https://github.com/Aswellle/quick-translate/actions/workflows/release.yml/badge.svg)](https://github.com/Aswellle/quick-translate/actions/workflows/release.yml)
[![Version](https://img.shields.io/github/v/release/Aswellle/quick-translate)](https://github.com/Aswellle/quick-translate/releases)
[![Downloads](https://img.shields.io/github/downloads/Aswellle/quick-translate/total)](https://github.com/Aswellle/quick-translate/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey)](https://github.com/Aswellle/quick-translate/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

![Tauri](https://img.shields.io/badge/Tauri-2.x-FFC131?logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-DEA584?logo=rust)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)
![TypeScript](https://img.shields.io/badge/TypeScript-strict-3178C6?logo=typescript&logoColor=white)

</div>

---

## Table of Contents

- [Introduction](#introduction)
- [Features](#features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Controls & Shortcuts](#controls--shortcuts)
- [Translation Provider Configuration](#translation-provider-configuration)
- [Reliability & Stability](#reliability--stability)
- [Building from Source](#building-from-source)
- [Technical Architecture](#technical-architecture)
- [FAQ](#faq)
- [License](#license)

---

## Introduction

QuickTranslate is a **copy-to-translate** utility for Windows and macOS. It lives in your system tray, monitoring your clipboard — whenever you copy text, a translation popup appears near your cursor.

> It removes those ten-second cycles of "switch to browser → open translator → paste → wait → switch back → find where you were." Dozens of times per document. What's really consumed isn't time — it's the focus you just built up.

It is designed to the standard of always-on software: network outages, provider failures and clipboard contention never take it down; recoverable faults heal themselves in the background — no restarts, no re-configuration.

**Use cases**: Reading English documents and papers, handling foreign emails, browsing overseas content, terminal compiler errors, English terms in Figma/Jira... anywhere you can select and copy text.

---

## Features

### Core Experience

| Feature | Description |
|:--|:--|
| **Copy to Translate** | Clipboard monitoring triggers translation — no hotkeys, no window switching |
| **Smart Popup Positioning** | DPI-aware, multi-monitor support; positions by real window size and monitor work area; auto-flips to stay on screen |
| **Non-activating Popup** | Doesn't steal foreground focus; closes automatically when you switch to another app |
| **Five Translation Providers** | DeepL / Tencent / Baidu / Youdao / Google — configure multiple for automatic failover |
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
| **Automatic Provider Failover** | When the primary provider fails or gets rate-limited, traffic switches to the next one automatically — and switches back once it recovers. Zero manual intervention |
| **Live Health Panel** | Settings and tray show each provider's current state (healthy / recovering / rate-limited / auth failed / quota exhausted) |
| **Offline Cache Fallback** | While offline, previously translated content still displays (labeled as from local cache); translation resumes automatically when the network returns |
| **Graceful Degradation** | Local data trouble never takes translation down — history degrades temporarily and the app keeps running, no crash, no exit |
| **Credential Protection** | Saved API keys are automatically salvaged if the database is corrupted — no need to re-enter provider credentials after a repair |
| **Latest-Wins Results** | Copy several passages in quick succession — only the last one is ever shown; a late stale result can never overwrite it |
| **Clipboard Self-healing** | Clipboard contention or handle errors trigger automatic backoff recovery — without re-popping old content as if it were a fresh copy |

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

Five providers supported; configure multiple for automatic failover. **Failover order**: DeepL → Tencent → Baidu → Youdao → Google (last resort).

- Enter credentials in Settings → Providers; use the "Verify" button to validate instantly (no quota consumed).
- Credentials are stored locally only, encrypted in the database.
- Switch providers anytime; when the primary fails, traffic falls over to the next one automatically — and back once it recovers, no manual intervention needed.
- Each provider card shows its live health status — rate-limited, auth failure, quota exhausted and other causes are visible at a glance, so you never have to guess what went wrong.

Step-by-step credential setup for each provider is available in the in-app settings panel.

---

## Reliability & Stability

QuickTranslate is built to the standard of **long-running resident software**: failures of external services, the network and local data are isolated to their own scope, and recoverable faults are handled automatically in the background — users barely notice.

| Scenario | What you experience |
|:--|:--|
| Provider outage / rate limiting | Automatic switch to the next available provider — translations keep coming; the failed one rejoins when it recovers |
| Network offline | Popup, settings and history all keep working; previously translated content is served from local cache; the next copy after reconnection just works |
| Clipboard held by another app | Monitoring backs off and retries automatically; recovered without re-popping old content as a fresh translation |
| Local database trouble | Translation continues unaffected; history degrades temporarily with honest status; saved API keys are salvaged automatically — no re-configuration needed |
| Rapid consecutive copies | Only the last copy is ever shown; stale translations never overwrite fresh ones |
| Long-running sessions | Stable memory and resource footprint; bounded write queues — nothing grows unbounded with usage |

**Engineering assurance**: all core fault paths are covered by automated tests (200+ unit tests). CI runs frontend builds, Rust tests and static checks on Linux / Windows / macOS; release artifacts are built and signed automatically by GitHub Actions.

---

## Building from Source

### Prerequisites

- Node.js ≥ 20 (CI runs on Node 22)
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

Push a `v*` tag to trigger the [release workflow](.github/workflows/release.yml), which builds and publishes signed installers (Windows MSI / NSIS, macOS DMG) to GitHub Releases and generates the auto-update manifest.

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
| Database | SQLite (rusqlite, WAL mode) |
| Desktop | System tray, clipboard monitor, global updater, auto-start |

**Core flow**: Clipboard monitor detects copy → the monitor supervisor keeps it alive → the request coordinator cancels stale requests and orchestrates the popup → the translation engine walks the failover chain (with circuit breaking and request budgets) → local cache on total failure → result pushed to the frontend.

**Data flow**: Frontend invokes Tauri commands via `invoke()`; backend pushes results via Tauri events; history and cache are persisted asynchronously by a dedicated bounded-queue worker — a slow database never delays the translation you see.

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
Configure multiple providers with failover enabled — when the primary fails, traffic switches to the next automatically. Each provider card in Settings shows its live health status (recovering / rate-limited / auth failed / quota exhausted), so the cause is visible at a glance. On network interruption, previously translated content is served from local cache.

**Something wrong with local data — does translation still work?**
Yes. When the database misbehaves, translation keeps running and history degrades temporarily with honest status. Saved provider credentials are salvaged automatically — no re-configuration after the data is repaired.

**Bug reports or feature suggestions?**
Welcome in [Issues](https://github.com/Aswellle/quick-translate/issues).

---

## License

This project is open source under the **[MIT License](LICENSE)** — free to use, modify and distribute, including for commercial purposes.

> The copyright holder reserves the right to modify or supplement the license terms for future releases, as needed. Each release is governed by the license published with it; copies already obtained are unaffected.

---

<div align="center">

**If it saved you those ten seconds, ⭐ it so more people can find it.**

[⬇ Download Latest](https://github.com/Aswellle/quick-translate/releases/latest) · [🐛 Report Issue](https://github.com/Aswellle/quick-translate/issues) · [📋 Changelog](https://github.com/Aswellle/quick-translate/releases)

<sub>QuickTranslate · Copyright © 2026 welle</sub>

</div>
