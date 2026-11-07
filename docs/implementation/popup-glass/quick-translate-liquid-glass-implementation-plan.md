# QuickTranslate 翻译悬浮窗：原生 Liquid Glass、主题同步与可读性专项实施规格

> **目标仓库**：<https://github.com/Aswellle/quick-translate>  
> **审计分支**：`main`  
> **审计基准**：`7d10376c94b809938d3ef7e4104d00d4fe9aada9`（2026-10-21 的仓库 HEAD）  
> **文档用途**：交给 Codex / Claude Code 等 AI 编程 Agent 直接执行  
> **实施方式**：先复现与建立基线，再按里程碑实现；不要一次性重构悬浮窗生命周期

---

## 0. 给 AI Agent 的强制执行入口

本文件是实施规格，不是要求 Agent 不经审查便逐字照抄的代码补丁。开始前必须以工作区最新代码为准重新核实本规格的文件路径和行为。仓库可能已经在审计基准之后有新提交。

### 0.1 第一个任务：只审计，不写功能代码

1. 检查 `git status --short`、当前分支、HEAD commit、`AGENTS.md`、`CLAUDE.md` 和其他项目级规则。不得覆盖用户未提交的工作。
2. 阅读本规格引用的全部代码，至少包括：
   - `src/windows/popup/PopupWindow.tsx`
   - `src/windows/popup/TrafficLights.tsx`
   - `src/windows/popup/ResultView.tsx`
   - `src/windows/popup/LoadingView.tsx`
   - `src/windows/popup/ErrorView.tsx`
   - `src/styles/globals.css`
   - `src/hooks/useTheme.ts`
   - `src/stores/configStore.ts`
   - `src/windows/settings/SettingsWindow.tsx`
   - `src/lib/commands.ts`
   - `src-tauri/src/system/translation_flow.rs`
   - `src-tauri/src/commands/system.rs`
   - `src-tauri/src/system/popup_geometry.rs`
   - `src-tauri/src/system/popup_watch.rs`
   - `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json`、`src-tauri/capabilities/default.json`
3. 记录当前实际的 Tauri / Rust / Node / npm 版本；不能把 `Cargo.toml` 中的宽松版本范围当成已安装版本。审计基准的 `Cargo.lock` 中 `tauri` 为 `2.10.3`。
4. 本阶段运行当前工作区适用的基线命令，并保存结果：

   ```bash
   npm test
   npm run build
   npx tsc --noEmit
   cd src-tauri && cargo fmt --check
   cd src-tauri && cargo clippy -- -D warnings
   cd src-tauri && cargo test
   ```

   如当前环境不能在某一平台执行对应测试，记录为“未运行 / 需平台实测”，绝不写成“通过”。
5. 在实际目标设备上记录当前浮窗截图或屏幕录制：系统主题、应用主题、展开/折叠、短/长译文、加载和错误状态。尤其要确认“系统效果是否真正采样了桌面/其他窗口”，而不是只看 CSS 高光是否漂亮。
6. 产出 `docs/implementation/popup-glass/BASELINE.md` 和 `docs/implementation/popup-glass/INTEGRATION_MAP.md`，审阅后再进入里程碑 1。

### 0.2 不可突破的架构边界

- 保留现有 Tauri 2 + React 19 + TypeScript + Zustand 架构；不引入新的 UI 框架、全局 store、CSS-in-JS 系统或视觉依赖，除非审计证明现有技术路径不可行。
- 只改造翻译悬浮窗视觉与主题同步相关路径，不改写翻译源回退、剪贴板监控、请求 latest-wins、历史持久化等无关业务逻辑。
- 必须保留：被动态显示不抢焦点、点击后才进入交互态、Esc/空格关闭规则、红黄绿灯按钮语义、拖动与记忆位置、双宽度切换、折叠后摘要显示、窗口自动量高和边界定位。
- 原生材质失败时必须存在可读的降级样式。不能因视觉效果 API 失败导致翻译窗口不可读、不可关闭或不能显示内容。
- 不要在单次提交中同时升级大批不相关依赖。Tauri 升级必须单独成一个可审查的提交或明确的里程碑。
- 不触发 release workflow、不打 tag、不上传签名包、不替用户发布生产版本。

---

## 1. 目标与成功定义

本专项不是“让背景更透明”这么简单，而是让悬浮窗在真实桌面背景、不同系统材质、深浅主题、内容状态和交互状态下都保持三个属性：

1. **真正的系统级玻璃材质**：能观察到桌面背景的模糊与色彩传递、边缘高光和层次深度，而不是只用 `rgba()` 与渐变伪装成玻璃。
2. **内容优先且可读**：译文是视觉主角。不能为了展示透明度而牺牲正文、语言方向、状态、按钮和错误提示的可识别性。
3. **视觉、主题和窗口生命周期一致**：CSS 的深浅主题、macOS 原生材质的 effective appearance、Windows DWM 材质明暗一致；展开、折叠、切换阅读宽度、翻译状态变化不造成闪白、错色、裁切、元素消失或窗口交互回归。

### 1.1 完成判定（Definition of Done）

- macOS 26 及以上版本使用原生 Liquid Glass **Regular** 材质；支持版本更旧的 macOS 有经过验证的原生材质回退。
- Windows 继续使用已经建立的原生 Acrylic 路径，除非基准测试证明需要调整；CSS 负责内容层和可读性，不冒充系统桌面模糊。
- 应用设置为浅色、深色、跟随系统时，CSS 和系统材质保持一致。必须覆盖“应用浅色 + 系统深色”和“应用深色 + 系统浅色”这两个最容易暴露错色的组合。
- 正文/重要提示在视觉上可立即读取；正文颜色对其实际渲染底色达到 WCAG AA 对比度门槛，普通文字至少 `4.5:1`，大型文字至少 `3:1`。对透明背景不能仅根据 CSS 文本色与设计稿的纯色样片宣称合格，必须用目标平台实机截图验证。
- 折叠条的状态、译文摘要、语言方向不会互相挤掉；没有译文时仍有明确、可理解的状态反馈。
- 鼠标 hover 不是识别关键操作或重要图标的唯一途径；键盘焦点态有可见指示。
- 系统减少透明效果/提高对比度等可用性设置生效时，悬浮窗可切换到更稳固的衬底，而不是强迫用户接受高透明玻璃。
- 既有交互与窗口定位行为无回归；所有单元测试、构建与 Rust 检查按平台可运行范围通过。

