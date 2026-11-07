# Liquid Glass 专项 · 基线审计（BASELINE）

> 审计日期：2026-10-22
> 审计对象：工作区 `D:\Chrome_Downloads\AI_Coding\12.QuickTraslate_Repo\testflight\quicktranslate`
> 本阶段性质：**只读审计**。未修改任何产品源代码、Rust 代码、依赖版本或数据库/配置行为。
> 对应规格：`docs/implementation/popup-glass/quick-translate-liquid-glass-implementation-plan.md`

---

## 1. 工作区与版本基准

### 1.1 Git 状态

| 项 | 值 |
|---|---|
| 当前分支 | `feat/stats-dashboard-and-popup-polish` |
| HEAD commit | `d3ed69a3565f52b27f6cf90e984a67b00e232746` |
| HEAD 提交信息 | `feat: 设置页统计仪表盘、折叠态信息条与主题实时同步` |
| upstream | `origin/feat/stats-dashboard-and-popup-polish` |
| `origin/main` | `7d10376c94b809938d3ef7e4104d00d4fe9aada9` |
| HEAD 与 origin/main 的树差异 | **空**（`git diff --stat d3ed69a origin/main` 无输出） |
| 未跟踪文件 | `.zcodeignore`、`bash.exe.stackdump`、`docs/implementation/` |

**重要结论**：规格文档声明的审计基准 `7d10376` 就是当前的 `origin/main`，且当前 HEAD 与它**树完全一致**（PR #68 已以 squash 方式合并）。因此规格第 2 节的代码观察在本工作区**全部成立**，无需重新定位。

未跟踪的 `.zcodeignore` 与 `bash.exe.stackdump` 与本专项无关，本次审计未触碰。`docs/implementation/` 是本阶段新增的文档目录。

### 1.2 项目规则文件

- **`CLAUDE.md`**（仓库根，已存在）：包含构建/测试命令、架构说明、跨语言契约、发布流程。**无 `AGENTS.md`**（`node_modules/recharts/AGENTS.md` 是第三方包自带文件，不适用）。
- 无 `.cursorrules`、无 `.github/copilot-instructions.md`。
- 本次审计遵循 `CLAUDE.md` 的既有约束：注释用中文、不得提交凭证、发布走 PR。

### 1.3 工具链实际版本（已实测，非 Cargo.toml 宽松范围）

| 工具 | 实测版本 | 来源 |
|---|---|---|
| Node | `v24.19.0` | `node --version` |
| npm | `11.17.0` | `npm --version` |
| rustc | `1.99.0 (b940084d7 2026-09-28)` | `rustc --version` |
| cargo | `1.99.0 (5f94df478 2026-08-27)` | `cargo --version` |
| Rust toolchain 固定 | `rust-toolchain.toml` → `channel = "1.99.0"` | 文件 |

> 注意：CI 使用 Node **22**，本机是 Node **24**。两者均满足 `@tauri-apps/cli` 与 Vite 6 的要求，但属于环境差异，记录在案。

### 1.4 依赖锁定版本（`src-tauri/Cargo.lock` 实测）

| Crate | 锁定版本 | 备注 |
|---|---|---|
| `tauri` | **2.10.3** | 规格基准一致 |
| `tauri-build` | 2.5.6 | |
| `tauri-runtime` | 2.10.1 | |
| `tauri-runtime-wry` | 2.10.1 | |
| `tauri-utils` | **2.8.3** | 决定 `Effect` 枚举可用变体 |
| `wry` | 0.54.4 | |
| `tao` | 0.34.8 | 决定 macOS `set_theme` 行为 |
| `window-vibrancy` | **0.6.0** | 实际执行系统材质的 crate |
| `objc2` | 0.6.4 | |
| `objc2-app-kit` | 0.3.2 | 当前仅启用 `NSWorkspace`、`NSRunningApplication` |
| `tauri-plugin-updater` | 2.10.1 | |
| `tauri-plugin-single-instance` | 2.4.5 | |

前端 `package.json` 声明为 `^2`，**实际安装**：

| 包 | 实测版本 |
|---|---|
| `@tauri-apps/api` | **2.10.1** |
| `@tauri-apps/cli` | **2.10.1** |

### 1.5 目标版本调研（已查 crates.io / npm / docs.rs，非凭记忆）

