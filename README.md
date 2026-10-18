<div align="center">

<img src="assets/banner.jpg" alt="QuickTranslate" width="100%">

# QuickTranslate

**复制，就是翻译。任何应用，任何语言。**

在屏幕上的任何地方选中文字，按下 `Ctrl+C`（macOS 用 `⌘C`），译文浮窗就在光标旁展开。

不用切窗口，不用开网页，不用粘贴框 —— 手始终停在键盘上。


[![CI](https://github.com/Aswellle/quick-translate/actions/workflows/ci.yml/badge.svg)](https://github.com/Aswellle/quick-translate/actions/workflows/ci.yml)
[![Version](https://img.shields.io/github/v/release/Aswellle/quick-translate)](https://github.com/Aswellle/quick-translate/releases)
[![Downloads](https://img.shields.io/github/downloads/Aswellle/quick-translate/total)](https://github.com/Aswellle/quick-translate/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue)](https://github.com/Aswellle/quick-translate/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

![Tauri](https://img.shields.io/badge/Tauri-2.x-FFC131?logo=tauri&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-stable-DEA584?logo=rust)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)
![TypeScript](https://img.shields.io/badge/TypeScript-strict-3178C6?logo=typescript&logoColor=white)

**[⬇ 下载最新版](https://github.com/Aswellle/quick-translate/releases/latest)** · [🐛 报告问题](https://github.com/Aswellle/quick-translate/issues) · [💡 功能建议](https://github.com/Aswellle/quick-translate/issues) · [📋 更新日志](https://github.com/Aswellle/quick-translate/releases)

[中文](README.md) · [English](README.en.md)


</div>

---

## 它解决什么问题

翻译本身只要两秒。真正的成本在切换。

> 选中一段文字 → 切到浏览器 → 打开翻译页 → 粘贴 → 等加载 → 读 → 切回文档 → 重新找到刚才那一行。

一份文档里重复几十次。被消耗的不只是那十几秒，而是每次切换都会打断的**阅读连续性**——你刚建立起来的上下文，断了。

QuickTranslate 把这条链路压缩成一个动作：**复制**。译文在光标旁出现，读完按 `空格` 关掉，你从未离开过当前窗口。

它面向的不是「学英语」，而是**任何需要跨越语言读取信息的时刻**：

- 读外文论文、文档、RFC、Release Notes
- 处理外语邮件、Slack / Teams 消息、客服工单
- 看外语视频的字幕文本、社交媒体帖子、游戏内文本
- 排查终端报错、堆栈信息、CI 日志
- Figma / Jira / Notion 里的外语字段名与描述
- 看日文、韩文、德文、法文、俄文、阿拉伯文……以及任何你尚未掌握的语言
- 甚至是**反向输出**：把中文写好后翻译成英文，再复制到邮件或 Issue 里

只要是屏幕上能被选中、能被复制的文字，它就能用。

---

## 为什么是它，而不是网页翻译

| | 网页翻译 / 浏览器插件 | QuickTranslate |
|:--|:--|:--|
| **工作范围** | 只在浏览器内 | **任何应用**：编辑器、终端、PDF 阅读器、IM、设计工具 |
| **触发方式** | 打开页面 / 划词 / 右键菜单 | **复制** —— 已有的肌肉记忆，零学习成本 |
| **打断程度** | 切窗口、切标签页 | **不切窗口**，浮窗不抢焦点 |
| **失败表现** | 页面空白、报错 | **自动回退**到下一个翻译源，断网走本地缓存 |
| **隐私** | 文本经过第三方网页 | 只发给**你自己配置**的翻译服务商，密钥本地加密 |

一句话：网页翻译服务于「我正在浏览网页」，QuickTranslate 服务于「我正在做别的事，只是顺手要个译文」。

---

## 30 秒上手

**1. 安装并启动**（首次启动约半分钟）

安装包约 5 MB。首次运行会进入引导向导，选择目标语言即可。翻译源可以直接点「暂时跳过」——内置的 Google 源免配置、开箱可用。

**2. 在任意应用里复制一段文字**

`Ctrl+C`（Windows）/ `⌘C`（macOS）。

**3. 读完，关掉**

浮窗出现在光标旁。按 `空格` 或 `Esc` 关闭，继续手头的事。

之后它就安静地待在系统托盘里。右键托盘图标可以打开设置、浏览历史、或临时关闭剪贴板监控。

> 浮窗是**非激活式**的：不会抢走你正在打字的焦点；切换到其他应用时自动关闭。拖动过浮窗之后，下次会记住你调整的位置。

---

## 核心能力

### 复制即翻译

- **零快捷键** —— 复用已有的复制动作，没有新的组合键要记
- **非激活浮窗** —— 不抢焦点，切换应用自动关闭，不打断输入
- **智能定位** —— DPI 感知、多显示器支持，按真实窗口尺寸与显示器工作区计算，自动避开光标、贴边翻转
- **位置记忆** —— 拖动过就记住，下次唤出沿用你调整的位置；只有新的复制动作才回到光标旁
- **液态玻璃材质** —— Windows 使用 Acrylic，macOS 使用 NSVisualEffectView，浮窗背后是真实的桌面模糊而非纯色块

### 12 种语言，双向互译

简体中文 · 繁體中文 · English · 日本語 · 한국어 · Français · Deutsch · Español · Русский · Português · Italiano · العربية

源语言自动识别，目标语言在设置中切换（默认简体中文）。复制的内容已经是目标语言时直接透传，不做无谓的往返翻译。

### 五路翻译源，自动回退

| 翻译源 | 需要配置 | 免费额度 | 适合 |
|:--|:--|:--|:--|
| **Google 翻译** | 免配置 | 无限制（非官方接口） | 开箱即用的兜底源 |
| **DeepL** | API Key | 50 万字符/月 | 欧美语言质量顶尖 |
| **腾讯翻译君** | SecretId + SecretKey | 500 万字符/月 | 中英互译优秀 |
| **百度翻译** | APP ID + 密钥 | 100 万字符/月 | 中日韩互译友好 |
| **有道翻译** | 应用 ID + 应用密钥 | 新用户赠送额度 | 多语言覆盖 |

回退顺序：**DeepL → 腾讯 → 百度 → 有道 → Google**。可以只配一个，也可以全配。

- 主力源失败或限流时自动切到下一个，该源恢复后自动回归
- 设置页每张卡片实时显示健康状态：正常 / 正在恢复 / 请求频繁 / 认证失败 / 额度用尽
- 「验证」按钮即时校验凭证，**不消耗翻译配额**

### 历史与隐私

- **本地历史** —— SQLite 存储，支持搜索、标星收藏、删除、导出
- **智能展示** —— 命中搜索词自动定位居中；时间显示为「刚刚 / 12 分钟前 / 昨天」；按今天 / 昨天 / 最近一周分组
- **加密存储** —— API Key 以 AES-256-GCM 加密落盘，密钥绑定当前机器
- **FIFO 自动清理** —— 超出上限时优先删除最旧的**未收藏**记录，收藏内容永不自动删除
- **不偷偷上传** —— 只有你复制的文字会发给你自己配置的翻译源，其余数据全部留在本地

### 轻量常驻

- 安装包约 5 MB，空闲内存 < 50 MB，CPU 占用约 0
- 深色 / 浅色 / 跟随系统
- 开机自启、托盘常驻、静默自动更新
- 单次翻译上限 5000 字符，超出自动截断并提示

---

## 它为什么不会坏

QuickTranslate 按**长期常驻软件**的标准设计：外部服务、网络、本地数据的故障各自隔离，可恢复的故障在后台自愈。

| 你遇到的状况 | 它的表现 |
|:--|:--|
| 翻译服务商宕机 / 限流 | 自动切到下一个可用源，译文照常出现；该源恢复后自动回归 |
| 网络断开 | 浮窗、设置、历史全部照常可用；已译过的内容从本地缓存显示并标注来源，联网后下一次复制自动恢复 |
| 剪贴板被其他程序占用 | 监控自动退避重试；恢复后不会把旧内容误判为新复制而重复弹窗 |
| 本地数据库损坏 | 翻译功能不受影响继续运行，历史临时降级并如实提示；已保存的 API Key 自动抢救，修复后无需重新配置 |
| 连续快速复制多段 | 永远只显示最后一次的结果，迟到的旧译文不会覆盖新内容 |
| 长时间挂机 | 内存与资源占用稳定，写入队列有界，不随使用时长膨胀 |

**工程保障**：核心故障路径由 220+ 单元测试覆盖；CI 在 Linux / Windows / macOS 三平台跑前端构建、Rust 测试与静态检查（`clippy -D warnings`）；发布产物由 GitHub Actions 自动构建并签名，更新清单自动生成。

---

## 安装

### Windows

**系统要求**：Windows 10 / 11 (x64)，需要 WebView2 运行时（Win11 一般自带，缺失时安装包会引导安装）。

从 [Releases](https://github.com/Aswellle/quick-translate/releases/latest) 下载：

| 文件 | 说明 |
|:--|:--|
| `.msi` | 推荐，标准安装向导 |
| `-setup.exe` | NSIS 安装包 |

安装时可**自由选择安装路径**，并选择「仅为当前用户安装」（免管理员）或「为所有用户安装」。

### macOS

**系统要求**：macOS 13+ (Apple Silicon)。

从 [Releases](https://github.com/Aswellle/quick-translate/releases/latest) 下载 `.dmg`，打开后拖入「应用程序」文件夹即可。

### 应用内升级

已安装旧版？打开 **设置 → 关于 → 检查更新**，会显示「当前已为最新版」或「检查到新版本 x.y.z」，随后可一键下载（带实时进度）并重启完成升级。

---

## 操作与快捷键

| 操作 | 效果 |
|:--|:--|
| `Ctrl+C` / `⌘C` | 复制即翻译（唯一需要的动作） |
| `空格` / `Esc` | 关闭浮窗 |
| 点击浮窗外任意位置 | 关闭浮窗 |
| `Enter` | 折叠 / 展开浮窗 |
| 拖动浮窗 | 移动位置，并记住这个位置 |
| 🔴 红点 | 关闭 |
| 🟡 黄点 | 折叠为标题栏（保留待看） |
| 🟢 绿点 | 切换标准 / 阅读视口宽度 |

> 浮窗关闭时会记录剪贴板状态以避免误触发；只有真正重新复制同一段文字才会再次翻译。浮窗内可自由选中译文片段复制，不会误触发整窗拖拽。

---

## 翻译源配置

在 **设置 → 翻译源** 填入对应凭证，点「验证」即时校验（不消耗配额）。凭证只保存在本地，加密后写入数据库。

各翻译源的注册与取密钥步骤在应用内设置面板中有分步指引。翻译源随时可切换，主力不可用时自动回退，恢复后自动回归——不需要手动干预。

---

## 常见问题

**复制了，但浮窗没出来？**
右键托盘图标，检查剪贴板监控是否被关闭。另外注意：很多终端里 `Ctrl+C` 是中断信号而非复制——Windows Terminal 中需要先选中文字。

**能改成快捷键触发吗？不想自动弹。**
目前是复制即触发。不想被打扰时，托盘菜单一键关闭监控即可。

**它会偷偷上传我的数据吗？**
只有你复制的文字会发给你自己配置的翻译服务商（这是翻译的必要条件）。历史记录与 API Key 都存在本地，Key 加密存储。

**翻译不准 / 某个源挂了怎么办？**
多配几个翻译源并启用回退。设置页的卡片会实时显示各源状态（正在恢复 / 请求频繁 / 认证失败 / 额度用尽），原因一眼可见。断网时已译过的内容会从本地缓存回退显示。

**本地数据出问题了，还能用吗？**
能。数据库异常时翻译照常运行，历史功能临时降级并如实提示；已保存的凭证会被自动抢救，数据修复后无需重新配置。

**占用多少资源？**
空闲内存 50 MB 以内，CPU 基本为 0，安装包约 5 MB。

**支持 Linux 吗？**
目前只发布 Windows 与 macOS。代码基于 Tauri，理论上可移植，但尚未做 Linux 适配与测试。

**有 bug 或功能建议？**
欢迎在 [Issues](https://github.com/Aswellle/quick-translate/issues) 反馈。

---

## 从源码构建

### 环境要求

- Node.js ≥ 20（CI 在 Node 22 上验证）
- Rust stable ≥ 1.75
- Tauri CLI 2.x

### 开发

```bash
npm install          # 安装依赖
npm run tauri dev    # 启动 Tauri 开发模式（含 Vite HMR）
```

仅调试前端（不使用 Tauri API）：

```bash
npm run dev
```

### 构建与检查

```bash
npm run tauri build                            # 生产构建，产物在 src-tauri/target/release/
npx tsc --noEmit                               # TypeScript 类型检查
cd src-tauri && cargo fmt --check              # Rust 格式检查
cd src-tauri && cargo clippy -- -D warnings    # Rust 静态检查
cd src-tauri && cargo test                     # Rust 单元测试
```

### 发布

推送 `v*` 标签触发 [release workflow](.github/workflows/release.yml)，自动构建并发布签名安装包（Windows MSI / NSIS、macOS DMG）到 GitHub Releases，同时生成自动更新清单。

```bash
git tag v0.7.0
git push origin v0.7.0
```

---

## 技术架构

| 层 | 技术 |
|:--|:--|
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS + Zustand |
| 后端 | Rust（Tauri 2）+ Tokio 异步运行时 |
| 数据 | SQLite（rusqlite，WAL 模式） |
| 桌面能力 | 系统托盘、剪贴板监控、窗口材质、自动更新、开机自启 |

**核心流程**

```text
剪贴板监控检测到复制
      ↓
监控守护保证存活（崩溃自愈）
      ↓
请求协调器取消旧请求、编排浮窗
      ↓
翻译引擎沿回退链调用（熔断 + 请求预算）
      ↓
全源失败 → 查本地缓存
      ↓
结果推送到前端
```

**数据流**：前端通过 `invoke()` 调用 Tauri 命令；后端通过 Tauri 事件推送结果；历史与缓存由独立的有界队列异步落盘——数据库再慢，也不影响你看到译文的那一刻。

---

## 许可证

本项目基于 **[MIT 许可证](LICENSE)** 开源——可自由使用、修改、分发，包括商业用途。

> 版权持有者保留在未来版本中按需修改或补充许可条款的权利：每一版本软件适用其发布时所附带的许可条款，已获得的版本继续按其附带条款使用。

---

<div align="center">

### ⭐ 如果它帮你省下了那些来回切换

点个 Star，让更多被同样问题困扰的人找到它。

**[⬇ 下载最新版](https://github.com/Aswellle/quick-translate/releases/latest)** · [🐛 报告问题](https://github.com/Aswellle/quick-translate/issues) · [💡 功能建议](https://github.com/Aswellle/quick-translate/issues) · [📋 更新日志](https://github.com/Aswellle/quick-translate/releases)

<sub>QuickTranslate · Copyright © 2026 welle · MIT License</sub>

</div>