---

## 2. 代码审计结论：当前已有什么、真正缺什么

> 以下是基于上述审计基准的源码观察，不代表未经复核的后续提交状态。

### 2.1 已有基础（尽量复用）

| 现有模块 | 当前职责与代码证据 | 实施要求 |
|---|---|---|
| `src-tauri/src/system/translation_flow.rs` | `build_popup_window()` 集中构建悬浮窗；启用 `decorations(false)`、`always_on_top(true)`、`skip_taskbar(true)`、`focusable(false)`、`transparent(true)`；`apply_popup_material()` 给 Windows 施加 `Effect::Acrylic`，macOS 施加 `Effect::Popover` | 保留单一建窗入口和窗口属性契约；增强 macOS 原生材质并保证先应用材质再显示窗口 |
| `src/styles/globals.css` | 已有浅色/深色语义 token；`.popup-container` 背景、边框、阴影及 `::before` / `::after` 光学层；折叠摘要、状态点、交通灯及入场动画都有独立样式 | 保留分层思路；重新分离“外层玻璃材质”与“正文可读性底层”，不要把所有信息放在同一层透明背景上 |
| `src/windows/popup/PopupWindow.tsx` | 管理 `collapsed`、`wide`、`activated`、`closing` 与翻译状态；按内容高度调用 `resizePopup()`；`MutationObserver` 观察 `<html>` 的 `.dark` 并调用 `setPopupTheme()` | 不改变请求/键盘/窗口生命周期；收敛材质主题同步，处理首次显示与系统效果时序 |
| `src/hooks/useTheme.ts` | 根据配置的 `theme`（`system` / `light` / `dark`）修改根节点 `.dark`；监听 `theme-changed` 跨窗口事件；系统模式以 `matchMedia` + 轮询兜底 | 继续作为应用 CSS 的主题事实来源；不要另建第二套脱离配置 store 的主题状态 |
| `src/windows/settings/SettingsWindow.tsx` | 常规设置页有“界面主题”选择，保存后调用 `applyTheme` 与 `setConfigBatch` | 主题选择仍是唯一用户偏好入口；如新增“减少透明效果”选项，需持久化且保持设置行为一致 |
| `src-tauri/src/commands/system.rs`、`src/lib/commands.ts` | 有 `set_popup_theme(dark: boolean)` IPC；Windows 用 `DWMWA_USE_IMMERSIVE_DARK_MODE`；非 Windows 实现目前是空操作 | 修复 macOS 分支的主题同步，而不破坏前端 `invoke("set_popup_theme", { dark })` 契约，除非同步更新两侧调用和测试 |
| `src/windows/popup/ResultView.tsx` | 正文约 `14.5px`、中等字重、`1.8` 行高，最多 `360px` 高并允许纵向滚动；语言方向与操作元信息使用语义色 | 保留正文排版的成熟设定；重点改善正文实际衬底和次级信息的对比度，而非盲目把字号调得很大 |
| `src/windows/popup/TrafficLights.tsx` | macOS 红黄绿语义按钮；CSS 默认隐藏 glyph，鼠标悬停时显示；有 `aria-label` 与 `focus-visible` 样式 | 补齐键盘焦点下 glyph 的可见性，验证紧凑高度和浅/深主题中的按钮识别 |
| `src-tauri/src/system/popup_geometry.rs` | 后端唯一管理尺寸契约、边界与位置；圆角基准 `RADIUS_LG = 14.0` | 玻璃改造后保持圆角/尺寸契约一致，不把 CSS 动画引入窗口尺寸测量竞态 |

### 2.2 主要缺陷与风险（按优先级）

#### P0：macOS 并未选择 macOS 26 原生 Liquid Glass API

- 当前 `Cargo.lock` 中 `tauri = 2.10.3`。
- `translation_flow.rs` 的 macOS 分支使用 `Effect::Popover`，它是原生弹出层材质，不等于 macOS 26 引入的 Liquid Glass Regular / Clear 样式。
- Tauri 的 `LiquidGlassRegular` 与 `LiquidGlassClear` 在 macOS 26+ 可用；Tauri `2.12.0` 的发行说明引入了 Liquid Glass window effects。应将 Tauri 2.12.x 升级作为独立兼容性里程碑，并锁定经过 CI 验证的具体依赖版本。
- 悬浮窗是正文阅读型 popover，优先采用 **Regular**，不是 Clear。Apple 的材料设计指南明确建议：大量文本、弹出面板、侧边栏等更适合 Regular；Clear 更强调底下的丰富视觉内容，必要时还要加调暗层。

#### P0：macOS 的 `set_popup_theme()` 当前是空实现

`src-tauri/src/commands/system.rs` 的 `#[cfg(not(windows))]` 分支只返回 `Ok(())`。这意味着前端确实同步了 `.dark` class，但该 IPC 并没有把 macOS 原生窗口的 appearance 切到与 CSS 一致的明暗模式。若用户应用主题与系统主题相反，原生材质和 CSS 可能出现明暗不一致。

**注意**：不要简单把 Rust 的 `_dark` 改名为 `dark` 后就认为修复完成。现有命名及 Tauri command 参数序列化有专门注释；要保留或明确测试 `invoke(..., { dark })` 的参数映射。

#### P1：目前正文主要依赖文本色 token，不能保证真实玻璃上的对比度

`globals.css` 的外层 `--popup-bg` 是非常薄的透明着色（审计基准浅色约 `rgba(255,255,255,0.10)`，深色约 `rgba(28,28,30,0.16)`）；光学层用 `::before` 的高光渐变支撑文字衬底。其外观强烈依赖背后的窗口和桌面内容。

问题不是这些数值必然错误，而是：**文本对比度不能只根据 `--text-primary` 的十六进制颜色推断**。同一文字可能叠在很亮、很暗、鲜艳或低纹理的桌面区域上。把外层 tint 直接改回更高不透明度，可能又复现“雾白板/色块”的历史问题，并损害真正的玻璃感。

#### P1：CSS `backdrop-filter` 不能替代跨窗口桌面模糊

当前样式已正确注明：透明 Tauri 窗口背后的桌面像素由 OS 合成，WebView 内部的 `backdrop-filter` 不能可靠地采样其他应用窗口。它可作浏览器渲染降级，但真正的桌面模糊必须由 Windows DWM 或 macOS 原生窗口材质完成。