| 包 | 最新稳定版 | 与 Liquid Glass 的关系 |
|---|---|---|
| `tauri` | **2.12.5**（2026-10-21） | 2.12.0 起引入 Liquid Glass |
| `tauri-utils` | 2.10.2 | `WindowEffect` 新增 `LiquidGlassRegular` / `LiquidGlassClear`（**已从 docs.rs 源码确认**） |
| `window-vibrancy` | **0.8.1**（2026-09-22） | 新增 `apply_liquid_glass` / `clear_liquid_glass` |
| `@tauri-apps/api` | 2.12.4 | |
| `@tauri-apps/cli` | 2.12.4 | |

**tauri 2.12.5 的传递依赖跃迁**（来自 crates.io 依赖接口）：

```
tauri 2.12.5
 ├─ tauri-build      ^2.7.4   (当前 2.5.6)
 ├─ tauri-runtime    ~2.12.2  (当前 2.10.1)
 ├─ tauri-runtime-wry ~2.12.3 (当前 2.10.1) → wry ^0.57.0 (当前 0.54.4), tao ^0.37.0 (当前 0.34.8)
 ├─ tauri-utils      ~2.10.2  (当前 2.8.3)
 └─ window-vibrancy  ^0.8.1   (当前 0.6.0)
```

**MSRV**：`tauri 2.12.5` 声明 `rust_version = 1.90`，本项目固定 1.99.0 → **满足**。该 crate 自身使用 edition 2024，不影响调用方 edition 2021。

**已识别的依赖冲突风险**：`window-vibrancy 0.8.1` 依赖 `windows-sys >=0.60, <=0.61`，本项目直接依赖 `windows-sys 0.52`。二者在 Cargo 中可共存（不同主版本），但需在 M1 中确认 `DwmSetWindowAttribute` 调用方（`commands/system.rs`）仍走 0.52 且能编译。

---

## 2. 基线命令实测结果

全部在仓库根目录执行，本机 Windows 11 build 22631。

| 命令 | 结果 | 摘要 |
|---|---|---|
| `npm test` | ✅ **通过** | 4 个测试文件 / **25 个用例**，Vitest 5.0.3 |
| `npx tsc --noEmit` | ✅ **通过** | 无输出，exit 0 |
| `npm run build` | ✅ **通过** | 658 模块，15.78s；JS 722.10 kB（gzip 215.19 kB），CSS 44.07 kB |
| `cargo fmt --check` | ✅ **通过** | exit 0 |
| `cargo clippy -- -D warnings` | ✅ **通过** | 与 CI 同款参数（**不带** `--all-targets`），23.53s |
| `cargo test`（完整，非 `--lib`） | ✅ **通过** | **231 passed / 0 failed**；另有两个测试目标各 0 用例 |

### 2.1 需要如实记录的差异与限制

1. **`cargo clippy --all-targets` 会失败**（14 个 error），全部集中在既有测试代码（如 `popup_geometry.rs` 的 `assertions_on_constants`）。CI 使用**不带** `--all-targets` 的形式，因此不阻塞。本专项若新增测试代码，需注意不要引入同类 lint。
2. **`npm run build` 有两条警告**（非错误）：
   - Browserslist 数据 7 个月未更新（环境提示，非代码问题）
   - `@tauri-apps/api/event` 同时被动态与静态导入，无法拆包
   - 主 chunk 722 kB 超过 500 kB 建议阈值（引入 recharts 后）
3. **macOS 相关命令全部未运行**：本机是 Windows，`cargo test`/`clippy`/`fmt` 的 macOS 目标路径**未验证**。CI 的 `macos-latest` job 只能证明编译与单测，**不能证明原生材质渲染**。
4. **无任何视觉/对比度自动化测试**：现有 25 个前端用例只覆盖 `constants`、`domUtils`、`export`、`historyStore`，**没有一条**触及浮窗渲染、主题同步或材质。

### 2.2 Rust 测试分布（231 用例）

| 模块 | 用例数 |
|---|---|
| `domain::translator` | 61 |
| `system::clipboard` | 42 |
| `system::popup_geometry` | 19 |
| `system::translation` | 18 |
| `system::popup_watch` | 16 |
| `runtime::status` | 14 |
| `domain::config` / `infra::crypto` / `domain::cache` | 12 / 12 / 12 |
| `system::tray` | 8 |
| `util::backoff` | 6 |
| `system::persistence` | 5 |
| `infra::database` | 3 |
| 其他 | 4 |

