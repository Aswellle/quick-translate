# Liquid Glass 专项 · 集成关系图（INTEGRATION MAP）

> 审计日期：2026-10-22
> 对应基线：`d3ed69a`（树等同 `origin/main` `7d10376`）
> 配套文档：`BASELINE.md`
> 标注约定：**[S]** = 已由源码证实；**[P]** = 必须在真实 macOS/Windows 上验证，源码无法下结论。

---

## 1. 总览：五条链路

浮窗的"看起来怎么样"由五条彼此独立的链路共同决定。它们**没有统一的中枢**，这是本专项最主要的结构性风险。

```
① 原生材质链路   Rust 窗口构建 → OS 合成器 → 桌面像素被模糊
② CSS 衬底链路   前端 CSS 变量 → 半透明色 + 高光 → 叠在原生材质之上
③ 明暗主题链路   设置页 → Rust 广播 → 各窗口 .dark class
④ 原生外观链路   .dark class → IPC → DWM（Win）/ 无（macOS）
⑤ 显示时序链路   建窗(visible?) → 施加材质 → show()
```

①③ 的产物在 ②④ 处汇合，⑤ 决定用户在首帧看到的是"完整结果"还是"中间态"。

---

## 2. 链路 ①：原生材质

### 2.1 调用图 [S]

```
lib.rs::setup()  (第 273 行起 tray / 第 293-312 行 onboarding retry)
  └─ ensure_popup_window(app)                      translation_flow.rs:111
       ├─ app.get_webview_window("popup") 存在？
       │    └─ 是 → reuse 分支 (translation_flow.rs:156-162)
       │            set_position → set_focusable(false) → show()
       │            ⚠ 不再调用 apply_popup_material
       └─ 否 → build_popup_window(app, pos, /*visible=*/true)   translation_flow.rs:82
                 ├─ WebviewWindowBuilder::new(...)
                 │    .decorations(false)
                 │    .always_on_top(true)
                 │    .skip_taskbar(true)
                 │    .focusable(false)
                 │    .transparent(true)
                 │    .visible(true)   ← 窗口在此已可见
                 │    .build()?        ← OS 窗口 + webview 创建完毕
                 └─ apply_popup_material(&window)          translation_flow.rs:39
                      └─ window.set_effects(effects)        tauri 2.10.3
                           └─ run_on_main_thread(|| {
                                 let _ = vibrancy::set_window_effects(...)   ← 错误被丢弃
                              })
                                └─ window-vibrancy 0.6.0
                                     ├─ Windows: apply_acrylic
                                     │    └─ Win11 22H2+ → DWMWA_SYSTEMBACKDROP_TYPE
                                     │                     = DWMSBT_TRANSIENTWINDOW
                                     │       （color 参数被忽略）
                                     └─ macOS: apply_vibrancy(Popover, FollowsWindowActiveState)
```

### 2.2 当前参数 [S]

| 平台 | 声明 | 代码位置 |
|---|---|---|
| Windows | `Effect::Acrylic` + `color(255,255,255,18)` | `translation_flow.rs:43-47` |
| macOS | `Effect::Popover` + `FollowsWindowActiveState` + `radius(14)` | `translation_flow.rs:49-59` |

### 2.3 关键结论

- **[S] 材质施加失败不可观测**：`set_effects` 内层 `let _ =` 吞掉错误，`vibrancy::set_window_effects` 恒返回 `Ok(())`。应用层 `if let Err` 的 `popup_material_unavailable` 日志**几乎不可能触发**。tauri 2.12.5 依然如此。→ 必须改用 OS 版本探测 + 显式日志。
- **[S] Windows 颜色参数无效**：Win11 22H2+ 走 `DWMWA_SYSTEMBACKDROP_TYPE`，Acrylic 的 tint 不参与。透明窗口背后的模糊强度由系统决定，应用只能通过 CSS 叠层调观感。
- **[S] macOS `Focusable(false)` + `FollowsWindowActiveState` 语义冲突**：窗口永不成为 key window，`NSVisualEffectView` 很可能长期渲染 inactive（更淡）外观。**[P] 需真机对照 `EffectState::Active`**。
- **[S] `clear_effects` 缺失**：tauri 2.10.3 的 `set_window_effects` 不先清理旧材质；重复调用会在 macOS 上叠加 subview（window-vibrancy 每次插入新视图）。当前代码只在建窗时调用一次，暂时无害，但 M1 若改为"主题切换时重施材质"就必须先升级到 2.12.5（其 `apply_effects` 会先 `clear_effects`）。