因此不能用“把 `backdrop-filter: blur()` 从 30px 改到 60px”作为主修复。

#### P1：初次出现时存在原生材质应用时序风险

`build_popup_window()` 目前使用传入的 `visible` 参数创建窗口，之后才调用 `apply_popup_material()`。在需要直接可见的创建路径上，窗口可能先被显示，再施加原生效果，产生短暂的无材质/错误外观首帧。必须核查所有窗口创建路径并统一“先创建为隐藏 → 应用材质和 native appearance → 再显示”的顺序。

#### P1：交通灯 glyph 只在 hover 时显示

`.traffic-light svg` 默认 `opacity: 0`，当前主要由 `.traffic-lights.group:hover ...` 显示。按钮依然有有色圆点，但键盘 `focus-visible` 时若未触发 group hover，图标可能仍然隐藏。要确保焦点本身也会显露对应图标，避免视觉反馈仅依赖鼠标悬停。

#### P1：缺少浮窗视觉/主题的专门自动测试与标准验收矩阵

审计基准中没有看到覆盖 `PopupWindow` 展开折叠、应用主题与 native effect 一致性、不同桌面背景对比度的专门前端测试。现有 `popup_watch.rs` / `popup_geometry.rs` 的 Rust 测试主要覆盖关闭看守和几何定位，并不能证明玻璃效果或对比度合格。必须新增可自动化的逻辑测试和明确的 Windows/macOS 实机检查清单。

#### P2：透明效果和减少动态效果的无障碍降级尚未形成完整契约

CSS media query `prefers-reduced-transparency` 并非所有目标 WebView 都可靠支持。不能只加一条 CSS 规则，就认为 Windows/macOS 系统“减少透明效果”已经被遵守。应增加能力检测，并在无法可靠读取系统偏好时，提供应用内的“减少透明效果”选项作为确定性回退；减少动效模式则应关闭呼吸、缩放/浮动等非必要动画。

#### P2：主题同步存在双重轮询

`useTheme.ts` 已在 `system` 模式下每 2 秒轮询 `matchMedia`，用于处理部分 WebView2 的系统主题事件不可靠问题。`PopupWindow.tsx` 又每 2 秒轮询根节点 class，虽然 `lastDark` 防止重复 native IPC，但形成重复检查。只有在新同步链路与测试稳定后，才移除 popup 的第二份轮询；不能破坏 `useTheme` 对系统主题变化的现有兜底。

---

## 3. 目标视觉架构：分离玻璃和文字衬底

### 3.1 视觉层次

实现必须明确区分以下层级，不能把它们揉成一个半透明的 `.popup-container` 背景：

1. **L0 原生桌面材质层**：真正采样桌面背景并产生系统模糊、色彩融合和随 appearance 变化的 luminosity。
2. **L1 外轮廓玻璃层**：外层圆角、细边、顶部光学高光、阴影。透明度较低，保留玻璃感，不直接承载大段正文的可读性责任。
3. **L2 内容可读层**：正文区域使用独立的阅读衬底/局部调光层，透明度比外轮廓高，负责稳定正文对比度。它不应把整个窗口重新涂成纯色板。
4. **L3 内容与控件**：正文、语言方向、操作栏、复制按钮、错误提示、状态点与交通灯按钮。颜色始终来自统一语义 token，并有 hover / active / focus-visible 状态。

Apple 的 Liquid Glass 指南把 Liquid Glass 定义为浮在内容之上的功能层，并建议在有大量文字的 popover 场景优先采用 Regular；正文内容层应保持结构和阅读层级。因此建议外层保持轻薄、有深度，正文使用局部标准材质/阅读衬底，而不是把整个窗口统一提高不透明度。

### 3.2 色彩与透明度原则

- 不使用父容器 `opacity` 来调节玻璃；这会一并淡化文本、图标和控件，无法独立控制可读性。
- 背景、边框、高光与正文色分别用语义变量控制，避免各组件散落硬编码 `rgba()`。
- 维持当前浅色/深色文本主色的方向，重新检查 `--text-secondary`、`--text-tertiary`、折叠态语言标签及 `kbd-hint` 的实际对比度。玻璃上的辅助文本不能自动降级成“只作装饰”的低对比灰字。
- 为 `.popup-reading-surface`（建议新增的语义类名）分别定义 light/dark 底色、边框与最小可读性状态。初始不透明度只作为实机调校参数，不作为无证据的通过结论。
- 测试对比度时必须检查真实合成后的截图：窗口前景分别置于纯白、纯黑、高饱和、纹理复杂、视频/动态背景等场景；暗背景/亮背景各至少一组。普通文本要求 `4.5:1`，大型文本 `3:1`，交互图形边界/关键状态标识参考 `3:1`。
- 当检测到减少透明效果或更高对比度偏好时，正文区域必须切换到更不透明的阅读底层；高对比度模式不能只调文字颜色而保留难以区分的透明边框。

### 3.3 推荐的材质选择

| 平台/系统 | 主路径 | 回退路径 | 关键约束 |
|---|---|---|---|
| macOS 26+ | Tauri 原生 `Effect::LiquidGlassRegular` | 如果施加失败，回退到 `Effect::Popover` | Regular 适合阅读型浮窗；避免给大量正文使用 Clear 后依靠文字色硬撑对比度 |
| macOS 13–25（项目当前支持范围需保持） | Tauri `Effect::Popover` 原生材质 | 更不透明 CSS 阅读表面；必要时清除失败的 effect | 不能要求旧版系统呈现 macOS 26 完全一致的 Liquid Glass，但功能、主题和可读性必须一致 |
| Windows 10/11 | 继续用当前 `Effect::Acrylic` DWM 路径 | CSS 阅读衬底/适度不透明背景 | 不默认换成 Mica；先测 Acrylic 的性能和视觉。Tauri 文档注明 `EffectsBuilder.color` 在 Windows 11 不起同样作用，不能依赖它保证对比度 |
| 不支持/原生效果失败的平台 | 透明/半透明 CSS 背景降级 | 高可读性、更接近不透明的窗口面板 | 记录诊断信息；不得空白、黑板、不可读或影响关闭 |