**与浮窗直接相关的只有 `popup_geometry`（19）与 `popup_watch`（16）**，二者分别覆盖几何定位与关闭看守，**完全不覆盖材质、主题或对比度**。

### 2.3 实测环境

| 项 | 值 |
|---|---|
| OS | Windows 11 家庭中文版，build **22631**（10.0.22631） |
| WebView2 Runtime | **155.0.4283.62** |
| 显示器 | 单屏 1280×720，缩放 100% |

> `22631 ≥ 22523` → `window-vibrancy 0.6.0` 走 `DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW` 分支（源码确认）。历史实机查询 DWM 属性返回 `backdrop=3`，证明 Acrylic **确实在工作**。

---

## 3. 规格第 2 节缺陷的逐条复核

对规格列出的每个 P0/P1/P2，本审计给出**源码确认**结论，并标注行号。

### 3.1 P0 · macOS 未使用 Liquid Glass —— **成立**

- `src-tauri/src/system/translation_flow.rs:50-59`：macOS 分支只声明 `Effect::Popover` + `EffectState::FollowsWindowActiveState`。
- `Cargo.lock` 的 `tauri-utils = 2.8.3` 的 `WindowEffect` 枚举**根本没有** `LiquidGlassRegular`（已读本地 registry 源码 `tauri-utils-2.8.3/src/lib.rs:70-141`）。
- 因此当前代码**无法编译**出 Liquid Glass，不是配置问题，是版本缺失。

### 3.2 P0 · macOS `set_popup_theme` 空实现 —— **成立**

`src-tauri/src/commands/system.rs:174-178`：

```rust
#[cfg(not(windows))]
#[tauri::command]
pub async fn set_popup_theme(_dark: bool) -> Result<(), AppError> {
    Ok(())
}
```

前端 `src/windows/popup/PopupWindow.tsx:345-371` 的 `MutationObserver` 确实在 `.dark` 变化时调用 `setPopupTheme(dark)`，但 macOS 上这次 IPC 什么也没做。

**参数契约必须保留**：注释明确说明 `_dark` 的下划线是刻意的（tauri 宏会 unraw 后转 camelCase，键仍是 `dark`）。改成 `dark` 会触发 unused 告警，改成别的名字会让 IPC 键漂移。M3 若需要用到该值，应改为 `dark: bool` 并在 Windows 分支之外也使用它，或加 `#[allow(unused)]`。

### 3.3 P1 · 正文对比度只依赖文本 token —— **成立**

- `src/styles/globals.css:69`：`--popup-bg: rgba(255,255,255,0.10)`
- `globals.css:124`：深色 `--popup-bg: rgba(28,28,30,0.16)`
- 文字衬底实际由 `globals.css:196-227` 的 `::before` 高光渐变 + `::after` 内描边承担。
- `ResultView.tsx:59` 的译文直接渲染在这个玻璃层上，**没有独立的阅读衬底类**。

### 3.4 P1 · `backdrop-filter` 不能替代桌面模糊 —— **成立（且代码已自述）**

`globals.css:178-182` 的注释已说明：透明窗口背后的像素由 OS 合成，webview 采不到，30px 的 CSS 模糊在 Windows/macOS 上等于空转。保留它只是无原生材质平台的兜底。

### 3.5 P1 · 首次显示的原生材质时序风险 —— **成立，且比规格描述更具体**

`translation_flow.rs:111-132` 的 `ensure_popup_window()` 在窗口不存在时调用：

```rust
build_popup_window(app, (fallback_pos.x, fallback_pos.y), true)  // ← visible = true
```

而 `build_popup_window`（`translation_flow.rs:82-108`）内部是：

```rust
let window = WebviewWindowBuilder::new(...)
    .visible(visible)          // ← 先按 true 显示
    .build()?;
apply_popup_material(&window); // ← 之后才施加材质
```

**结论**：`ensure_popup_window` 路径存在"先可见、后上材质"的首帧窗口期。这正是 onboarding 关闭后的建窗路径（`lib.rs:66-88` 的 `CloseRequested` 分支）。