---

## 3. 链路 ②：CSS 阅读衬底（L0–L3 分层）

### 3.1 当前分层 [S]

规格提出的 L0–L3 四层模型，与现有 CSS 的对应关系：

```
┌──────────────────────────────────────────────────────────────┐
│ L3 · 内容层  ResultView / LoadingView / ErrorView             │
│      z-index: 2 (.popup-container > *)                        │
│      直接坐在 L1 高光之上，无独立阅读衬底 ← 规格要求补 L2.5    │
├──────────────────────────────────────────────────────────────┤
│ L2 · 高光层  .popup-container::before  (globals.css:196-217)  │
│      radial-gradient 左上角白色 38% + 竖向 linear-gradient     │
│      + inset 1px 白色顶边高光                                 │
│      z-index: 1                                               │
├──────────────────────────────────────────────────────────────┤
│ L1 · 底色层  .popup-container { background: var(--popup-bg) } │
│      light rgba(255,255,255,0.10)                             │
│      dark  rgba(28,28,30,0.16)                                │
│      + backdrop-filter: blur(30px) saturate(185%)             │
│        ← 对 OS 合成后的像素无效，仅作无原生材质平台的兜底      │
├──────────────────────────────────────────────────────────────┤
│ L0 · 原生材质  Acrylic / Popover / (未来 Liquid Glass)        │
│      由 Rust 施加，前端不可见、不可控                          │
└──────────────────────────────────────────────────────────────┘
```

另有 `.popup-container::after`（`globals.css:219-227`）提供内描边，`z-index: 1`，与 `::before` 同层。

### 3.2 问题 [S]

- **L3 直接压在 L1 上**：`ResultView.tsx:59` 的译文只有 `text-[var(--text-primary)]`，没有自己的衬底。当桌面背景是高饱和或高纹理时，L1 + L2 的透过率（浅色 0.10 白 + 高光）不足以稳定支撑 4.5:1。
- **`--popup-bg` 的双重角色**：它既当"阅读衬底"又当"玻璃染色"。提高不透明度会同时削弱原生材质观感——这是规格要求拆分 `--popup-reading-*` 的直接原因。
- **无 `prefers-reduced-transparency`**：全文件唯一 `prefers-reduced` 规则是 `globals.css:620-626` 的 reduced-motion。**[S]**（grep 证实）。WebView2/Chromium 155 支持该媒体查询 **[S·本机 Edge 实测]**；WKWebView 支持情况 **[P]**。
  > **后续（2026-10-22）**：WKWebView 已确定**不支持**（MDN BCD `safari.version_added: false`，WebKit bug 175497）。已补应用内开关，见 DECISIONS D6。

### 3.3 目标分层（M2 后）

```
L3  内容层
L2.5  .popup-reading-surface   ← 新增：独立不透明度 + 可选微弱模糊
L2  ::before 高光
L1  .popup-container 底色（降为纯染色）
L0  原生材质
```

`--popup-reading-bg` 与 `--popup-bg` 分离，前者可独立调到 0.55–0.72 而不动玻璃染色。

---

## 4. 链路 ③④：主题广播与原生外观

### 4.1 完整调用图 [S]

