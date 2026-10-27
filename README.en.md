<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="100%">

# QuickTranslate

**Copy. That's the translation. Any app, any language.**

Select text anywhere on screen, press `Ctrl+C` (or `⌘C` on macOS), and a translation popup opens beside your cursor.

No window switching, no browser, no paste box — your hands never leave the keyboard.

[中文](README.md) · [English](README.en.md)

[![CI](https://github.com/Aswellle/quick-translate/actions/workflows/ci.yml/badge.svg)](https://github.com/Aswellle/quick-translate/actions/workflows/ci.yml)
[![Version](https://img.shields.io/github/v/release/Aswellle/quick-translate)](https://github.com/Aswellle/quick-translate/releases)
[![Downloads](https://img.shields.io/github/downloads/Aswellle/quick-translate/total)](https://github.com/Aswellle/quick-translate/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey)](https://github.com/Aswellle/quick-translate/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

![Tauri](https://img.shields.io/badge/Tauri-2.x-FFC131?logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-DEA584?logo=rust)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)
![TypeScript](https://img.shields.io/badge/TypeScript-strict-3178C6?logo=typescript&logoColor=white)

**[⬇ Download Latest](https://github.com/Aswellle/quick-translate/releases/latest)** · [🐛 Report a Bug](https://github.com/Aswellle/quick-translate/issues) · [💡 Request a Feature](https://github.com/Aswellle/quick-translate/issues) · [📋 Changelog](https://github.com/Aswellle/quick-translate/releases)

<sub>Windows 10/11 · macOS 13+ (Apple Silicon) · ~5 MB installer · under 50 MB idle RAM</sub>

</div>

---

## The Problem It Solves

Translation itself takes two seconds. The switching is what costs you.

> Select text → switch to browser → open translator → paste → wait → read → switch back → find the line you were on.

A dozen times per document. What gets consumed isn't just those ten seconds — it's the **reading continuity** that every context switch breaks.

QuickTranslate compresses that chain into a single action: **copy**. The translation appears beside your cursor, you press `Space`, and you never left the window you were in.

It isn't built for "learning English". It's built for **any moment you need to read across a language boundary**:

- Reading foreign papers, docs, RFCs, release notes
- Handling emails, Slack / Teams messages, support tickets in another language
- Subtitles, social posts, in-game text you can select and copy
- Terminal errors, stack traces, CI logs
- Foreign field names and descriptions in Figma / Jira / Notion
- Japanese, Korean, German, French, Russian, Arabic — any language you don't yet read
- And the **reverse direction**: write in your language, translate, paste into an email or issue

If it can be selected and copied on screen, QuickTranslate works on it.

---

## Why Not Just Use a Web Translator

| | Web translators / browser extensions | QuickTranslate |
|:--|:--|:--|
| **Scope** | Inside the browser only | **Any application**: editors, terminals, PDF readers, chat, design tools |
| **Trigger** | Open a page / select / right-click | **Copy** — a habit you already have, zero learning curve |
| **Interruption** | Window or tab switching | **No switching**; the popup never steals focus |
| **On failure** | Blank page or an error | **Automatic failover** to the next provider; local cache when offline |
| **Privacy** | Text passes through a third-party site | Sent only to **providers you configure yourself**; keys encrypted locally |

In one line: web translators serve "I'm browsing a web page." QuickTranslate serves "I'm doing something else and just need the meaning right now."

---

## Up and Running in 30 Seconds

**1. Install and launch** (~30 seconds on first run)

The installer is about 5 MB. The first launch opens an onboarding wizard — pick your target language. You can skip provider setup entirely: the built-in Google provider needs no configuration.

**2. Copy text in any application**

`Ctrl+C` on Windows, `⌘C` on macOS.

**3. Read it, dismiss it**

The popup appears next to your cursor. Press `Space` or `Esc` and get back to what you were doing.

After that it lives quietly in the system tray. Right-click the tray icon to open settings, browse history, or temporarily pause clipboard monitoring.

> The popup is **non-activating**: it never takes the focus you're typing into, and it closes when you switch to another app. Once you drag it, it remembers where you put it.

---

## What It Does

### Copy-to-Translate

- **No hotkeys** — it reuses the copy action you already perform, so there's nothing new to memorize
- **Non-activating popup** — no focus stealing, auto-closes on app switch, never interrupts typing
- **Smart positioning** — DPI-aware and multi-monitor aware, computed from the real window size and monitor work area; avoids the cursor and flips to stay on screen
- **Position memory** — drag it once and it reopens where you left it; only a fresh copy returns it to the cursor
- **Liquid glass material** — Acrylic on Windows, NSVisualEffectView on macOS, so the popup blurs the real desktop behind it instead of painting a flat panel

### 12 Languages, Both Directions

简体中文 · 繁體中文 · English · 日本語 · 한국어 · Français · Deutsch · Español · Русский · Português · Italiano · العربية

Source language is detected automatically; the target language is selected in Settings (Simplified Chinese by default). If the copied text is already in the target language it passes straight through — no pointless round trip.

### Five Providers, Automatic Failover

| Provider | Setup | Free quota | Best for |
|:--|:--|:--|:--|
| **Google Translate** | None | Unlimited (unofficial endpoint) | Works out of the box as the fallback |
| **DeepL** | API key | 500K chars/month | Best-in-class European languages |
| **Tencent TMT** | SecretId + SecretKey | 5M chars/month | Excellent Chinese ↔ English |
| **Baidu Translate** | APP ID + secret key | 1M chars/month | Strong CJK coverage |
| **Youdao** | App ID + app secret | Trial credit for new users | Broad language support |

Failover order: **DeepL → Tencent → Baidu → Youdao → Google**. Configure one, or all of them.

- When the primary provider fails or is rate-limited, traffic moves to the next one automatically — and moves back once it recovers
- Each provider card in Settings shows live health: healthy / recovering / rate-limited / auth failed / quota exhausted
- The **Verify** button validates credentials instantly — DeepL queries the official usage endpoint (zero quota); Tencent/Baidu/Youdao submit one tiny test translation (~5 chars, counted against the monthly quota)

### History and Privacy

- **Local history** — SQLite storage with search, starring, deletion, and export
- **Built for scanning** — search hits are centered in the truncated text; timestamps read "just now / 12 minutes ago / yesterday"; records are grouped by Today / Yesterday / This Week
- **Encrypted at rest** — API keys are encrypted with AES-256-GCM and bound to the current machine
- **FIFO cleanup** — when the limit is exceeded, the oldest **unstarred** records go first; starred records are never auto-deleted
- **No silent uploads** — only the text you copy goes to the providers you configured. Everything else stays on your machine

### Lightweight by Design

- ~5 MB installer, under 50 MB idle RAM, effectively 0% idle CPU
- Dark / light / follow-system themes
- Launch at login, tray residency, silent auto-update
- 5000-character limit per translation, truncated with a clear notice beyond that

---

## Why It Doesn't Break

QuickTranslate is built to the standard of **long-running resident software**: failures in external services, the network, and local data are isolated to their own scope, and recoverable faults heal in the background.

| What happens | What you see |
|:--|:--|
| Provider outage or rate limiting | Automatic switch to the next available provider; translations keep coming, and the failed one rejoins when it recovers |
| Network offline | Popup, settings and history keep working; previously translated content is served from local cache and labeled as such; the next copy after reconnecting just works |
| Clipboard held by another app | Monitoring backs off and retries; once recovered it won't mistake stale content for a fresh copy and re-pop |
| Local database corruption | Translation keeps running unaffected; history degrades temporarily with an honest status; saved API keys are salvaged automatically, no reconfiguration needed |
| Rapid consecutive copies | Only the last copy is ever shown — a late stale result can never overwrite fresh content |
| Long-running sessions | Stable memory and resource footprint; bounded write queues that don't grow with uptime |

**Engineering assurance**: core fault paths are covered by 220+ unit tests; CI runs frontend builds, Rust tests and static checks (`clippy -D warnings`) on Linux / Windows / macOS; release artifacts are built and signed automatically by GitHub Actions, with the update manifest generated as part of the pipeline.

---

## Installation

### Windows

**Requirements**: Windows 10 / 11 (x64) with the WebView2 runtime (usually present on Win11; the installer offers to install it if missing).

Download from [Releases](https://github.com/Aswellle/quick-translate/releases/latest):

| File | Notes |
|:--|:--|
| `.msi` | Recommended, standard installer wizard |
| `-setup.exe` | NSIS installer |

The installer lets you **choose the install directory** and pick "install for the current user" (no admin rights) or "for all users".

### macOS

**Requirements**: macOS 13+ (Apple Silicon).

Download the `.dmg` from [Releases](https://github.com/Aswellle/quick-translate/releases/latest) and drag it into Applications.

### In-app upgrade

Already on an older version? Open **Settings → About → Check for Updates**. It reports either "You're on the latest version" or "New version x.y.z found", then downloads with live progress and restarts to finish the upgrade.

---

## Controls & Shortcuts

| Action | Effect |
|:--|:--|
| `Ctrl+C` / `⌘C` | Copy-to-translate (the only action you need) |
| `Space` / `Esc` | Close the popup |
| Click outside the popup | Close the popup |
| `Enter` | Collapse / expand the popup |
| Drag the popup | Move it, and remember that position |
| 🔴 Red dot | Close |
| 🟡 Yellow dot | Collapse to title bar (keep for later) |
| 🟢 Green dot | Toggle standard / reading viewport width |

> The popup records clipboard state when it closes to avoid false triggers; only a genuine re-copy of the same text translates again. Text inside the popup is freely selectable for copying, without accidentally dragging the whole window.

---

## Provider Configuration

Enter credentials in **Settings → Providers** and press **Verify** to validate them instantly. DeepL validation queries the official usage endpoint (zero quota); Tencent/Baidu/Youdao have no dedicated check endpoint, so validation sends one tiny test translation counted against the monthly quota — the button states this clearly. Credentials are stored locally and encrypted in the database.

Step-by-step instructions for registering each provider are available inside the in-app settings panel. You can switch providers at any time; when the primary is unavailable traffic fails over automatically and returns when it recovers — no manual intervention.

---

## FAQ

**I copied something but no popup appeared.**
Right-click the tray icon and check whether clipboard monitoring was paused. Note that in many terminals `Ctrl+C` sends an interrupt rather than a copy — in Windows Terminal, select the text first.

**Can I use a hotkey instead of auto-popup?**
Currently copy-triggered. When you don't want interruptions, pause monitoring from the tray menu.

**Does it upload my data silently?**
Only the text you copy is sent to the translation providers you configured (that's a precondition for translating). History and API keys stay local, and keys are encrypted.

**The translation is off, or a provider is down.**
Configure several providers with failover enabled. Each card in Settings shows live status (recovering / rate-limited / auth failed / quota exhausted), so the cause is visible. When offline, previously translated content is served from local cache.

**What if local data gets corrupted?**
Translation keeps working. When the database misbehaves, history degrades temporarily with an honest status, and saved credentials are salvaged automatically — no reconfiguration after repair.

**How much does it use?**
Under 50 MB idle RAM, effectively 0% CPU, ~5 MB installer.

**Is Linux supported?**
Only Windows and macOS are currently released. The stack is Tauri-based and theoretically portable, but Linux has not been adapted or tested.

**Found a bug or have an idea?**
Please open an [issue](https://github.com/Aswellle/quick-translate/issues).

---

## Building from Source

### Prerequisites

- Node.js ≥ 20 (CI runs on Node 22)
- Rust stable ≥ 1.75
- Tauri CLI 2.x

### Development

```bash
npm install          # Install dependencies
npm run tauri dev    # Tauri dev mode with Vite HMR
```

Frontend-only debugging (no Tauri APIs):

```bash
npm run dev
```

### Build and Checks

```bash
npm run tauri build                            # Production build → src-tauri/target/release/
npx tsc --noEmit                               # TypeScript type check
cd src-tauri && cargo fmt --check              # Rust formatting
cd src-tauri && cargo clippy -- -D warnings    # Rust lints
cd src-tauri && cargo test                     # Rust unit tests
```

### Release

Push a `v*` tag to trigger the [release workflow](.github/workflows/release.yml), which builds and publishes signed installers (Windows MSI / NSIS, macOS DMG) to GitHub Releases and generates the auto-update manifest.

```bash
git tag v0.7.0
git push origin v0.7.0
```

---

## Architecture

| Layer | Technology |
|:--|:--|
| Frontend | React 19 + TypeScript + Vite + Tailwind CSS + Zustand |
| Backend | Rust (Tauri 2) + Tokio async runtime |
| Data | SQLite (rusqlite, WAL mode) |
| Desktop | System tray, clipboard monitoring, window materials, auto-update, launch at login |

**Core flow**

```text
Clipboard monitor detects a copy
      ↓
Monitor supervisor keeps it alive (self-healing)
      ↓
Request coordinator cancels stale work and orchestrates the popup
      ↓
Translation engine walks the failover chain (circuit breaking + request budget)
      ↓
All providers failed → local cache
      ↓
Result pushed to the frontend
```

**Data flow**: the frontend calls Tauri commands via `invoke()`; the backend pushes results through Tauri events; history and cache are persisted asynchronously by a dedicated bounded-queue worker — a slow database never delays the translation you see.

---

## License

Released under the [MIT License](LICENSE).