另一条路径 `show_popup_loading`（`translation_flow.rs:163-169`）传 `false`，建窗后 `apply_popup_material` 再 `show()`，**顺序正确**。

### 3.6 P1 · 交通灯 glyph 仅在 hover 显示 —— **成立**

- `globals.css:381-390`：`.traffic-light svg { opacity: 0; }`
- `globals.css:398`：`.traffic-lights.group:hover .traffic-light:not(:disabled) svg { opacity: 1; }`
- `globals.css:403-406`：`.traffic-light:focus-visible` **只加 outline**，未恢复 svg 的 opacity。

**结论**：键盘 Tab 聚焦到按钮时，能看到 focus ring，但**看不到图标**。规格 §6.5 要求补 `.traffic-lights:focus-within .traffic-light:focus-visible svg { opacity: 1 }` 等价规则。

### 3.7 P1 · 缺少浮窗专项测试 —— **成立**

见 §2.1 第 4 条与 §2.2。

### 3.8 P2 · 减少透明/减少动态的降级不完整 —— **部分成立，需修正规格描述**

| 偏好 | 现状 | 证据 |
|---|---|---|
| `prefers-reduced-motion` | ✅ **已全局处理** | `globals.css:620-626` 把 animation-duration 压到 0.01ms、iteration-count 压到 1。呼吸点、shimmer、入场动画全部被抑制 |
| `prefers-reduced-transparency` | ❌ **完全未处理** | 全文件仅一处 `prefers-reduced` 匹配（即上面的 reduced-motion） |
| `prefers-contrast` | ❌ 未处理 | 无匹配 |
| `forced-colors` | ❌ 未处理 | 无匹配 |

> **对规格的修正**：规格 §2.2 把 reduced-motion 与 reduced-transparency 并列描述为"尚未形成完整契约"。实测 reduced-motion **已经有全局兜底**（虽然是被动地靠 0.01ms 时长，而不是显式的静态点）。M4 只需确认它不会误伤，重点应放在 reduced-transparency。

**WebView2 支持性实测**（本机 Edge 155 headless，双查询法：`reduce` 与 `no-preference` 都不匹配才判定为不支持）：

```json
"reducedTransparency": { "reduce": false, "noPref": true, "supported": true },
"reducedMotion":       { "reduce": false, "noPref": true, "supported": true },
"prefersContrast":     { "noPref": true, "supported": true }
```

**结论**：Chromium 155 内核**支持** `prefers-reduced-transparency`。但这是 Edge，不是 Tauri 的 WebView2 实例。

> **后续（2026-10-22）**：WKWebView 的支持情况已从"未知"变为**确定不支持**——MDN BCD `safari.version_added: false`（impl_url 指向 WebKit bug 175497），Safari 26.0 官方特性清单全文亦无此项。WKWebView 与 Safari 同引擎，因此 macOS 上该媒体查询恒为 `false`。已按 R7 的预案补应用内开关，见 DECISIONS D6。M4 在 WebView2 上的复测已完成（媒体查询与开关两条路径均实测）。

### 3.9 P2 · 主题同步双重轮询 —— **成立**

- `src/hooks/useTheme.ts:50-62`：`system` 模式下每 2s 轮询 `applyTheme("system")`
- `src/windows/popup/PopupWindow.tsx:361-365`：浮窗再每 2s 轮询 `sync()`（读 `<html>` class）

`lastDark` 去重使真正的 IPC 只在明暗变化时发生，但**检查本身是双份的**。规格要求：先证明新链路稳定，再考虑移除浮窗那份，且**不能**动 `useTheme` 的系统兜底。

---

## 4. 规格未列出、本次审计新发现的问题

以下四条**不在规格第 2 节中**，但影响 M1/M3 的验收，必须记录。

### 4.1 【新·高】`set_effects` 的错误被静默吞掉，材质失败日志是死代码

`translation_flow.rs:67-73` 写了：

```rust
if let Err(e) = window.set_effects(effects) {
    tracing::warn!(event = "popup_material_unavailable", ...);
}
```

但 Tauri 2.10.3 的实现（本地源码 `tauri-2.10.3/src/window/mod.rs:1849-1855`）是：

