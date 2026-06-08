# QuickTranslate

> **系统级剪贴板翻译工具** — 复制即翻译，浮窗就地呈现，无需切换任何应用。

[![Version](https://img.shields.io/badge/version-0.1.2-blue)](https://github.com/Aswellle/quick-translate/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-informational)](#运行环境)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-orange)](#)
[![CI](https://github.com/Aswellle/quick-translate/actions/workflows/release.yml/badge.svg)](https://github.com/Aswellle/quick-translate/actions)

---

## 它解决什么问题？

**场景一：阅读英文技术文档**
你在 VS Code 里看报错堆栈，或在浏览器里读 MDN / GitHub Issues，遇到不确定的表达，只需 `Ctrl+C` 复制，翻译浮窗立刻出现在光标旁边，确认含义后 `Esc` 关闭，继续阅读。全程不离开当前窗口，不打断思路。

**场景二：研究/阅读 PDF 论文**
在 Adobe Reader、Zotero 或浏览器内嵌 PDF 中读文献，选中一段晦涩的英文摘要或结论，复制，0.x 秒内看到中文译文。无需打开翻译网站，无需粘贴——对比切来切去节省的是注意力，不只是时间。

**场景三：处理外文邮件 / 即时消息**
收到英文或日文客户邮件，在 Outlook / 企业微信 / Slack 里逐段复制，浮窗依次弹出。回复前再复制对方原文确认语气，一整封邮件处理完不用离开邮件客户端。

**场景四：看外文字幕 / 文章**
刷 YouTube 外文视频、看 Twitter/X 推文或海外新闻，遇到不认识的单词或俚语，复制，即刻得到翻译，并自动保存到历史记录，方便事后回顾和积累词汇。

**场景五：程序员日常工作**
在终端复制编译错误信息，在 Figma 复制英文设计术语，在 Jira 复制英文需求描述——任何能复制文本的地方都能触发翻译，不受应用类型限制。

---

## 核心特性

| 特性 | 说明 |
|------|------|
| **零操作触发** | 复制（`Ctrl+C`）即触发，剪贴板监控全自动，无需额外快捷键 |
| **就地浮窗** | 弹窗紧贴光标位置，DPI 感知精确定位，支持拖移，`Esc` 或失焦自动关闭 |
| **五大翻译源** | DeepL · 腾讯翻译君 · 百度翻译 · 有道翻译 · Google Translate |
| **自动 Fallback** | 主翻译源失败时按优先级自动切换备用源，保障可用性 |
| **翻译历史** | SQLite 本地持久化，支持关键词搜索、星标收藏、分页浏览 |
| **API Key 加密存储** | AES-256-GCM 加密，密钥不明文落盘 |
| **自动静默更新** | 启动 5 秒后后台检查更新，有新版本自动下载安装，Toast 提示进度 |
| **极致轻量** | 安装包 ≤ 15MB，空闲内存 ≤ 50MB，CPU 占用接近 0% |
| **高 DPI 适配** | 物理像素坐标转逻辑像素，150% / 200% 缩放下浮窗位置精确 |
| **开机自启** | 可选注册表自启，状态可在设置中随时切换 |

---

## 下载安装

前往 [GitHub Releases](https://github.com/Aswellle/quick-translate/releases) 下载最新版本：

| 文件 | 说明 |
|------|------|
| `QuickTranslate_x.x.x_x64_en-US.msi` | Windows 标准安装包（推荐） |
| `QuickTranslate_x.x.x_x64-setup.exe` | Windows NSIS 安装包 |

> **系统要求**：Windows 10 / 11，x86_64，需安装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 11 已内置）。

---

## 快速上手

1. 安装后首次启动，完成引导向导（设置目标语言、可选配置翻译源 API Key）
2. 应用最小化到**系统托盘**，后台持续运行
3. 在任意应用中**选中文本后复制**（`Ctrl+C`）
4. 翻译浮窗自动弹出，显示译文、来源语言、翻译源
5. 点击「译文」或「原文」按钮一键复制，或按 `Esc` / 点击浮窗外侧关闭

> 可在设置中**暂停/恢复**剪贴板监控，避免特定场景下的误触发。

---

## 翻译源配置

右键托盘图标 → **设置** → **翻译源** 标签页

| 翻译源 | 所需凭证 | 免费额度 |
|--------|---------|---------|
| **Google Translate** | 无需配置 | 免费（非官方接口） |
| **DeepL** | API Key（`xxxx:fx` 格式） | 500,000 字符 / 月 |
| **腾讯翻译君** | SecretId + SecretKey | 500 万字符 / 月 |
| **百度翻译** | AppId + 密钥 | 100 万字符 / 月 |
| **有道翻译** | 应用 ID + 应用密钥 | 按量计费，有免费体验额度 |

**Fallback 优先级**（启用后主源失败自动切换）：
```
DeepL → 腾讯翻译君 → 百度翻译 → 有道翻译 → Google Translate
```

---

## 开发环境

### 依赖项

| 工具 | 版本要求 | 说明 |
|------|---------|------|
| [Node.js](https://nodejs.org/) | ≥ 18 | 前端构建 |
| [Rust](https://rustup.rs/) | stable ≥ 1.75 | 后端编译（通过 rustup 安装） |
| [Tauri CLI](https://tauri.app/start/) | 2.x | `npm install` 时自动安装 |
| Windows SDK / MSVC | 随 Rust 安装 | 需勾选 `Desktop development with C++` |

### 本地开发

```bash
# 克隆并安装依赖
git clone https://github.com/Aswellle/quick-translate.git
cd quick-translate
npm install

# 开发模式（Rust 热重载 + Vite HMR）
npm run tauri dev

# 调整 Rust 日志级别
RUST_LOG=debug npm run tauri dev

# TypeScript 类型检查
npx tsc --noEmit

# Rust lint
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# 生产构建（输出至 src-tauri/target/release/）
npm run tauri build
```

> **注意**：本项目为 **Windows 平台用户专用**，剪贴板模块依赖 `windows-sys`（`GetCursorPos` / `SendInput`）。

### 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS + Zustand |
| 后端 | Rust（Tauri 2 + Tokio 异步运行时） |
| 本地存储 | SQLite（rusqlite，WAL 模式，FTS5 全文索引） |
| HTTP | Reqwest（翻译 API 请求） |
| 加密 | AES-256-GCM（API Key 落盘加密） |
| 更新 | tauri-plugin-updater + GitHub Releases |

### 项目结构

```
quicktranslate/
├── src/                        # React 前端
│   ├── App.tsx                 # 窗口路由（#popup / #settings / #history / #onboarding）
│   ├── windows/
│   │   ├── popup/              # 翻译浮窗（加载 / 结果 / 错误 / ErrorBoundary）
│   │   ├── settings/           # 设置面板
│   │   ├── history/            # 翻译历史浏览器
│   │   └── onboarding/         # 首次运行引导向导
│   ├── stores/                 # Zustand 状态（configStore / historyStore / translationStore）
│   ├── hooks/                  # useTauriEvent、useTheme
│   ├── lib/                    # commands.ts（唯一 invoke 入口）、constants、types
│   └── styles/globals.css      # macOS 风格 CSS 变量设计系统
│
└── src-tauri/src/              # Rust 后端
    ├── lib.rs                  # 5 步初始化 + Tauri 命令注册
    ├── state.rs                # AppState（Arc/RwLock/Mutex，Tauri managed）
    ├── error.rs                # AppError 枚举 + IPC 序列化（error_code）
    ├── types.rs                # 跨层共享数据结构
    ├── commands/               # Tauri command handlers（translate / config / history / system）
    ├── domain/
    │   ├── config.rs           # ConfigService（JSON-KV，SQLite 持久化）
    │   ├── history.rs          # HistoryRepository（CRUD + FIFO 淘汰 + 星标保留）
    │   └── translator/         # TranslationEngine + 5 个 Provider（Trait 模式）
    ├── infra/
    │   ├── database.rs         # SQLite 连接、WAL、完整性检查、Schema 版本迁移
    │   ├── http_client.rs      # Reqwest 封装
    │   └── crypto.rs           # AES-256-GCM 加密
    └── system/
        ├── tray.rs             # 系统托盘菜单
        ├── clipboard.rs        # 剪贴板读写（arboard）+ 按键模拟（enigo）
        ├── clipboard_monitor.rs# 翻译主触发器：500ms 轮询 + 400ms 防抖 + 自触发防护
        ├── translation_flow.rs # 完整翻译流程编排（光标捕获 → 浮窗 → 翻译 → 事件推送）
        └── updater.rs          # 后台更新检查 + 自动下载安装 + Toast 通知
```

---

## 发布流程

推送 `v*` tag 即触发 GitHub Actions 自动构建并发布：

```bash
# 三处版本号必须保持一致：package.json / src-tauri/tauri.conf.json / src-tauri/Cargo.toml
git tag v0.2.0
git push origin v0.2.0
```

CI 流程：版本号一致性检查 → TypeScript 类型检查 → Rust Clippy lint → npm audit → Tauri 构建签名 → 上传 Release Assets → 生成 `latest.json`（供客户端自动更新）。

**所需 GitHub Secrets**：

| Secret | 说明 |
|--------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | minisign 私钥（base64 编码），用于安装包签名 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码（无密码时设为空字符串） |

---

## 路线图

- [x] 剪贴板监控触发翻译（全自动，无需手动快捷键）
- [x] 5 个翻译源 + 自动 Fallback 链
- [x] DPI 感知浮窗定位 + 拖移支持
- [x] 翻译历史（SQLite + 星标 + 搜索）
- [x] AES-256-GCM API Key 加密存储
- [x] 引导向导（Onboarding）
- [x] 静默自动更新（下载 + 安装）
- [x] GitHub Actions CI/CD（Windows `.msi` 自动构建 + 签名）
- [ ] 浮窗磨砂玻璃效果（Windows Acrylic）
- [ ] 历史记录导出（CSV / JSON）
- [ ] 多语言互译（当前默认目标语言为中文）
- [ ] 快捷键冲突检测

---

## License

MIT © QuickTranslate Contributors