```
[设置页] SettingsWindow.tsx:490-513
   onChange(theme)
     ├─ onChange("theme", v)                  // 本地 store
     ├─ applyTheme(v)                         // 立即改本窗口 <html>.dark
     └─ setConfigBatch([["theme", v]])        // IPC → Rust
            └─ commands/config.rs::set_config_batch
                 └─ handle_side_effect("theme", value)      config.rs:185-225
                      └─ emit_theme_changed(app, value)
                           ├─ app.emit("theme-changed", { theme })   // 全窗口广播
                           └─ tracing::info!("主题已切换: {}", theme)

[各窗口] useTheme.ts
   ├─ effect: listen("theme-changed") → applyTheme(payload.theme)
   │             └─ documentElement.classList.toggle("dark", ...)
   └─ effect (仅 system 模式):
        setInterval(() => applyTheme("system"), 2000)     useTheme.ts:50-62
        + matchMedia("(prefers-color-scheme: dark)").onchange

[浮窗] PopupWindow.tsx:345-371
   MutationObserver(documentElement, { attributes, attributeFilter:["class"] })
     └─ sync()  ← 只在 dark 值变化时触发（lastDark 去重）
          └─ setPopupTheme(dark)                 commands.ts
               └─ invoke("set_popup_theme", { dark })
                    ├─ [Windows] commands/system.rs:150-172
                    │    DwmSetWindowAttribute(hwnd,
                    │      DWMWA_USE_IMMERSIVE_DARK_MODE, &dark_val)
                    │    ← 让 DWM 亚克力跟随应用主题（而非系统主题）
                    └─ [macOS]  commands/system.rs:174-178
                         async fn set_popup_theme(_dark: bool) { Ok(()) }
                         ← 空实现：NSVisualEffectView 的明暗本应由
                            窗口 appearance 驱动，但当前无人设置 appearance
```

**另有一条冗余轮询**：`PopupWindow.tsx:361-365` 每 2s 调一次 `sync()`，与 `useTheme` 的 2s 轮询并行。`lastDark` 去重使 IPC 不会重复，但检查是双份的。

### 4.2 平台差异矩阵 [S]

| 环节 | Windows | macOS |
|---|---|---|
| CSS 明暗（`.dark` class） | ✅ 正常 | ✅ 正常 |
| 原生材质明暗 | ✅ `DWMWA_USE_IMMERSIVE_DARK_MODE` | ❌ **无人设置 appearance** |
| 材质是否跟随系统 | 应用显式覆写为应用主题 | 跟随系统（不可控） |
| 切换后是否需要重建窗口 | 否 | 否（但材质不变） |

**[P] macOS 实测项**：`NSVisualEffectView` 在 `NSWindow.appearance` 未显式设置时，取的是 `NSApp.effectiveAppearance`。而 `tauri::set_theme` 在 macOS 上是**应用级**（`tao` 调 `[NSApplication setAppearance:]`），所以用 `set_theme` 修这个问题会**同时改变所有窗口**的原生外观——这正是 BASELINE R3/R4 的由来。

### 4.3 首帧错色窗口 [S]

`apply_popup_material` 只施加材质，**不带 dark 参数**。Windows 上 `DWMWA_USE_IMMERSIVE_DARK_MODE` 要等前端挂载 → `MutationObserver` 首次 `sync()` → IPC 才写入。

因此在"应用深色 + 系统浅色"组合下：

```
t0  窗口可见，DWM 亚克力 = 系统主题（浅）
t1  webview 加载、React 挂载、CSS 生效（深色文字）
t2  MutationObserver 首次 sync() → IPC → DWM 转深
```

`t0 → t2` 之间存在可见的错色首帧。M1 应在建窗前把 dark 状态传给 `apply_popup_material`，或建窗时用 `EffectsBuilder` 一次性带上。

---

## 5. 链路 ⑤：窗口创建与显示时序

### 5.1 三条建窗路径 [S]

| 路径 | 入口 | `visible` | 材质时机 | 时序正确？ |
|---|---|---|---|---|
| **A · 兜底建窗** | `ensure_popup_window()` `translation_flow.rs:111` | `true` | build 之后 | ❌ **先可见后材质** |
| **B · 加载态建窗** | `show_popup_loading()` `translation_flow.rs:163` | `false` → `show()` | build 之后、show 之前 | ✅ 正确 |
| **C · 复用** | `translation_flow.rs:156-162` | 已存在 | **不重施材质** | ⚠ 依赖首次建窗成功 |

路径 A 的触发点：
- `lib.rs::on_window_event` 的 `CloseRequested` 分支（onboarding 关闭后）
- 启动流程中任何需要确保浮窗存在的调用

### 5.2 目标时序（M1 后）

