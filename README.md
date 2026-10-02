<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="720">

# QuickTranslate

### 复制，就是翻译。

**看到不认识的英文，`Ctrl+C`。译文浮窗出现在光标旁边。**

无需切窗口，无需开网页，无需粘贴框。手都不用离开键盘。

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

[⬇ 下载最新版](https://github.com/Aswellle/quick-translate/releases/latest) · [🐛 报告问题](https://github.com/Aswellle/quick-translate/issues) · [📋 更新日志](https://github.com/Aswellle/quick-translate/releases)
</div>

---

## 目录

- [简介](#简介)
- [功能特性](#功能特性)
- [安装](#安装)
- [快速开始](#快速开始)
- [操作与快捷键](#操作与快捷键)
- [翻译源配置](#翻译源配置)
- [可靠性与稳定性](#可靠性与稳定性)
- [从源码构建](#从源码构建)
- [技术架构](#技术架构)
- [常见问题](#常见问题)
- [许可证](#许可证)

---

## 简介

QuickTranslate 是一款面向 Windows 与 macOS 的**复制即翻译**工具。它常驻系统托盘，监控你的剪贴板——任何时候复制一段文字，译文就会以浮窗的形式出现在光标附近。

> 它删掉的是「切到浏览器 → 打开翻译页 → 粘贴 → 等加载 → 切回文档 → 找回刚才读到哪一行」这十几秒。一份文档几十次，真正被消耗的不是时间，是你刚建立起来的那点专注。

它按常驻软件的标准设计：断网、翻译服务故障、剪贴板被占用，都不会打断它的工作；可恢复的故障在后台自动恢复，不需要你重启程序或重新配置。

**适用场景**：读英文文档与论文、处理外文邮件、刷海外内容、终端编译报错、Figma/Jira 里的英文术语……任何能选中并复制文字的地方，它都能用。

---

## 功能特性

### 核心体验

| 特性 | 说明 |
|:--|:--|
| **复制即翻译** | 剪贴板监控触发，无需快捷键，无需切换窗口 |
| **智能浮窗定位** | DPI 感知、多显示器支持，基于真实窗口尺寸与显示器工作区定位，自动避开光标、贴边翻转 |
| **非激活式弹窗** | 不抢夺前台焦点，切换到其他应用时自动关闭，不打断工作流 |
| **五路翻译源** | DeepL / 腾讯 / 百度 / 有道 / Google，配置多个时自动回退切换 |
| **多语言互译** | 支持 11 种语言，源语言自动识别，目标语言可选（默认简体中文） |

### 翻译源对比

| 翻译源 | 需要配置 | 免费额度 | 特点 |
|:--|:--|:--|:--|
| **Google 翻译** | ❌ 免配置 | 无限制 | 开箱即用的兜底源 |
| **DeepL** | API Key | 50 万字符/月 | 欧美语言质量顶尖 |
| **腾讯翻译君** | SecretId + SecretKey | 500 万字符/月 | 中英互译优秀 |
| **百度翻译** | APP ID + 密钥 | 100 万字符/月 | 中日韩互译友好 |
| **有道翻译** | 应用 ID + 应用密钥 | 新用户赠送额度 | 多语言支持 |

### 可靠性

| 特性 | 说明 |
|:--|:--|
| **翻译源自动回退** | 主力源失败或限流时自动切换下一个，该源恢复后自动回归，全程无需干预 |
| **实时健康面板** | 设置页与托盘显示每个翻译源的当前状态（正常 / 正在恢复 / 请求频繁 / 认证失败 / 额度用尽） |
| **离线缓存回退** | 断网时已译过的内容照常显示（标注来自本地缓存），网络恢复后自动继续 |
| **故障降级运行** | 本地数据异常时翻译照常工作，历史功能临时降级，应用不崩溃、不退出 |
| **凭证自动保护** | 数据库损坏时自动抢救已保存的 API Key，修复后无需重新配置各翻译源 |
| **最新结果优先** | 连续快速复制多段文字时，始终只显示最后一次的结果，迟到的旧译文不会覆盖新内容 |
| **剪贴板自愈** | 剪贴板被占用或句柄异常时自动退避恢复，恢复后不会把旧内容误当新复制重新弹出 |

### 数据与安全

| 特性 | 说明 |
|:--|:--|
| **本地历史记录** | SQLite 存储，支持搜索、收藏（标星永久保留）、删除、导出 |
| **加密存储** | API Key 使用 AES-256-GCM 加密后落盘，密钥绑定当前机器 |
| **FIFO 自动清理** | 超出上限时优先删除最旧的未收藏记录，收藏记录永不自动删除 |

### 其他

| 特性 | 说明 |
|:--|:--|
| **深浅主题** | 深色 / 浅色 / 跟随系统 |
| **静默更新** | 启动后自动检查，新版本后台下载安装，弹出提示后一键升级 |
| **轻量驻留** | 空闲内存 < 50MB，CPU 占用约 0 |
| **开机自启** | 支持随系统启动 |
| **首次引导向导** | 首次启动引导选择翻译源与目标语言，可跳过使用 Google 兜底 |

其他细节：PDF 断行自动拼接、译文/原文一键复制、同语言直通（不做无谓翻译）、单次 5000 字符上限、翻译源凭证可用性一键验证、托盘菜单一键开关剪贴板监控。

---

## 安装

### Windows

**系统要求**：Windows 10 / 11（x64），需要 WebView2 运行时（Win11 一般已自带，缺失时安装包会引导安装）。

前往 [Releases 页面](https://github.com/Aswellle/quick-translate/releases/latest) 下载：

- **`.msi`** — 推荐，标准安装向导
- **`.exe`** — NSIS 便携安装包

安装包约 5MB，安装完成后从开始菜单或桌面快捷方式启动即可。

### macOS

**系统要求**：macOS 13+（Apple Silicon）。

前往 [Releases 页面](https://github.com/Aswellle/quick-translate/releases/latest) 下载：

- **`.dmg`** — 磁盘映像，拖入应用程序文件夹即可。

---

## 快速开始

**1. 首次启动（半分钟）**

首次运行会进入引导向导：选择目标语言，翻译源可直接点「暂时跳过」——内置的 Google 源免配置、开箱即用。

**2. 复制一段文字**

在任意应用中选中并复制一段外文（`Ctrl+C`）。

**3. 看译文，然后关掉**

浮窗出现在光标旁边。看完按 `空格` 或 `Esc` 关闭，继续阅读。

之后 QuickTranslate 会安静地待在系统托盘里。右键托盘图标可以打开设置、浏览历史，或临时关闭剪贴板监控。

---

## 操作与快捷键

| 操作 | 效果 |
|:--|:--|
| `空格` / `Esc` | 关闭浮窗 |
| 点击浮窗外任意位置 | 关闭浮窗 |
| `Enter` | 折叠 / 展开浮窗 |
| 红色圆点 | 关闭 |
| 黄色圆点 | 折叠为标题栏（保留待看） |
| 绿色圆点 | 切换标准 / 阅读视口宽度 |

> 浮窗关闭时会记录当前剪贴板状态，避免误触发；真正重新复制同一段文字才会再次翻译。

---

## 翻译源配置

支持五路翻译源，可配置多个并启用回退。**回退顺序**：DeepL → 腾讯 → 百度 → 有道 → Google（兜底）。

- 在设置 → 翻译源中填入对应凭证，可用「验证」按钮即时校验（不消耗翻译配额）。
- 凭证只保存在本地，加密后写入数据库。
- 翻译源随时可切换；主力不可用时自动回退到下一个，恢复后自动回归，无需手动干预。
- 每张翻译源卡片实时显示健康状态——限流、认证失败、额度用尽等原因一眼可见，不用猜哪个环节出了问题。

各翻译源注册与获取凭证的详细步骤见应用内设置面板。

---

## 可靠性与稳定性

QuickTranslate 按**长期常驻**的标准设计：外部服务、网络、本地数据的故障都被隔离在各自范围内，可恢复的故障在后台自动处理，用户几乎无感。

| 场景 | 你看到的表现 |
|:--|:--|
| 翻译服务商宕机 / 限流 | 自动切换到下一个可用源，翻译照常出结果；该源恢复后自动回归 |
| 网络断开 | 浮窗、设置、历史全部照常可用；已译过的内容从本地缓存显示；网络恢复后下一次复制自动恢复 |
| 剪贴板被其他程序占用 | 监控自动退避重试；恢复后不会把旧内容误当新复制重新弹窗 |
| 本地数据库异常 | 翻译功能不受影响继续运行，历史功能临时降级并如实提示；已保存的 API Key 自动抢救，无需重新配置 |
| 连续快速复制多段文字 | 始终只显示最后一次复制的结果，不会出现旧译文覆盖新内容 |
| 长时间挂机使用 | 内存与资源占用稳定，写入队列有界，不会随使用时间膨胀 |

**工程保障**：核心故障路径全部有自动化测试覆盖（200+ 单元测试），CI 在 Linux / Windows / macOS 三平台运行前端构建、Rust 测试与静态检查，发布产物由 GitHub Actions 自动构建签名。

---

## 从源码构建

### 环境要求

- Node.js ≥ 20（CI 在 Node 22 上验证）
- Rust stable ≥ 1.75
- Tauri CLI 2.x

### 开发模式

```bash
npm install          # 安装依赖
npm run tauri dev    # 启动 Tauri 开发模式（含 Vite HMR）
```

仅调试前端（不使用 Tauri API）：

```bash
npm run dev
```

### 生产构建

```bash
npm run tauri build  # 产物输出到 src-tauri/target/release/
```

### 代码检查

```bash
npx tsc --noEmit                              # TypeScript 类型检查
cd src-tauri && cargo fmt --check             # Rust 格式检查
cd src-tauri && cargo clippy -- -D warnings   # Rust 静态检查
```

### 发布

推送 `v*` 标签触发 [release workflow](.github/workflows/release.yml)，自动构建并发布签名的安装包（Windows MSI / NSIS、macOS DMG）到 GitHub Releases，同时生成自动更新清单。

```bash
git tag v0.3.0
git push origin v0.3.0
```

---

## 技术架构

| 层 | 技术 |
|:--|:--|
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS + Zustand |
| 后端 | Rust（Tauri 2）+ Tokio 异步运行时 |
| 数据库 | SQLite（rusqlite，WAL 模式） |
| 桌面能力 | 系统托盘、剪贴板监控、全局更新器、开机自启 |

**核心流程**：剪贴板监控检测复制 → 监控守护自动保障存活 → 请求协调器取消旧请求、编排浮窗弹出 → 翻译引擎按回退链调用（含熔断与请求预算）→ 全源失败时查本地缓存 → 结果回传前端。

**数据流**：前端通过 `invoke()` 调用 Tauri 命令；后端通过 Tauri 事件推送结果；历史与缓存由独立的有界队列异步落盘，数据库再慢也不影响译文显示。

---

## 常见问题

**复制了但浮窗没出来？**
右键托盘图标检查剪贴板监控是否被关闭。另外注意：很多终端里 `Ctrl+C` 是中断命令而非复制——在 Windows Terminal 中需先选中文字。

**能改成快捷键触发，别自动弹吗？**
目前是复制即触发。不想被打扰时，托盘菜单一键关闭监控即可。

**它会偷偷上传我的数据吗？**
只有你复制的文字会发给你自己配置的翻译服务商（翻译的必要条件）。历史记录与 API Key 均保存在本地，Key 加密存储。

**占用多少资源？**
空闲内存 50MB 以内，CPU 基本为 0，安装包约 5MB。

**翻译结果不准 / 翻译源挂了怎么办？**
配置多个翻译源并启用回退，主力不可用时自动切换到下一个。设置页的翻译源卡片会实时显示各源健康状态（正在恢复 / 请求频繁 / 认证失败 / 额度用尽），原因一眼可见。网络中断时，已翻译过的内容会从本地缓存回退显示。

**本地数据出问题了，翻译还能用吗？**
能。数据库异常时翻译功能照常运行，历史功能临时降级并如实提示；已保存的翻译源凭证会被自动抢救，数据修复后无需重新配置。

**有 bug 或功能建议？**
欢迎在 [Issues](https://github.com/Aswellle/quick-translate/issues) 反馈。

---

## 许可证

本项目基于 **[MIT 许可证](LICENSE)** 开源——可自由使用、修改、分发，包括商业用途。

> 版权持有者保留在未来版本中按需修改或补充许可条款的权利：每一版本软件适用其发布时所附带的许可条款，已获得的版本继续按其附带条款使用。

---

<div align="center">

**如果它帮你省下了那些十几秒，点个 ⭐ 让更多人看到。**

</div>