```rust
pub fn set_effects<E: Into<Option<WindowEffectsConfig>>>(&self, effects: E) -> crate::Result<()> {
    let effects = effects.into();
    let window = self.clone();
    self.run_on_main_thread(move || {
        let _ = crate::vibrancy::set_window_effects(&window, effects);  // ← 内层 Result 被丢弃
    })
}
```

`vibrancy::set_window_effects`（`tauri-2.10.3/src/vibrancy/mod.rs:16-31`）返回 `Ok(())` 恒定，macOS 的 `apply_effects` 更是返回 `()`。

**结论**：`popup_material_unavailable` 这条 warn **只在 `run_on_main_thread` 本身失败时**才会打印（几乎不可能）。真正的材质失败（如旧系统不支持 Acrylic、Liquid Glass 版本不足）**不会留下任何日志**。

**Tauri 2.12.5 仍然如此**（已核对 docs.rs 源码）。因此 M1 不能依赖这个日志判断材质是否生效，需要另设诊断手段（例如启动时探测 OS 版本并记录决策）。

### 4.2 【新·中】`ensure_popup_window` 与 `show_popup_loading` 的材质路径不对称

见 §3.5。两条路径中只有一条满足"先材质后显示"。M1 的"统一先隐藏再显示"必须**同时覆盖**这两条，否则修了一条仍会复现。

### 4.3 【新·中】Windows 首帧材质明暗跟随系统主题，而非应用主题

`apply_popup_material` 只施加 Acrylic，**不设置** `DWMWA_USE_IMMERSIVE_DARK_MODE`。该属性要等前端挂载、`MutationObserver` 首次 `sync()` 后才通过 `set_popup_theme` 写入。

在"应用深色 + 系统浅色"组合下，浮窗创建到前端首次同步之间，**背后是一块浅色亚克力**，而 CSS 已是深色文字——存在可见的错色首帧。

**源码依据**：`PopupWindow.tsx:350-357`（`sync()` 在 effect 内首次调用）；`translation_flow.rs:39-74`（材质施加处无 dark 参数）。

### 4.4 【新·中】`EffectState::FollowsWindowActiveState` 与 `focusable(false)` 的语义冲突

macOS 分支声明了 `FollowsWindowActiveState`（`translation_flow.rs:56`），而浮窗是 `focusable(false)`（`translation_flow.rs:96`）——它**永远不会成为 key window**。

`NSVisualEffectView.state = .followsWindowActiveState` 在窗口非活动时会渲染更淡、更"失焦"的材质。这意味着**当前 macOS 上的材质很可能始终处于 inactive 外观**。

> 这是**假设，不是结论**：必须在真实 macOS 上对照 `EffectState::Active` 观察差异。若成立，M1 应改为 `Active`（浮窗本来就是短生命周期置顶窗，恒定"活跃"外观更符合设计意图，与 `globals.css:415-418` 中"不做窗口失焦转灰"的既有决策一致）。

---

## 5. 实机视觉基线（Windows，本机）

| 项 | 观测 |
|---|---|
| 系统材质 | Acrylic（DWM `DWMSBT_TRANSIENTWINDOW`）**确认生效**：历史查询 `backdrop=3`；黑底白字探头可见背景被模糊成晕影 |
| 浅色主题展开态 | 玻璃透出桌面，文字清晰 |
| 深色主题展开态 | `--popup-bg = rgba(28,28,30,0.16)`，正文 `#F5F5F7`，对比正常 |
| 折叠态（44px） | 状态点 + 摘要 + 语言方向完整；浅/深主题均正常 |
| 已知渲染陷阱 | `PrintWindow(PW_RENDERFULLCONTENT)` 抓不全 GPU 合成层（会漏掉右端元素）；**CDP `Page.captureScreenshot` 才准确**。后续视觉验收应统一用 CDP |

**未采集**：本阶段未重新截图存档。规格 §0.1.5 要求的目标设备截图矩阵，建议在 M0 收尾时与 `evidence/` 一并补齐（见 INTEGRATION_MAP §6）。

---

## 6. 风险清单（需用户审阅）