**版本要求**：审计基准的 Tauri 是 `2.10.3`，而 `LiquidGlassRegular` 在 Tauri `2.12.0` 增加。建议先把 Rust 侧 Tauri / tauri-build 与 npm 侧 Tauri CLI 升级到经过验证的兼容 `2.12.x` 版本，更新并提交对应 lockfile。不要只改 `Cargo.toml` 的版本范围却仍编译旧 `Cargo.lock`。

### 3.4 macOS Liquid Glass 的实现细节

在 `src-tauri/src/system/translation_flow.rs` 中封装一个可测试的材质构建函数，Mac 26+ 优先列出 `LiquidGlassRegular`，并把 `Popover` 作为旧系统回退项。Tauri 当前窗口 effect API 支持在 macOS 同时声明一个 Liquid Glass 样式和一个旧式 Visual Effect material 作为旧系统回退；Agent 应先核实升级后 `EffectsBuilder` API 的实际签名，再实现。

- 保留 `radius(popup_geometry::RADIUS_LG)`，保持 Rust 与 `--radius-lg` 的 `14px` 一致。
- 对 Regular 材质保留原生明暗自适应语义；不要手工叠加大量白色渐变把系统材质洗成雾白板。
- `EffectState` 对 Liquid Glass 的语义与旧 Popover 不完全相同，升级后以当前 Tauri API 文档为准，不要假定 `FollowsWindowActiveState` 对两者完全同义。
- 可考虑只在 macOS 27+ 启用 native interactive glass interaction；它不是功能正确性的前提，也不应在普通折叠/动画上叠加过多动效。
- 如果需要通过 AppKit 强制窗口 appearance，必须走主线程，并优先用 Tauri 已有窗口 API。只有确认 Tauri API 不足时，才为 `objc2-app-kit` 精确启用 `NSWindow` / `NSAppearance` 等所需 feature；不要为了材质重写整个 native window。

### 3.5 Windows 材质的实现细节

- 延续 `Effect::Acrylic` + `.transparent(true)`。先使用实机基线判断当前系统模糊是否真实工作，再调 tint。
- 继续让 `DWMWA_USE_IMMERSIVE_DARK_MODE` 与有效 CSS `.dark` 状态一致。不能只看系统主题：应用显式指定浅色/深色时，DWM 明暗也必须跟随应用的有效主题。
- `EffectsBuilder.color(Color(255,255,255,18))` 不应被误认作 Windows 11 的可读性控制手段；正文底色/文字对比应由 CSS 的阅读层处理。
- Acrylic 在部分 Windows 10/11 版本调整/拖动时存在性能成本。视觉改动不得让每一帧都重建原生 effect 或重复调用 DWM。

---

## 4. 文件级实施地图

| 文件 | 改动目标 | 修改边界 |
|---|---|---|
| `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` | Tauri 更新到支持 Liquid Glass Regular 的稳定 2.12.x 版本，确认 tauri-build / runtime / CLI 兼容 | 单独提交；不要任意更新与浮窗无关的依赖 |
| `package.json`、`package-lock.json` | 若需要，与 Rust Tauri 版本对齐 Tauri CLI；固定实际安装版本 | 只改必要条目并运行 build/CI |
| `src-tauri/src/system/translation_flow.rs` | macOS Liquid Glass Regular + Popover fallback；应用原生材质后再 show；材质失败降级日志 | 保持 `build_popup_window()` 为唯一窗口构建入口；不改定位、latest-wins 或 focusable 语义 |
| `src-tauri/src/commands/system.rs` | 将 `set_popup_theme` 的 macOS 空实现补成原生 appearance 同步；Windows DWM 分支保留 | 必须与前端 `dark` 参数 IPC 契约一致；AppKit 窗口调用在主线程执行 |
| `src/lib/commands.ts` | 仅在 IPC 参数/回退需要扩展时更新类型封装 | 不允许出现 TS 调用与 Rust command 命名或 serde 参数不一致 |
| `src/hooks/useTheme.ts` | 继续将设置偏好解析成有效 `.dark` 状态；保持系统模式兼容 | 不创建第二个彼此竞争的 Theme Store；系统模式已有轮询时先不要删除 |
| `src/windows/popup/PopupWindow.tsx` | 收敛原生材质主题同步；确保初始状态/主题变化都触发一次有效同步；保留尺寸/关闭/键盘流程 | 可抽出 `usePopupAppearance`，但必须证明减少复杂度，不要顺手重构全部 useEffect |
| `src/styles/globals.css` | 引入 `--popup-glass-*` 与 `--popup-reading-*` 语义 token；阅读层、焦点层、减少透明/减少动态模式 | 保留双层伪元素的堆叠逻辑；禁止让伪元素覆盖文本或拦截输入 |
| `src/windows/popup/ResultView.tsx` | 给正文增加独立阅读表面语义类；校验次级文字、截断标记与底部 meta 的对比度 | 保留复制、文本选择、最大高度、滚动和语言方向行为 |
| `src/windows/popup/LoadingView.tsx`、`ErrorView.tsx` | 骨架、错误信息、错误码、操作提示全部覆盖两主题和阅读衬底 | 错误文字不能仅靠红色区分，不能用低对比 text-tertiary 展示关键指示 |
| `src/windows/popup/TrafficLights.tsx` | 保留红黄绿语义；确保 hover、focus-visible 与鼠标离开后的提示合理 | 不能改变按钮点击、拖动禁区 `data-no-drag` 或关闭行为 |
| `src/windows/settings/SettingsWindow.tsx`（可选 P2） | 增加“减少透明效果”偏好，或提供确定性 accessibility fallback | 需要同步 config type、Rust 默认值/持久化与所有语言的设置文案；除非实测系统媒体特性不可靠，否则不必抢在核心修复前做 |
| 新增 `src/windows/popup/__tests__/...` 和/或 `src/lib/__tests__/popupAppearance.test.ts` | 主题解析、IPC 调用去重、状态/样式类回归 | 使用项目现有 Vitest；不引入 E2E 框架作为首个修复的前置条件 |
| 新增 `docs/implementation/popup-glass/` | 基线、集成图、实机截图矩阵、验收记录 | 文档要记录实际 OS/WebView/ Tauri 版本和失败项 |

---

## 5. 主题与原生材质同步设计

### 5.1 唯一事实来源

主题流应当是：

