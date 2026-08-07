<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="720">

# QuickTranslate

### 复制，就是翻译。

**看到不认识的英文，`Ctrl+C`。译文浮窗出现在光标旁边。**

没有切窗口，没有开网页，没有粘贴框。手都不用离开键盘。

[![下载](https://img.shields.io/badge/⬇_立即下载-Windows_x64-2563eb?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)
[![版本](https://img.shields.io/github/v/release/Aswellle/quick-translate?style=for-the-badge&label=版本&color=555)](https://github.com/Aswellle/quick-translate/releases)
[![平台](https://img.shields.io/badge/平台-Windows_10/11-2563eb?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)
[![体积](https://img.shields.io/badge/安装包-约5MB-555?style=for-the-badge)](https://github.com/Aswellle/quick-translate/releases/latest)

</div>

---

## 目录

- [简介](#简介)
- [功能特性](#功能特性)
- [安装](#安装)
- [快速开始](#快速开始)
- [操作与快捷键](#操作与快捷键)
- [翻译源配置](#翻译源配置)
- [从源码构建](#从源码构建)
- [技术栈](#技术栈)
- [常见问题](#常见问题)
- [许可证](#许可证)

---

## 简介

QuickTranslate 是一款面向 Windows 的**复制即翻译**工具。它常驻系统托盘，监控你的剪贴板——任何时候复制一段文字，译文就会以浮窗的形式出现在光标附近。

> 它删掉的是「切到浏览器 → 打开翻译页 → 粘贴 → 等加载 → 切回文档 → 找回刚才读到哪一行」这十几秒。一份文档几十次，真正被消耗的不是时间，是你刚建立起来的那点专注。

**适用场景**：读英文文档与论文、处理外文邮件、刷海外内容、终端编译报错、Figma/Jira 里的英文术语……任何能选中并复制文字的地方，它都能用。

---

## 功能特性

| 特性 | 说明 |
|:--|:--|
| **复制即翻译** | 剪贴板监控触发，无需快捷键，无需切换窗口 |
| **智能浮窗定位** | DPI 感知、多显示器支持，自动避开光标区域、贴边翻转，屏幕边缘不截断 |
| **五路翻译源** | DeepL / 腾讯 / 百度 / 有道 / Google，配置多个时按顺序自动回退 |
| **多语言互译** | 支持 12 种语言，源语言自动识别，目标语言可选（默认中文） |
| **历史记录** | 本地存储，支持搜索、收藏、删除、导出 |
| **安全存储** | API Key 使用 AES-256-GCM 加密后落盘 |
| **深浅主题** | 深色 / 浅色 / 跟随系统 |
| **静默更新** | 启动后自动检查，新版本后台下载安装，仅弹出提示 |
| **轻量驻留** | 空闲内存 < 50MB，CPU 占用约 0 |
| **随开随关** | 支持开机自启；托盘菜单可随时暂停剪贴板监控 |

其他细节：PDF 断行自动拼接、译文/原文一键复制、同语言直通（不做无谓翻译）、单次 5000 字符上限、翻译源凭证可用性一键验证。

---

## 安装

**系统要求**：Windows 10 / 11（x64），需要 WebView2 运行时（Win11 一般已自带，缺失时安装包会引导安装）。

前往 [Releases 页面](https://github.com/Aswellle/quick-translate/releases/latest) 下载：

- **`.msi`** — 推荐，标准安装向导
- **`.exe`** — NSIS 便携安装包

安装包约 5MB，安装完成后从开始菜单或桌面快捷方式启动即可。

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
| 绿色圆点 | 切换 400px / 520px 阅读视口 |

> 浮窗关闭时会记录当前剪贴板状态，避免误触发；真正重新复制同一段文字才会再次翻译。

---

## 翻译源配置

支持五路翻译源，可配置多个并启用回退。**回退顺序**：DeepL → 腾讯 → 百度 → 有道 → Google（兜底）。

| 翻译源 | 需要配置 | 免费额度 | 备注 |
|:--|:--|:--|:--|
| **Google 翻译** | ❌ 免配置 | 无限制（非官方接口） | 开箱即用的兜底源 |
| **DeepL** | 一个 API Key | 50 万字符 / 月 | 欧美语言质量顶尖 |
| **腾讯翻译君** | SecretId + SecretKey | 500 万字符 / 月 | 中英互译优秀 |
| **百度翻译** | APP ID + 密钥 | 100 万字符 / 月 | 中日韩互译友好 |
| **有道翻译** | 应用 ID + 密钥 | 按量计费 | 新用户有体验金 |

- 在设置 → 翻译源中填入对应凭证，可用「验证」按钮即时校验（不消耗翻译配额）。
- 凭证只保存在本地，加密后写入数据库。
- 翻译源随时可切换；主力不可用时自动回退到下一个，无需手动干预。

---

## 从源码构建

### 环境要求

- Node.js ≥ 18
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

### 类型检查与测试

```bash
npx tsc --noEmit                          # TypeScript 类型检查
cargo clippy                              # Rust 静态检查（在 src-tauri/ 下）
cargo test --manifest-path src-tauri/Cargo.toml --lib   # Rust 单元测试
```

### 发布

推送 `v*` 标签触发 [release workflow](.github/workflows/release.yml)，自动构建并发布带签名的 MSI / NSIS 安装包到 GitHub Releases，同时生成自动更新清单。

```bash
git tag v0.2.4
git push origin v0.2.4
```

---

## 技术栈

| 层 | 技术 |
|:--|:--|
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS + Zustand |
| 后端 | Rust（Tauri 2）+ Tokio 异步运行时 |
| 数据库 | SQLite（rusqlite，内置 FTS5 全文索引） |
| 桌面能力 | 系统托盘、剪贴板监听、全局更新器、开机自启 |

**架构总览**：前端通过 `invoke()` 调用 Tauri 命令；剪贴板监控线程检测到复制后，由 `translation_flow` 编排「定位光标 → 弹出浮窗 → 调起翻译引擎 → 事件回传前端」。翻译引擎以 Provider 模式组织，按回退链顺序调用，失败自动切换。

详细结构见 [`CLAUDE.md`](CLAUDE.md)。

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

**有 bug 或功能建议？**
欢迎在 [Issues](https://github.com/Aswellle/quick-translate/issues) 反馈。

---

## 许可证

本项目为**专有软件**，仅供个人非商业使用。详细条款见 [LICENSE](LICENSE)。

商业授权或合作请联系：WL_Oneace@163.com

---

<div align="center">

**如果它帮你省下了那些十几秒，点个 ⭐ 让更多人看到。**

[⬇ 下载最新版](https://github.com/Aswellle/quick-translate/releases/latest) · [🐛 报告问题](https://github.com/Aswellle/quick-translate/issues) · [📋 更新日志](https://github.com/Aswellle/quick-translate/releases)

<sub>QuickTranslate · 版权所有 © 2026 Aswellle</sub>

</div>
