# QuickTranslate

> **系统级划词翻译工具** — 在任意应用中选中文本，按快捷键即可就地翻译。

[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue)](#)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-orange)](#)

---

## 特性

| 特性 | 说明 |
|------|------|
| 跨应用翻译 | 在 Chrome、VS Code、PDF 阅读器、微信等任意应用中均可使用 |
| 极速响应 | 快捷键到浮窗出现 ≤ 200ms（不含网络），P95 端到端延迟 ≤ 1.5s |
| 轻量常驻 | 安装包 ≤ 15MB，空闲内存 ≤ 50MB，CPU ≈ 0% |
| 五大翻译源 | DeepL · 腾讯翻译君 · 百度翻译 · 有道翻译 · Google Translate，自动 Fallback |
| 翻译历史 | 本地 SQLite 持久化，支持全文搜索（FTS5），默认保留最近 200 条 |
| 完全可配 | 快捷键自定义、目标语言、主题（浅/深/跟随系统）、开机自启 |
| 自动更新 | 启动后后台静默检查，有新版本时 Toast 提示并自动下载安装 |
| 高 DPI 适配 | 150% / 200% 缩放下浮窗位置精确跟随光标 |

---

## 快速开始

### 下载安装

从 [GitHub Releases](https://github.com/your/quicktranslate/releases) 下载：

- **Windows**：`QuickTranslate_x.x.x_x64_en-US.msi` 或便携版 `.zip`
- **macOS**：`QuickTranslate_x.x.x_x64.dmg`

### 使用方式

1. 启动应用 → 首次运行显示引导向导，完成后自动最小化到系统托盘
2. 在任意应用中**选中文本**
3. 按下 `Ctrl+Shift+D`（macOS：`Cmd+Shift+D`）
4. 浮窗出现，展示翻译结果（超过 5000 字符自动截断）
5. 点击复制按钮 或 按 `Esc` 关闭浮窗

> **注意**：重复按下快捷键会取消正在进行的翻译并立即开始新一次翻译。

---

## 翻译源配置

QuickTranslate 支持五个翻译源，默认激活 **Google Translate（无需配置）**。其余翻译源需填入对应 API 凭证，凭证经 AES-256-GCM 加密后存储在本地。

### Fallback 优先级

```
DeepL → 腾讯翻译君 → 百度翻译 → 有道翻译 → Google Translate
```

开启 Fallback 后，主翻译源失败时自动按优先级依次尝试已配置的备用源。

### 各翻译源配置入口

右键托盘图标 → **设置** → **翻译源** 标签页

| 翻译源 | 所需凭证 | 免费额度 |
|--------|---------|---------|
| **DeepL** | API Key（格式：`xxxx:fx`） | 500,000 字符/月 |
| **腾讯翻译君** | SecretId + SecretKey | 500 万字符/月 |
| **百度翻译** | AppId + 密钥 | 100 万字符/月 |
| **有道翻译** | 应用 ID + 应用密钥 | 按量计费，有免费体验 |
| **Google Translate** | 无需配置 | 免费（非官方接口） |

---

## 开发

### 环境要求

| 工具 | 版本 |
|------|------|
| Node.js | ≥ 18 |
| Rust | stable ≥ 1.75 |
| Tauri CLI | 2.x |

### 本地开发

```bash
# 安装前端依赖
npm install

# 启动开发模式（Rust 热重载 + Vite HMR）
npm run tauri dev

# 控制 Rust 日志级别（默认 info）
RUST_LOG=debug npm run tauri dev

# 生产构建（输出到 src-tauri/target/release/）
npm run tauri build
```

### 项目结构

```
quicktranslate/
├── src/                        # React 前端
│   ├── App.tsx                 # 窗口路由（#popup / #settings / #history / #onboarding）
│   ├── windows/                # 四个窗口组件
│   │   ├── popup/              # 翻译浮窗
│   │   ├── settings/           # 设置面板
│   │   ├── history/            # 历史记录浏览
│   │   └── onboarding/         # 首次运行引导向导
│   ├── stores/                 # Zustand 状态（config / history / translation）
│   ├── hooks/                  # useTauriEvent、useTheme
│   └── lib/                    # 常量、类型、Tauri invoke 封装
│
└── src-tauri/src/              # Rust 后端
    ├── lib.rs                  # 5 步初始化序列 + 命令注册
    ├── state.rs                # AppState（Arc 包装，Tauri managed）
    ├── error.rs                # AppError 枚举 + IPC 序列化
    ├── types.rs                # 跨层共享数据结构
    ├── commands/               # Tauri command handlers
    ├── domain/                 # 核心业务逻辑
    │   ├── config.rs           # ConfigService（KV 配置，SQLite 持久化）
    │   ├── history.rs          # HistoryRepository（CRUD + FTS5 搜索）
    │   └── translator/         # TranslationEngine + 5 个 Provider 实现
    ├── infra/                  # 基础设施层
    │   ├── database.rs         # SQLite（WAL 模式、完整性检查、Schema 迁移）
    │   ├── http_client.rs      # Reqwest 封装
    │   └── crypto.rs           # AES-256-GCM API Key 加密
    └── system/                 # OS 集成层
        ├── tray.rs             # 系统托盘
        ├── hotkey.rs           # 全局快捷键注册
        ├── clipboard.rs        # 剪贴板读写 + 按键模拟
        ├── translation_flow.rs # 翻译主流程编排（含 DPI 感知浮窗定位）
        └── updater.rs          # 后台静默更新检查
```

### 后端初始化顺序

`lib.rs::run()` 的 setup 回调中，以下步骤**必须按序执行**：

1. **基础设施层**：数据库（WAL + 完整性检查）+ HTTP 客户端
2. **Domain 层**：ConfigService + HistoryRepository
3. **翻译源注册**：注册全部 5 个 Provider，从配置加载激活项和 Fallback 开关
4. **AppState 组装**：`app.manage()` 注入全局状态
5. **系统服务**：全局快捷键注册 + 系统托盘 + 后台更新检查（异步，5s 延迟）

### 自动更新配置

参考 [`docs/UPDATER_SETUP.md`](docs/UPDATER_SETUP.md) 完成签名密钥生成、`tauri.conf.json` 配置和 GitHub Secrets 设置。发布新版本只需打 tag：

```bash
git tag v0.2.0 && git push origin v0.2.0
```

GitHub Actions 将自动构建、签名、发布，并生成 `latest.json` 供客户端更新检测。

---

## 路线图

- [x] **Stage 1 — MVP Core**
  - 系统托盘 + 全局快捷键 + 剪贴板捕获
  - 5 个翻译源（DeepL / 腾讯 / 百度 / 有道 / Google）+ Fallback 链
  - 翻译浮窗（加载态 / 错误态 / 结果展示）
  - 翻译历史（SQLite + FTS5 全文搜索，最多 200 条）
  - 设置面板（完整配置项）
  - DPI 感知浮窗定位（150% / 200% 缩放）
  - 首次运行引导向导（Onboarding）
  - 开机自启动（Windows 注册表 / macOS LaunchAgent）
  - 自动更新（GitHub Releases + Tauri Updater）

- [ ] **Stage 2 — 体验完善**
  - 浮窗磨砂玻璃效果（Windows Acrylic / macOS NSVisualEffectView）
  - 快捷键冲突检测与提示
  - 深色 / 浅色主题完整适配
  - 错误处理 Toast 通知优化

- [ ] **Stage 3 — 分发**
  - GitHub Actions CI/CD（Windows `.msi` + macOS `.dmg` 自动构建）
  - README 截图 + 演示 GIF

---

## License

MIT © QuickTranslate Contributors