```text
SettingsWindow (theme preference: system | light | dark)
  └─ setConfigBatch + applyTheme()
       ├─ configStore 保存用户偏好
       ├─ theme-changed 事件广播到其他 WebView
       └─ useTheme 计算当前有效明暗，更新 document.documentElement.classList
            └─ Popup appearance sync 观察有效 .dark
                 ├─ CSS vars 实时切换
                 └─ invoke(set_popup_theme, { dark: effectiveDark })
                      ├─ Windows: DWM immersive dark mode
                      └─ macOS: 将 popup 的 NSWindow appearance 同步为 Aqua / Dark Aqua
```

`effectiveDark` 指已经解析完成的实际明暗状态，不是原始配置字符串。例如主题偏好 `system`、系统当前深色时，传 `true`；系统切浅色并导致 `.dark` 消失后传 `false`。

### 5.2 具体要求

1. `applyTheme()` 继续是 CSS 根主题切换的单一入口；禁止另一个 Hook 自行读取配置并独立决定 CSS 明暗。
2. 浮窗启动时先进行一次同步；之后响应根节点 `.dark` 的实际变化。不因任意其他 `<html class>` 变化而重复触发昂贵原生调用，保留“上次 effectiveDark”去重。
3. Windows 继续走现有 `DWMWA_USE_IMMERSIVE_DARK_MODE` 路径，原生调用失败应 debug/diagnostic 记录但不导致翻译失败。
4. macOS 不可再保持空实现。优先选择经过确认可工作的 Tauri / AppKit API，将 popup window appearance 设置成与有效 CSS 明暗相同的 `Aqua` / `Dark Aqua`，并确认“应用显式主题与系统主题相反”时原生材质仍一致。AppKit 修改应在主线程执行。
5. 对“跟随系统”模式，系统主题改变后 `useTheme` 负责更新有效 CSS `.dark`；Popup 的 observer 随后把同一个有效值同步给原生材质。系统轮询兜底先保留，待测试稳定后再决定是否删除 Popup 内额外的 `setInterval(sync, 2000)`。
6. 任何跨窗口 `theme-changed` 事件都要同时确保设置界面、历史窗口、Popup 的 CSS 状态一致。不要只修 popup 本身却破坏全局主题。
7. 同步失败要有有界重试或后续主题变更时重试；不允许 infinite loop。日志不得打印译文、剪贴板内容或凭证。

### 5.3 建议的实现拆分

可在 `src/windows/popup/` 或 `src/hooks/` 新增一个极小的 `usePopupAppearance`，承担：

- 读取 `<html>` 的有效 `.dark`。
- mount 时立即同步一次，之后观察 class 变化。
- 仅在 `dark` 值变化时调用 `setPopupTheme`。
- 用取消标志/清理逻辑避免卸载后 Promise 回调更新状态。
- 仅当已经证明 `useTheme` 的系统主题轮询会反映到 `.dark` 后，才考虑移除 Popup 额外轮询。

如果继续保留在 `PopupWindow.tsx`，也可以；验收标准是只有一份明确的“有效明暗 → native appearance”逻辑，而不是必须新增文件。

---

## 6. CSS 视觉实现规格

### 6.1 新增语义 token（名称可调整，但要统一）

建议在 `:root` 与 `.dark` 中定义以下类别，不要散落魔数：

- `--popup-glass-tint`：外层的微弱着色；优先让原生系统材质成为主角。
- `--popup-glass-edge`：与主题匹配的边缘高光/内描边。
- `--popup-glass-highlight`：左上/顶部光学高光。
- `--popup-glass-shadow`：外部深度阴影。
- `--popup-reading-bg`：正文区域阅读衬底；Light 与 Dark 使用不同配方。
- `--popup-reading-border`：阅读区域与外层材质之间的分界。
- `--popup-text-primary` / `--popup-text-secondary` / `--popup-text-tertiary`：仅在需要与通用设置页 token 分开校准时使用；否则复用当前语义文字色。
- `--popup-divider`、`--popup-control-hover`、`--popup-control-active`、`--popup-focus-ring`。

所有 token 要有浅色与深色配方，并在降级模式下有明确替代值。边缘高光不能作为可读性衬底，辅助文本不能依靠阴影来“补出”对比度。

### 6.2 外层 `.popup-container`

- 保留 `position: relative`、圆角、`overflow: hidden` 和轻薄边缘。
- 不把 `backdrop-filter` 当作真实桌面模糊来源；保留作为无 native effect 时的可选兜底即可。
- `::before`、`::after` 必须 `pointer-events: none`，其 z-index 低于正文和控件。
- 不要为 `.popup-container` 或其祖先设置全局低 `opacity`。
- 保留当前玻璃材质历史经验：外层 tint 从低不透明度开始，通过真实截图调校；不要未经比较直接回到 `0.55` 或任意大数值。
- 如果 `apply_popup_material()` 返回错误，应移除/标记 native effect 不可用，并令 CSS fallback 有可辨别、较稳定的背景色；fallback 不要求看起来与 macOS 26 原生玻璃完全一样，但必须可读。

### 6.3 正文阅读表面

给 `ResultView` 主要译文区域添加独立类名，例如 `.popup-reading-surface`，将其视觉基底与全窗的玻璃光学层分开。

要求：

- 阅读衬底只覆盖正文阅读区域或真正需要稳定对比度的区域；不要给整个容器套一个大面积高不透明白板。
- Light mode 采用浅色、低纹理衬底，正文使用深色高对比文字；Dark mode 采用深色衬底，正文使用浅色高对比文字。
- 允许保留一部分原生材质透出，但透明度最终必须由真实合成截图检验，而不是只看 CSS token 的 alpha 值。
- 保持 `ResultView` 当前正文字号、字重和行高作为回归基线（约 `14.5px` / `font-medium` / `line-height: 1.8`）。只有在真实设备上发现稳定的阅读问题时才改排版。
- 保留 `maxHeight: 360px` 与 `overflow-y: auto`，并验证阅读区域圆角、滚动条和阴影不被父级 `overflow-hidden` 错误裁切。
- 译文可选择、复制；不能让阅读衬底或伪元素干扰文本选择、按钮点击或窗口拖动。
- 截断标记、语言方向、翻译服务名称、键盘提示等内容也必须检查；不要只测正文第一行。

### 6.4 标题栏与折叠态