| # | 风险 | 影响 | 建议 |
|---|---|---|---|
| R1 | Tauri 2.10.3 → 2.12.5 连带 `wry`/`tao` 大版本跃迁（0.54→0.57 / 0.34→0.37） | 可能影响透明窗口、`startDragging`、`focusable`、窗口事件等既有行为 | M1 单独成提交，升级后**必须重跑全部 231 个 Rust 测试 + 实机回归 §9 清单** |
| R2 | `window-vibrancy` 0.6→0.8 的 `windows-sys` 版本区间与项目直接依赖的 0.52 不同 | 编译期可能出现两个 `windows-sys`，`DwmSetWindowAttribute` 调用点需复核 | M1 中确认 `commands/system.rs` 仍编译通过；必要时给直接依赖加版本说明注释 |
| R3 | macOS `set_theme` 是**应用级**（`tao` 调 `NSApp setAppearance:`），不是单窗口 | 用它修 `set_popup_theme` 会同时改变 settings/history 的原生外观 | 二选一：① 接受应用级（CSS 本来也全局一致）；② 用 `ns_window()` + `objc2` 做**单窗口** appearance。**需用户决策** |
| R4 | 若用应用级 `set_theme`，可能改变 WKWebView 的 `prefers-color-scheme` | `theme = system` 时，我们的覆写可能**掩盖系统主题变化**，导致轮询失效 | 若选方案①，M3 必须验证"跟随系统"下系统切换仍能被检测；否则强制选方案② |
| R5 | `FollowsWindowActiveState` + `focusable(false)` 可能让材质永远偏淡 | macOS 玻璃观感明显弱于预期 | M1 在真机对比 `Active` 与 `FollowsWindowActiveState`，以截图为准 |
| R6 | macOS 26 / 13–25 / 27 三个版本分支均无法在本机验证 | M1 的 macOS 验收**完全依赖用户或 CI 之外的真机** | 明确告知：Windows 侧可交付，macOS 侧只能交付"编译通过 + 代码正确"，**不能声称视觉通过** |
| R7 | `prefers-reduced-transparency` 在 WKWebView 的支持未知 | M4 若只写 `@media`，可能在 macOS 上完全无效 | **已确认（2026-10-22）**：WKWebView 不支持（MDN BCD `safari.version_added: false`，WebKit bug 175497，Safari 26.0 特性清单无此项）。已按预案补应用内"减少透明效果"开关（`AppConfig.reduce_transparency` + Rust 三路径 + 设置 UI），两条触发源取「或」。详见 DECISIONS D6 |
| R8 | 正文对比度无法用单测证明 | 规格的 4.5:1 门槛只能靠实机截图采样 | M2/M5 必须产出"纯白/纯黑/高饱和/纹理"四类背景的实测截图与采样值 |

---

## 7. 平台验证缺口（诚实声明）

| 能力 | Windows（本机） | macOS |
|---|---|---|
| 编译 / fmt / clippy / 单测 | ✅ 已实测 | ⚠️ 仅 CI（编译+单测），**无本机** |
| Acrylic 材质真实生效 | ✅ 已实测（DWM 属性 + 视觉） | — |
| Popover / Liquid Glass 渲染 | — | ❌ **完全未验证** |
| 原生 appearance 与 CSS 一致性 | ⚠️ 可验证 DWM 分支 | ❌ **完全未验证** |
| `prefers-reduced-transparency` | ✅ 已实测（媒体查询 + 应用内开关两条路径） | ❌ **平台不支持**（非"未验证"：MDN BCD `safari.version_added: false`，WebKit bug 175497） |
| 对比度达标 | ✅ 已实测（2026-10-22，见 DECISIONS R-D / R-E）：浅色与 reduce 全过；深色 normal 剩 18 条触 Acrylic 上限 | ❌ |

**结论**：M1 与 M3 的 macOS 部分，本机**无法给出视觉验收结论**。任何"M 里程碑完成"的汇报都必须把 macOS 标为"未运行 / 需平台实测"，不得写成通过。

---

## 8. 基线结论

1. 规格第 2 节的 P0/P1/P2 缺陷**全部成立**，并补充了 4 条规格未覆盖的新发现（§4）。
2. 其中 **§4.1（材质失败静默）** 与 **§4.4（材质状态可能与 focusable 冲突）** 会直接影响 M1 的"能否确认修好了"，建议提升优先级。
3. 当前基线**全部命令通过**，可以安全地在 `d3ed69a` 之上开始 M1。
4. 本阶段**未修改任何产品代码**；`git diff --stat` 为空。