```
① 读配置得到 dark
② WebviewWindowBuilder::new(...)
     .visible(false)
     .effects(EffectsBuilder::new()
        .effect(LiquidGlassRegular)      // macOS 26+
        .effect(Popover)                 // macOS 15- 回退
        .state(Active)
        .radius(RADIUS_LG)
        .build())
     .build()?
③ window.set_effects(...)   // 仅当需要按 dark 动态调整时
④ window.show()
```

关键点：`effects()` 在 builder 阶段施加，**先于窗口可见**，且 tauri 2.12.5 的 `apply_effects` 会先 `clear_effects`。

**[P] 未验证**：`WebviewWindowBuilder::effects()` 在 macOS 上是否真的在 `show()` 之前完成材质合成。文档未承诺，需真机录屏逐帧确认。

---

## 6. 平台验证缺口

| # | 结论 | 状态 | 验证方式 |
|---|---|---|---|
| 1 | Windows Acrylic 生效 | [S] 已实测 | DWM 属性查询 `backdrop=3` + 视觉 |
| 2 | macOS Popover 生效 | **[P] 未验证** | 真机截图 |
| 3 | macOS Liquid Glass 生效（26+） | **[P] 未验证** | 真机（需 macOS 26 设备） |
| 4 | macOS `FollowsWindowActiveState` 导致材质偏淡 | **[P] 假设** | 真机对比 `Active` |
| 5 | 首帧材质先于可见 | **[P] 未验证** | 真机逐帧录屏 |
| 6 | `prefers-reduced-transparency` 在 WKWebView 支持 | **已确定：不支持**（MDN BCD + WebKit bug 175497） | 已补应用内开关，见 D6 |
| 7 | 4.5:1 对比度达标 | **已实测（2026-10-22）**：浅色与 reduce 全达标；深色 normal 剩 18 条触 Windows Acrylic 合成上限 | 6 背景 × 浅深 × normal/reduce 全屏 BitBlt 采样，见 DECISIONS R-D / R-E |
| 8 | 交通灯 focus-visible 无图标 | [S] 已证实（CSS 缺规则） | 源码 + 键盘实测 |
| 9 | 状态点仅靠颜色传达状态 | [S] 已证实（`aria-hidden`） | 源码 |

---

## 7. M1–M5 修改范围与验收条件

> 以下为**计划**，本阶段不执行。每个里程碑独立成提交，可单独回滚。

### M0 · 基线固化（已完成）

**范围**：本文档 + `BASELINE.md` + `evidence/` 截图矩阵。
**验收**：
- [x] 六条基线命令全通过
- [x] 缺陷逐条源码定位
- [ ] 目标设备截图矩阵（Windows 浅/深 × 展开/折叠 × 成功/错误/加载 = 12 张，macOS 同）——**待补**
- [x] 未修改产品代码（`git diff --stat` 为空）

### M1 · 依赖升级 + Liquid Glass 接入 + 时序修正

**范围**：
- `src-tauri/Cargo.toml`：`tauri` 2.10.3 → 2.12.5（连带 `tauri-build` 2.7.4、`tauri-utils` 2.10.2、`window-vibrancy` 0.8.1、`wry` 0.57、`tao` 0.37）
- `package.json`：`@tauri-apps/api` / `@tauri-apps/cli` → 2.12.4
- `src-tauri/src/system/translation_flow.rs`：
  - `apply_popup_material` 改为 macOS 同时声明 `LiquidGlassRegular` + `Popover` 回退
  - `EffectState` **保持** `FollowsWindowActiveState`（观感取向为淡，inactive 外观正合预期；见 DECISIONS D4）
  - 三条建窗路径统一 `visible(false)` → 材质 → `show()`
  - 增加 OS 版本探测日志（替代失效的 `popup_material_unavailable`）

**验收**：
- [ ] `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test`（231）全绿
- [ ] `npx tsc --noEmit` / `npm test` / `npm run build` 全绿
- [ ] Windows 实机：Acrylic 仍生效，§9 回归清单全过
- [ ] macOS 26 实机：玻璃材质出现 **[P]**
- [ ] macOS 15- 实机：回退到 Popover **[P]**
- [ ] 首帧无错色、无"先白后玻璃" **[P]**
- [ ] **不得**声称 macOS 视觉通过（除非有真机截图）

> 当前无 macOS 设备（DECISIONS §3.1）。M1 的 macOS 三项验收标记为"待设备"，不阻塞 Windows 侧交付。