当前折叠态是一条高度 `44px` 的信息条，内容包括交通灯、状态点、译文摘要和语言方向；保留其单行布局和 `text-overflow: ellipsis`。

- 折叠态下：状态点不能挤压摘要到看不见；摘要 `flex: 1; min-width: 0`；语言方向 `flex-shrink: 0`；短宽度时优先保证交通灯和摘要有合理空间。
- 摘要色使用主文字 token，语言方向色不得低到在亮/暗玻璃上消失。
- 不能仅用颜色表示状态。为状态点提供可访问名称/可读状态文本（可以是 visually-hidden 文本或 `aria-label`），且状态更新不应造成每秒播报。
- 展开和折叠状态共享同一份主题 token；不允许某个 class 覆盖后导致折叠态使用固定浅色文字或固定边框。

### 6.5 交通灯与其他控件

- 红黄绿按钮的语义颜色维持平台熟悉度；它们的色点必须在默认、hover、active、disabled、focus-visible 状态都可识别。
- 给 `.traffic-lights:focus-within .traffic-light:focus-visible svg` 补充可见规则，或采用等价实现，确保键盘焦点时 glyph 可见，即使容器没有 hover。
- 不能移除 `data-no-drag`、`isDragBlockingTarget()` 对控件的保护。
- 交互状态至少包含：default、hover、pressed、disabled、focus-visible。若系统减少动态效果启用，按钮仍要有非动画的按压/焦点反馈。
- 边框、focus ring、按钮轮廓与玻璃表面之间应当有可见的非文本对比，不只靠阴影。

### 6.6 颜色和可访问性偏好

新增或明确支持以下退化规则：

```css
@media (prefers-reduced-transparency: reduce) {
  /* 将阅读表面改成高不透明度/实色，降低/移除光学叠层；不要只改全局 opacity。 */
}

@media (prefers-reduced-motion: reduce) {
  /* 关闭 statusBreathe / 不必要的内容浮动、缩放、过渡；保持即时状态提示。 */
}
```

这是行为规格示例，不是最终可直接粘贴的完整 CSS。由于 `prefers-reduced-transparency` 在一些 WebView 中并不保证可用：

1. Agent 应先在目标 macOS WebKit 与 Windows WebView2 中用 `matchMedia` 实测该偏好是否可见，并记录结果。
2. 媒体特性可用时，在 CSS 中直接提高阅读面板不透明度；原生 effect 若仍强烈透出桌面，可同步切换为无 effect/更厚的材质。
3. 若媒体特性无法覆盖所有受支持环境，应添加一个应用内的可持久化“减少透明效果”偏好，或采用可验证的原生 OS 偏好读取方案。不得声称只写 `@media` 就已跨平台完整支持。
4. 不应以动画代替状态；加载状态的呼吸点在 reduced-motion 下改为静态点并保留文字/ARIA 状态。

---

## 7. 展开、折叠、宽度与翻译状态矩阵

所有视觉实现必须使用真实 `PopupWindow` 的状态组合测试，而不是只测试一张静态成功译文截图。

| 内容状态 | 展开/折叠 | 宽度 | 必须验证的视觉点 | 必须验证的行为 |
|---|---|---|---|---|
| idle / 等待翻译 | 折叠、展开 | 标准、阅读宽（允许状态复位逻辑照常） | 空状态文字/折叠摘要可读，玻璃没有空白闪烁 | 新一轮翻译能复位折叠态 |
| loading | 展开、折叠 | 标准、阅读宽 | 骨架线与分隔线可见但不过度抢眼；加载点动效与 reduced-motion 一致 | 翻译结果到来后不会被折叠态遮住 |
| success，短译文 | 展开、折叠 | 标准、阅读宽 | 正文、语言方向、复制按钮和元信息完整；短摘要不被控件挤压 | 复制译文/原文正常；Esc/空格规则不变 |
| success，长译文/多段文本 | 展开、折叠 | 标准、阅读宽 | 正文可读，滚动边缘清晰，滚动条/圆角不被裁；折叠摘要正确省略 | 点击文本可选中；滚动不拖动整个窗口 |
| error / 认证或额度错误 | 展开、折叠 | 标准、阅读宽 | 错误原因、设置入口、再次复制提示可读，错误色不成为唯一信息载体 | 设置按钮与关闭快捷键不冲突 |
| truncated result | 展开、折叠 | 标准、阅读宽 | 截断提示与正文有足够对比，不能误读为无关装饰 | 内容与服务状态不变 |
| 翻译状态切换中 | 任意合法转换 | 标准 ↔ 阅读宽 | 不能闪白、变成错误明暗、显示旧宽度下的裁切；高光层不压过文字 | 保留当前尺寸契约的两段式量高与取消逻辑 |

**注意**：宽度切换/展开动画应延续当前 `PopupWindow.tsx` 的量高策略；不要通过为 `.popup-container` 加 CSS width/height transition 来假装原生窗口平滑变形，因为那会与 OS 窗口重设尺寸和两段式高度测量打架。

---

## 8. 细粒度开发里程碑

### M0 — 基线、集成图与视觉复现（不改产品代码）

**任务**

- 完成第 0 节所有源码核查和测试。
- 记录 Windows / macOS 实测环境（OS build、Tauri 版本、WebView/WKWebView 版本、显示比例、应用主题、系统主题）。
- 在玻璃层放置高对比“黑底白字/亮色块”测试窗口，证实当前是否能看到真实 native blur、是否有材质闪现，以及文字在不同背景下是否消失。
- 输出 `BASELINE.md`、`INTEGRATION_MAP.md`，附现有截图和每项问题的复现步骤。

**验收**：没有改动业务代码；所有 baseline 命令有真实运行结果；当前问题与待验证假设被区分开。

### M1 — 升级 Tauri 并接入 macOS Liquid Glass 正规路径

**任务**

1. 单独升级 Tauri Rust 依赖至一个支持 `Effect::LiquidGlassRegular` 的稳定 `2.12.x` 版本，更新 `Cargo.lock`；必要时更新 npm 侧 Tauri CLI 到兼容版本。先审阅 release notes 与锁文件 diff。
2. 在 `translation_flow.rs` 的 `apply_popup_material()` 中：
   - macOS 26+：Regular Liquid Glass。
   - macOS 13–25：Popover fallback。
   - Windows：保留 Acrylic，除非性能基线要求另行调整。
   - 其他平台/失败：保留 CSS 可读性降级。
3. 核查 `tauri.conf.json` 的 `macOSPrivateApi: true` 在升级后是否仍需要；没有证据时不要随意移除。当前浮窗需要透明背景这一条件仍不可去掉。
4. 建窗时统一先隐藏窗口，应用原生材质和必要 appearance，再 show。覆盖 `ensure_popup_window()` 与 `show_popup_loading()` 两条路径及后续创建路径。
5. native effect 失败记录可诊断日志，日志不包含译文或敏感内容。

**测试/验收**

- macOS 26+ 真机/虚拟机确认底下桌面内容真实透出且被原生材质模糊。
- macOS 13–25 能正常启动，Popover fallback 可用。
- Windows 10/11 的原有 Acrylic 路径仍可见、无明显启动闪白；在实际目标系统检查调宽/移动性能。
- `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`、`npm run build`、`npm test` 通过。

**回滚策略**：单独 revert Tauri 依赖升级/材质选择提交；不得把本阶段与 CSS 视觉改版混成不可分离的大提交。

### M2 — CSS 分层：外层真玻璃 + 局部阅读衬底

**任务**

1. 为 `globals.css` 中 popup glass/readability token 建立一套明确命名和双主题定义。
2. 外层只保留薄 tint、边缘、光学高光、阴影；正文不再完全依赖外层高光渐变承担可读性。
3. 在 `ResultView.tsx` 的正文区域添加阅读表面类；同步检查 `LoadingView`、`ErrorView` 与折叠摘要。
4. 检查 `--text-primary/secondary/tertiary` 在真实合成后的表现。可保留当前 `14.5px / font-medium / line-height 1.8` 正文设定，不要为了让视觉更“轻”而把正文改成低字重、低对比灰色。
5. 修复 layer order：伪元素永远不覆盖文本/交互；阅读底层不能形成意外的双重 border 或滚动裁切。
6. 用真实截图调节透明度。至少比较两档阅读衬底，不要只凭开发者个人的单一桌面壁纸判断。

**测试/验收**

- 浅色与深色中，译文都能在明亮/黑暗/饱和/杂乱背景上稳定阅读。
- 外层仍然明显具有透明、模糊、边缘高光和阴影；不是纯色板，也不因 tint 太弱而只剩三个按钮浮在文字上。
- 折叠态摘要和语言方向的对比度足够。
- 文本选择、滚动、复制按钮和拖动行为不变。

### M3 — 修复 native appearance 与 CSS 主题不一致

**任务**

1. 追踪设置页主题切换、`applyTheme()`、`theme-changed` 广播、Popup 的 class observer 和 Rust `set_popup_theme` 实际运行顺序。
2. 修复 macOS 非 Windows 空实现：同步 popup native `NSWindow` appearance 到有效明暗。优先使用 Tauri 现有 API；若必须用 AppKit，限制到小型 helper、主线程执行和必要 `objc2-app-kit` feature。
3. 保留 Windows DWM 分支，测试显式深浅主题与 OS 相反时也能同步。
4. 明确单一数据流：先由 `useTheme` 应用有效 `.dark`，Popup 只根据这份状态同步原生窗口。禁止直接把 `system/light/dark` 配置字符串当成 effective dark boolean。
5. 去重 native 调用；在测试证明可靠后，再评估是否移除 `PopupWindow.tsx` 的 2 秒轮询，不能提前删除 `useTheme.ts` 的系统主题回退轮询。
6. 初始显示、窗口隐藏后再次显示、设置窗口外部切换主题、系统主题实时切换都必须被覆盖。

**验收矩阵**

| App 偏好 | OS 主题 | 预期 |
|---|---|---|
| `light` | light | CSS + native material 均为浅色 |
| `light` | dark | CSS + native material 仍为浅色 |
| `dark` | light | CSS + native material 仍为深色 |
| `dark` | dark | CSS + native material 均为深色 |
| `system` | light → dark | `.dark`、native appearance 与材质跟随切换 |
| `system` | dark → light | `.dark`、native appearance 与材质跟随切换 |

切换过程中不得出现持续错色、黑白闪屏、无限事件循环或重复 native IPC 风暴。

### M4 — 展开/折叠、状态、控件可见性与无障碍降级

**任务**

1. 按第 7 节矩阵手工检查 idle / loading / success / error / truncated 等状态。
2. 调整交通灯焦点 glyph，使 `focus-visible` 时图标可见；确保 hover 不再是识别功能的唯一条件。
3. 为状态点增加有意义且不会高频播报的可访问状态名称；颜色只是状态冗余提示，不是唯一通道。
4. 实现 `prefers-reduced-motion: reduce`，关闭呼吸和非必要动效，保留即时状态变化。
5. 实测 `prefers-reduced-transparency` 支持情况。若无法跨目标 WebView 保证，增加应用设置“减少透明效果”并存入现有配置体系（需要扩展 `AppConfig`、Rust config key/default、常规设置 UI 和中英文等所有现有语言资源）。
6. 减少透明模式应实际移除或显著弱化透明/高光层，并使阅读表面更稳固；若 native effect 会继续明显透出背景，则考虑 `clear_effects()` 或选择更厚的 material。恢复默认后重新应用 native effect，不允许切换后永久丢失玻璃材质。

**验收**

- 键盘、鼠标、只支持 hover 的设备、可访问性偏好切换下，主要按钮与文字仍然可见。
- reduced motion 时无持续呼吸动画；loading 仍清楚表明状态。
- 减少透明效果关闭/开启的状态都可逆，无需重启应用。

### M5 — 专项测试、视觉回归与发布准备

**自动化测试要求**

- 新增 Popup appearance 同步测试：初次运行有一次同步；effective `.dark` 不变时不会重复 IPC；明暗改变会调用一次；组件卸载后 observer / interval 清理；native 命令失败不使 UI 崩溃。
- 覆盖主题解析：`light` 永远 light、`dark` 永远 dark、`system` 跟随 `matchMedia` 并处理变化。
- 覆盖视觉状态下类名及状态语义：折叠/展开、宽度切换、loading/success/error、reduced-motion/reduced-transparency。项目当前使用 Vitest；先复用它，不要立即安装重量级 E2E 工具。
- 如果测试 jsdom 中没有 `window.matchMedia` / MutationObserver 行为，使用本地 test mock；不可把测试专属逻辑加进生产路径。
- 为 Rust native appearance helper 添加可测的纯决策函数（例如 OS/version → effect choice），把需要平台 API 的部分封装在条件编译边界内。平台专属编译不能仅由 Linux 构建通过来代替 macOS 编译检查。