### M2 · CSS 阅读衬底分层

**范围**：
- `src/styles/globals.css`：新增 `--popup-reading-bg` / `--popup-reading-blur`，`--popup-bg` 降为纯染色
- 新增 `.popup-reading-surface` 类
- `ResultView.tsx` / `ErrorView.tsx` / `LoadingView.tsx` 套用该类
- `// PLATFORM: cjk-*` 标记保持不动

**验收**：
- [x] 正文对比度 ≥ 4.5:1 —— 实测 0/24 失败（`--popup-text-primary` 在六类背景全过）
- [x] 浅色 / 深色两套主题均达标（浅色 normal 最低 4.92:1，两侧 reduce 6.94 / 7.48）
- [x] 玻璃观感未被加厚 —— normal 的 `--popup-glass-tint` 仍是 0.10 / 0.16
- [ ] **深色 normal 未达标**：18/168 条，最差 2.92:1。经实测确认卡在 Windows Acrylic
      的合成上限（白/纹理/黄桌面下纯白文字也只有 3.41–3.95:1），需 B 或 E 才能跨过，见 D11
- [x] 无新增 CSS 变量泄漏到非浮窗组件（`--text-secondary` / `--text-tertiary` 均收窄到 `.popup-container` 作用域内）

### M3 · macOS 原生外观同步

**范围**：
- `src-tauri/Cargo.toml`：`objc2-app-kit` 增加 `NSWindow`、`NSAppearance` features
- `src-tauri/src/commands/system.rs`：macOS 分支由空实现改为 `ns_window()` + `NSWindow::setAppearance`，在 `run_on_main_thread` 内执行
- **必须保留** `dark` 参数键（`_dark` → `dark` 需处理 unused 告警）

**验收**：
- [ ] macOS：应用主题切换后，浮窗原生材质明暗随之变化 **[P]**
- [ ] 其他窗口（settings/history）的原生外观**不受影响** **[P]**
- [ ] `theme = system` 时，系统主题切换仍能被检测 **[P]**
- [ ] IPC 契约两侧参数键一致（前端 `{ dark }`）

**决策点**：若选应用级 `set_theme` 方案（BASELINE R3 方案①），则本里程碑验收改为"接受全窗口变化 + 验证系统跟随不失效"。

### M4 · 可访问性与降级

**范围**：
- `globals.css`：`.traffic-lights:focus-within .traffic-light:focus-visible svg { opacity: 1 }`
- `PopupWindow.tsx`：状态点补可访问文本（`aria-label` 或 `sr-only`）
- `globals.css`：新增 `@media (prefers-reduced-transparency: reduce)` 规则
- 若 WKWebView 不支持该媒体查询 → 补应用内开关（`AppConfig` + Rust 默认值 + 设置 UI 三处同步）

**验收**：
- [ ] 键盘 Tab 到交通灯，图标可见
- [ ] 屏幕阅读器可读出当前状态（成功/加载/错误/空闲）
- [ ] `prefers-reduced-transparency: reduce` 下衬底不透明度提升
- [ ] reduced-motion 现有行为未被破坏

### M5 · 测试与视觉验收

**范围**：
- 新增 Vitest：主题同步（`lastDark` 去重）、`isDragBlockingTarget` 在浮窗中的行为、折叠态摘要截断
- 新增 Rust 单测：材质参数构建（若抽成纯函数）
- `evidence/` 截图矩阵补全

**验收**：
- [ ] 前端用例从 25 → ≥ 35，全部通过
- [ ] 截图矩阵 Windows 12 张 + macOS 12 张齐备
- [ ] 对比度采样值记录在 `evidence/contrast.md`

---

## 8. 文件影响矩阵

| 文件 | M1 | M2 | M3 | M4 | M5 |
|---|---|---|---|---|---|
| `src-tauri/Cargo.toml` | ✎ | | ✎ | | |
| `src-tauri/Cargo.lock` | ✎ | | ✎ | | |
| `package.json` | ✎ | | | | |
| `src-tauri/src/system/translation_flow.rs` | ✎ | | | | |
| `src-tauri/src/commands/system.rs` | | | ✎ | | |
| `src/styles/globals.css` | | ✎ | | ✎ | |
| `src/windows/popup/ResultView.tsx` | | ✎ | | | |
| `src/windows/popup/ErrorView.tsx` | | ✎ | | | |
| `src/windows/popup/LoadingView.tsx` | | ✎ | | | |
| `src/windows/popup/PopupWindow.tsx` | | | | ✎ | |
| `src/lib/commands.ts` | | | ✎ | | |
| `src-tauri/tauri.conf.json` | ⚠ 视 `minimumSystemVersion` 决策 | | | | |
| 新增测试文件 | | | | | ✎ |

⚠ `tauri.conf.json`：当前**未固定** `minimumSystemVersion`。Liquid Glass 需 macOS 26+，但回退链保证 15- 可用，因此**不必**提高最低版本。若提高，会切断旧客户端升级路径——**建议不动**。

---

## 9. 回归清单（每个里程碑都要跑）

### 9.1 自动化

```bash
npx tsc --noEmit
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

### 9.2 实机交互（Windows 必做，macOS 尽力）

| # | 场景 | 期望 |
|---|---|---|
| 1 | 被动态显示 | 不抢焦点，光标所在窗口保持激活 |
| 2 | 点击浮窗 | 激活；键盘快捷键生效；`activated` 提示出现 |
| 3 | Esc 关闭 | 关闭且不残留 |
| 4 | 空格关闭 | 关闭；在可编辑区/有选中文本/按住重复键时**不**关闭 |
| 5 | 鼠标拖动 | >4px 视为拖动，窗口跟随；≤4px 视为点击激活 |
| 6 | 红黄绿按钮 | 关闭 / 折叠 / 切换宽度 |
| 7 | 宽度切换 | 400 ↔ 520，重定位正确，文本不跳变 |
| 8 | 折叠摘要 | 44px 内状态点 + 摘要 + 语言方向完整 |
| 9 | 自动量高 | 两段式测量，内容短时窗口收窄，长时 ≤480 封顶 |
| 10 | 窗口定位 | 右下列优先，贴近屏幕边缘时正确翻转 |
| 11 | 点击外部 | 关闭 |
| 12 | 前台窗口切换 | 关闭 |
| 13 | 主题切换（设置页） | 浮窗 CSS + 原生材质同步 |
| 14 | 系统主题切换（`theme=system`） | 2s 内跟随 |
| 15 | 重新复制触发翻译 | 复用窗口，不重建，不闪烁 |

### 9.3 现有测试覆盖对照

| 场景 | 现有测试 |
|---|---|
| 1–2 | ❌ 无（`popup_watch` 覆盖 `should_close_interactive` 逻辑，非端到端） |
| 3–4 | ❌ 无 |
| 5 | ⚠ 仅 `domUtils.test.ts` 覆盖 `isDragBlockingTarget` |
| 6–7 | ❌ 无 |
| 8 | ❌ 无 |
| 9 | ❌ 无 |
| 10 | ✅ `popup_geometry` 19 个用例 |
| 11–12 | ✅ `popup_watch` 16 个用例 |
| 13–14 | ❌ 无 |
| 15 | ❌ 无 |

**结论**：15 个场景中，自动化覆盖 2 个（10、11–12），部分覆盖 1 个（5），**12 个完全依赖人工**。M5 的测试补充应优先覆盖 8、9、13。

---

## 10. 结论与建议

1. **五条链路无统一中枢**是本专项的根本复杂度来源。M1–M3 实际上是在为它们建立契约（材质参数、衬底层级、外观同步），而不是"换个 API"。
2. **M1 是风险最高的一步**：Tauri 主版本内跨 6 个 minor、`wry`/`tao` 各跨 3 个 minor。建议单独成 PR，先只升级不接 Liquid Glass，跑完全部回归后再加材质参数。
3. **macOS 侧全部结论为 [P]**：本机无法验证，任何汇报必须显式标注"未运行"。
4. **建议下一阶段**：先做 **M1 的第一步——纯依赖升级**（不含 Liquid Glass 参数），验证 `wry`/`tao` 跃迁没有破坏浮窗的焦点、拖动、透明行为。这一步的回归结果决定 M1 剩余部分是否可行。