**视觉回归需要在实机执行**

- CSS 对比度和系统材质无法单凭 jsdom 或常规单测验收。必须在真正 Windows/macOS 系统截图检查。
- 至少验证 macOS 26+ Liquid Glass Regular、旧版 macOS fallback、Windows 10、Windows 11。
- 每个平台至少验证应用主题 light/dark/system 与系统主题不同的组合，以及展开/折叠、标准/阅读宽、短/长译文和 error/loading。
- 截图应记录系统版本、应用版本、主题组合和窗口状态，放在 `docs/implementation/popup-glass/evidence/`（如需提交则注意图片大小；至少保存人工验收记录）。

**最终验收命令**

```bash
npm test
npm run build
npx tsc --noEmit
cd src-tauri && cargo fmt --check
cd src-tauri && cargo clippy -- -D warnings
cd src-tauri && cargo test
```

并完成项目 CI 的 Windows / macOS 构建。macOS 原生材质变更不得以 Windows CI 绿色代替 macOS 验收。

---

## 9. 回归保护清单（必须逐项勾选）

- [ ] `focusable(false)` 首次被动显示仍不抢焦点。
- [ ] 用户点击后 `activatePopup()` 正常，浮窗不会约 96ms 后自关闭。
- [ ] 鼠标按在按钮上不启动 OS 拖动；拖动窗口仍记住位置。
- [ ] `.selectable` 译文可正常选中/复制；划选不被误判成拖窗。
- [ ] Esc / 空格 / Enter 的行为与当前设计相同；按钮聚焦时空格不会误关闭或误切换。
- [ ] 新请求到来时折叠态按既有逻辑复位；迟到的旧翻译结果仍被 `request_id` 检查拦住。
- [ ] 宽度变化保持两段式量高；正文不会因先改宽后换行而裁切。
- [ ] 最高高度、长译文滚动、最大宽度和多显示器定位不回归。
- [ ] 原生材质初始化和主题同步失败时仍能显示、读取、复制与关闭翻译结果。
- [ ] 首次窗口创建、已有隐藏窗口重用、向导关闭后创建 Popup 三条路径材质一致。
- [ ] Light/Dark/System 主题变化无需刷新所有窗口，且不出现 native/CSS 错色。
- [ ] Windows DWM 和 macOS 原生材质的错误只影响视觉层，不影响翻译流程和托盘生命周期。
- [ ] reduced-motion / reduced-transparency（若支持）不会误伤窗口尺寸或事件监听清理。

---

## 10. Agent 交付格式

每个里程碑结束时，Agent 必须按下面模板汇报，不要用“已优化”“效果更好”这样的不可验收描述。

```markdown
## Milestone: Mx — <名称>

### 变更摘要
- ...

### 修改文件
- `path/to/file`: 修改目的、重要行为

### 已验证行为
- [x] ...
- [ ] ...（未完成项要说明原因）

### 测试结果
- `npm test`: 通过 / 失败 / 未运行（给出真实摘要）
- `npm run build`: ...
- `cargo test`: ...
- macOS 真机验证：OS 版本 + 主题组合 + 截图记录 / 未运行原因
- Windows 真机验证：OS build + 主题组合 + 截图记录 / 未运行原因

### 未解决风险
- ...

### 回滚方式
- commit SHA 或精确 revert 范围

### 下一个里程碑
- ...（未经用户/项目负责人确认，不要自动进入不相关阶段）
```

### 10.1 推荐提交顺序

1. `chore(tauri): upgrade compatible window-effects API` — 只包含必要的 Tauri/lockfile 更新与可编译性修复。
2. `feat(popup): add macOS Liquid Glass regular material with fallback` — 原生材质与建窗时序。
3. `fix(popup): sync native and CSS appearance` — macOS appearance 与 Windows DWM 同步。
4. `feat(popup): separate glass shell from readable content surfaces` — CSS token、阅读表面和控件可见性。
5. `test(popup): cover appearance states and add visual verification notes` — 专项测试、验证矩阵与文档。

如果基线发现任何里程碑会造成非预期合并冲突，应重新组织提交，但不能把所有功能和依赖升级压到一个难以回滚的 commit。

---

## 11. 官方参考

- [Apple Human Interface Guidelines — Materials](https://developer.apple.com/design/human-interface-guidelines/materials)：Liquid Glass Regular/Clear 的语义、可读性、正文层与控制层的使用原则。
- [Apple Human Interface Guidelines — Dark Mode](https://developer.apple.com/design/human-interface-guidelines/dark-mode)：文字和系统语义色应随明暗外观自适应。
- [Tauri v2 Window API — `setEffects()` / `Effect`](https://v2.tauri.app/reference/javascript/api/namespacewindow/)：窗口必须透明才能应用系统 effect；提供 Acrylic、Popover 和 Liquid Glass effects。
- [Tauri Rust `Effect` enum](https://docs.rs/tauri/latest/tauri/window/enum.Effect.html)：当前支持的各平台窗口效果与版本限制。
- [Tauri 2.12.0 release notes](https://tauri.app/release/tauri/v2.12.0/)：新增 Regular / Clear Liquid Glass window effects。
- [W3C WCAG 2.2 — Contrast Minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)：普通文本至少 4.5:1，大型文本至少 3:1。
- [MDN — `prefers-reduced-transparency`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/%40media/prefers-reduced-transparency)：检测用户减少透明效果偏好；浏览器支持受限，因此应实测目标 WebView。

---

## 12. 最终原则

**不要把 Liquid Glass 做成一张低透明度白/黑色贴图。** 原生材质负责真实的桌面模糊与环境融合；外缘光学层只用于表达厚度与受光；正文阅读表面负责可读性；主题同步负责 CSS 和 native appearance 始终一致；对比度与交互状态通过实机和自动化测试证明。

视觉品质不是透明度越高越好，而是在“看得见桌面”和“读得清译文”之间，对不同层级进行独立控制。对于一个翻译悬浮窗，译文始终高于玻璃特效的优先级。
