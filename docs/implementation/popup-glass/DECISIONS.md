# 浮窗材质改造 · 决策记录

> 记录日期：2026-10-22
> 输入：`BASELINE.md` §6 风险清单、`INTEGRATION_MAP.md` §7 改造计划
> 性质：审计期决策。实施时若与实测冲突，以实测为准，并回写本文件。

---

## 一、决策摘要

| 编号 | 议题 | 决策 | 对应风险 |
|---|---|---|---|
| D1 | 依赖升级节奏 | 拆两步：先纯升级验证，再接材质参数 | R1 |
| D2 | `windows-sys` 版本 | 保留 0.52，允许与传递依赖双版本共存 | R2 |
| D3 | macOS 原生外观同步 | 单窗口 `NSWindow::setAppearance`，跟随系统时清空（`follow_system` 由前端传参） | R3 / R4 |
| D4 | macOS 材质状态 | 维持 `FollowsWindowActiveState`（观感取向为淡） | R5 |
| D5 | macOS 验收口径 | 只认真机截图，无截图一律记"未验证" | R6 |
| D6 | 减少透明的降级 | 媒体查询 + 应用内开关，两条触发源取「或」 | R7 |
| D7 | 对比度不达标时的取舍 | 调文字与局部衬底，不加厚整块面板（仅 normal 模式） | R8 |
| D8 | 最低系统版本 | 不修改 `minimumSystemVersion` | — |
| D9 | 材质失败的可观测性 | 建窗时记录效果决策日志，不依赖 `set_effects` 返回值 | 新发现 |
| D10 | 主题轮询去重 | 暂不合并两条 2 秒轮询 | 规格 P2 |
| D11 | normal 模式次级文字对比度 | A 已实施（浅色与 reduce 全达标，深色 normal 剩 18 条）；H1 已修（R-F） | R8（实测确认） |
| D12 | normal 模式 tertiary 文字与方向箭头 | 未处理，登记为已知缺口；逐 token 调色不可行，需局部衬底或改层级语义 | R8（实测确认） |

---

## 二、逐项说明

### D1 · 依赖升级拆两步

**决策**：第一步只升依赖（`tauri` 2.12.5 及连带 `tauri-build` / `tauri-utils` / `window-vibrancy` / `wry` / `tao`，前端 CLI 与 API 同步），`translation_flow.rs` 的 `effects` 参数保持原样；跑完回归后再做第二步（接入 Liquid Glass 与回退、调整 `EffectState`、统一建窗时序）。

**理由**：`wry` 0.54→0.57、`tao` 0.34→0.37 各跨三个 minor，可能影响透明窗口、拖动、焦点与窗口事件。若与材质参数同批修改，出问题时无法判断是升级带来的还是参数带来的。

**回退**：第一步独立成提交，可单独 revert，不影响后续工作。

### D2 · 保留 `windows-sys` 0.52

**决策**：直接依赖维持 `windows-sys = "0.52"`，不跟随 `window-vibrancy` 升到 0.60/0.61。

**理由**：项目对 `windows-sys` 的直接使用只有 `DwmSetWindowAttribute` 一处（`commands/system.rs`），0.52 的模块路径与 feature 已验证可用；升级需要重新核对 feature 名与 API 形状，收益为零。

**影响**：最终二进制同时包含两个主版本的 `windows-sys`，体积增量可忽略。

**验证**：升级后 `cargo clippy -- -D warnings` 与 `cargo test` 通过即可，无需额外适配。

### D3 · macOS 用单窗口 appearance，不用应用级 `set_theme`

**决策**：在 `set_popup_theme` 的 macOS 分支中，取 `window.ns_window()`，转成 `&NSWindow` 后调用 `setAppearance(...)`，整段放进 `run_on_main_thread`。

`follow_system` 由**前端传参**，不从 `ConfigService` 读取（见下方「实现偏差」）：

- `follow_system = true` → `setAppearance(nil)`，让窗口跟随系统
- `follow_system = false` → 按 `dark` 参数设置 `NSAppearanceNameAqua` / `NSAppearanceNameDarkAqua`

依赖侧给 `objc2-app-kit` 增加 `NSWindow`、`NSAppearance`、`NSResponder` 三个 feature。**三个缺一不可**：`NSWindow` 的 `NSAppearanceCustomization` 实现同时门控在 `NSAppearance` 与 `NSResponder` 上（`NSWindow` 自身以 `NSResponder` 为父类），少一个都不会生成 trait impl。

**理由**：`tauri::set_theme` 在 macOS 上是**应用级**（底层 `[NSApplication setAppearance:]`），用它修浮窗会连带改变设置页与历史窗口的原生外观，超出改动范围。

**必须避开的一个环**：若在 `theme = system` 时也强行设置窗口 appearance，会把浮窗 webview 的 `prefers-color-scheme` 钉死。此后系统主题变化时，浮窗自己的 `matchMedia` 轮询读到的永远是旧值，等于自断检测链路。所以"跟随系统"这一档必须清空 appearance，而不是设成当前明暗。

**实现偏差（2026-10-22 记录）**：本节原写"命令内部读取 `ConfigService` 的 `theme`"，实际实现是前端传 `follow_system` 参数。偏离的理由：

- `dark` 无论如何都必须由前端传（它是渲染结果，只有前端能可靠读到 `.dark` class 与 WebView2 的 `matchMedia`）
- 若 `follow_system` 由后端读、`dark` 由前端传，同一次判断被拆到两个进程两侧，时序上反而更难保证一致
- 后端完全有能力自判（`AppState.config` 是 `Arc<RwLock<ConfigService>>`，`set_config_batch_inner` 先写库再广播，收到事件时配置已是新值），所以这**是设计债不是错误**

**未来若改为后端读**：`set_popup_theme(app, dark)` 单参数，内部 `ConfigService::get("theme")`。收益是消灭前端"配置未加载时把未知误判成非 system"的语义分裂（该分裂已在 `PopupWindow.tsx` 用「config 为 null 就跳过」修掉，但那是前端侧的补丁，不是根治）。

**回退**：清空 appearance 即回到现状（空实现）。

### D4 · macOS 材质状态维持 `FollowsWindowActiveState`

**决策**：`EffectsBuilder` 的 `state` 保持 `FollowsWindowActiveState`，不改为 `Active`。

**理由**：浮窗是 `focusable(false)`，永远不会成为 key window，因此 `NSVisualEffectView` 会稳定渲染 inactive（更淡）外观。既然观感取向是"偏淡"，这恰好是想要的效果，不必改动。

原先考虑改为 `Active` 是担心 inactive 外观"过灰"；但在没有 macOS 实机的情况下，改动只会引入未经证实的风险。**维持现状是更小的改动面**。

**何时回退此决策**：真机截图显示 inactive 外观发灰、发脏（而非"淡"），再改为 `Active`。改动量是一行。

### D5 · macOS 验收只认真机截图

**决策**：macOS 侧的任何视觉结论必须以真机截图为准。没有截图时，汇报一律写"未验证"，不得写"通过"。

**理由**：本机是 Windows；CI 的 macOS runner 只能证明编译与单测，不能证明原生材质渲染。

**影响**：若改造完成时仍无 macOS 设备，Windows 侧可先行验收，macOS 侧维持现状（Popover），不阻塞其他工作。

### D6 · 减少透明：CSS 媒体查询 + 应用内开关（已实施）

**原决策**：先只增加 `@media (prefers-reduced-transparency: reduce)` 规则。若真机确认 WKWebView 不支持该媒体查询，再评估应用内开关。

**触发条件**：仅在"真机确认不支持"或"有用户实际反馈"时启动开关方案。

**触发结果（2026-10-22）**：条件已满足，开关方案已实施。

证据不是真机截图，而是**平台能力本身**——这比截图更确定，因为它是"该 API 是否存在"而不是"某台机器上表现如何"：

- MDN BCD `css/at-rules/media.json`：`prefers-reduced-transparency` 的 `safari.version_added: false`，`impl_url` 指向 [WebKit bug 175497](https://webkit.org/b/175497)
- WebKit 官方 "Features in Safari 26.0" 全文无此项
- Chrome 118+ 才支持（WebView2 满足）

WKWebView 与 Safari 同引擎，因此 macOS 上 `matchMedia("(prefers-reduced-transparency: reduce)")` 恒为 `false`。这不只是"CSS 白写"：`setPopupTransparency` 在 macOS 上**一次都不会被调用**，原生材质永远不会被撤——整条链路静默空操作。

**实施方案**：两条触发源取「或」，叠加而非互斥。

| 途径 | 载体 | 覆盖范围 |
|---|---|---|
| 系统级无障碍偏好 | `@media (prefers-reduced-transparency: reduce)` | Windows / Edge WebView2 |
| 应用内开关 | `config.reduce_transparency` → `:root.reduce-transparency` | 全部平台 |

系统偏好命中时永远生效，开关只是额外多一条途径——不是"用应用设置覆盖系统设置"。

**为什么 CSS 配方写了两遍**：触发条件在 CSS 中无法合并成一个「或」。`@media` 与选择器没有合成语法，`@container style()` 在目标 WebView 上支持面不够。用 JS 在媒体查询命中时补挂 class 能消掉一份，但那会让系统偏好的响应依赖脚本——脚本出错时无障碍设置就失效了。宁可重复，不引入这种耦合。两处都已标注「改配方时必须同步」。

**代价**：这是真实的维护成本，不是最优解。若将来 CSS 条件语法支持到位，应合并回一份。

**验证**：Windows 上以 release 构建 + CDP 读计算值实测四种组合（基线 / 开关开 / 开关关 / 深色+开关），结果与预期一致。macOS 侧按 D5 记未验证。

### D7 · 对比度不足时不动玻璃厚度

**决策**：四类背景（纯白 / 纯黑 / 高饱和 / 纹理）实测不达标时，**不通过整体加厚面板来换对比度**。按以下顺序处理：

1. 调 `--text-primary` / `--text-secondary` 的明度或字重
2. 只在文字区域加局部衬底，不铺满整块面板
3. 最后才考虑提高 `--popup-reading-bg` 的不透明度，且上限 0.65

**理由**：观感取向是"偏淡"，整体加厚会直接背离这个取向。而对比度问题多数情况下是文字色与背景的相对关系问题，调文字比调背景更精准，对玻璃厚度的影响也更小。

**上限**：若第 1、2 步做完仍不达标，说明玻璃透过率本身过高，此时应回头与用户确认是否接受"更淡但对比略低"，而不是单方面加厚。

**适用范围（2026-10-22 补充）**：本节管的是 **normal 模式**——即"系统原生材质在场、CSS 只是着色层"的情形。此时提高 alpha 是在玻璃之上再糊一层，确实会背离"偏淡"。

**reduce 模式不适用本节，且 0.94 不是对本节的违反**。原因在于前提变了：

| | normal | reduce |
|---|---|---|
| 原生材质 | Acrylic / Liquid Glass 在场 | **整个撤掉** |
| `backdrop-filter` | blur(30px) | **none** |
| 衬底的角色 | 着色层（玻璃才是主体） | **唯一的视觉载体** |

无材质、无模糊时，0.65 的衬底等于让桌面直接透出来——那不是"更淡的玻璃"，是"没有底"。R-C 的算术证实了这点：0.72 时 secondary 只有 2.8–4.0:1，低于 4.5:1。reduce 恰恰是用户要求"看得更清楚"的时候，此时留薄反而与用户意图相反。

所以 reduce 的 0.94 / 0.92 是**对 D7 适用范围的收窄，不是推翻**。D7 在 normal 模式下继续完全有效——normal 的 `--popup-glass-tint` 仍是 0.10 量级，没有借这次改动加厚。

**若将来要改 reduce 的配方**：仍应先按第 1 步调文字色。本次已经这么做了——深色 tertiary 从 `#98989D` 提到 `#C7C7CC`、浅色从 `#6E6E73` 收到 `#69696E`，两处都只在 reduce 媒体查询/class 内生效，normal 模式层次关系不变。

### D8 · 最低系统版本不动

**决策**：`tauri.conf.json` 保持不设 `minimumSystemVersion`。

**理由**：Liquid Glass → Popover 的回退链保证 macOS 15 及以下仍可用；提高最低版本会切断存量客户端的升级路径，代价远大于收益。

### D9 · 材质效果改为显式记录

**决策**：建窗时读取 OS 版本，写一条 `popup_material_decision` 日志，字段包含平台、系统版本、最终选择的效果（Liquid Glass / Popover / Acrylic）。不再依赖 `set_effects` 的返回值判断材质是否生效。

**理由**：`set_effects` 在两个版本上都会吞掉内层错误，`vibrancy::set_window_effects` 恒返回成功，现有 `popup_material_unavailable` 警告几乎不可能触发，属于失效的可观测点。

**验收**：日志能区分"选择了 Liquid Glass"与"回退到 Popover"两种情况。

### D10 · 主题轮询暂不合并

**决策**：保留 `useTheme` 与浮窗各一条 2 秒轮询，暂不删除浮窗那份。

**理由**：两条轮询的检查成本可忽略，真正的 IPC 已被 `lastDark` 去重。等外观同步链路在真机验证稳定后，再评估是否移除浮窗那条；`useTheme` 的系统兜底无论如何都不动。

---

## 三、已确认的事项

以下三项原为待确认项，现已定案。

### 3.1 macOS 设备：暂无

**结论**：改造期间没有 macOS 实机可用。

**影响**：
- macOS 侧的全部改动按 D5 处理——只交付"编译通过 + 代码正确"，一律标注未验证。
- Windows 侧照常验收，不因 macOS 缺席而阻塞。
- M1 的 `EffectState` 取舍（D4）与 M3 的 appearance 同步，均无法用截图佐证，只能靠 API 契约与源码推理。若后续拿到设备，需要补一轮回归。

### 3.2 玻璃观感：偏淡

**结论**：最终观感取"淡"这一端。

**对既有决策的修正**：

- **D4 需要重新评估**。`EffectState::Active` 会让 `NSVisualEffectView` 渲染更实、更浓的材质，方向上与"偏淡"相反。调整为：先按 `Active` 实现（状态语义正确），但**验收时以"是否偏浓"为否决项**；若真机上显得过实，改回 `FollowsWindowActiveState` 并接受 inactive 外观——那反而更接近"淡"。在没有 macOS 设备的前提下，此项维持待定，代码里保留一行可切换的注释。
- **D7 的取舍顺序需要收窄**。原先写的是"对比度不够就加厚阅读衬底"，与"偏淡"冲突。修正为：**不通过整体加厚来换对比度**。可用的手段按优先级排列：
  1. 调 `--text-*` 的明度/字重（不影响玻璃厚度）
  2. 只在文字所在区域加局部衬底，而不是整块面板
  3. 最后才考虑提高 `--popup-reading-bg`

**可调的具体旋钮**（按对"淡"的影响从小到大）：

| 旋钮 | 现状 | 调整方向 |
|---|---|---|
| `--text-primary` / `--text-secondary` 明度 | `#1D1D1F` / `#515154`（浅色） | 对比不足时先加深文字 |
| `.popup-container::before` 高光强度 | 0.38 / 0.20 / 0.06 | 偏浓时先削这层 |
| `backdrop-filter` 的 `saturate(185%)` | 185% | 降到 140–160% 会更"薄" |
| `--popup-bg` 不透明度 | 0.10（浅）/ 0.16（深） | 已经较薄，尽量不动 |
| `--popup-reading-bg` | 尚未存在 | 最后手段 |

### 3.3 审计文档：留在本地仓库，不纳入版本库

**结论**：`docs/implementation/popup-glass/` 下的四份文档保留在工作区，保持未跟踪状态。

**约束**：后续里程碑**不再向 `docs/` 目录提交任何内容**。实施过程中的记录写在本目录下即可，不进提交历史。

---

## 四、M3 收尾：三项风险的处置结果

> 记录日期：2026-10-22。针对 M3 交付时列出的三条尚存风险。

### R-A · macOS 编译未验证 → 已用跨目标类型检查消解

**问题**：本机只有 `x86_64-pc-windows-msvc`，`#[cfg(target_os = "macos")]`
的代码在本地从未被编译过。M3 期间靠人工审查才发现一个 `let _ = mtm;`
引用已不存在绑定的错误，说明盲区真实存在。

**处置**：`rustup target add aarch64-apple-darwin`（rust-std 由 rustup 提供，
不需要 macOS 的 C 工具链），然后在项目目录内（`rust-toolchain.toml` 锁
1.99.0）跑 `cargo check --target aarch64-apple-darwin`。

**结果**：整个 tauri 依赖链在 `objc2-exception-helper` 处失败——它要编译
`try_catch.m`，需要 macOS 的 `cc`。**全量跨目标检查在 Windows 上不可行。**

**替代方案（已执行）**：把 `apply_popup_appearance` 的 objc2 调用抽成一个
最小 crate（只依赖 `objc2` + `objc2-app-kit`，关掉 `exception` feature 以
避开 C 编译），**版本锁到与项目 Cargo.lock 完全一致的 `objc2 0.6.4` +
`objc2-app-kit 0.3.2`**，在 `aarch64-apple-darwin` 下 `cargo check`。

**结果**：通过。并做了两次**反向验证**确认这套检查不是空转：
- 注入原始 bug（`let _ = mtm;`）→ `E0425: cannot find value mtm` ✅
- 去掉 `NSAppearanceCustomization` 导入 → `E0599: no method setAppearance` ✅

**边界**：这只证明「我写的那段 objc2 调用在 macOS 目标下类型正确」，
**不证明** tauri 整合层、链接、以及运行时行为正确。后者仍需 CI 的
macOS runner（`rust-clippy` job，`aarch64-apple-darwin`）与真机。

### R-B · `set_effects(None)` 对 Liquid Glass 的清除行为 → 源码确认

**问题**：reduce 时用 `set_effects(None)` 撤原生材质，但不确定它对
macOS 26+ 的 `NSGlassEffectView` 是否有效。

**处置**：读源码确认调用链。

- `tauri::vibrancy::set_window_effects(window, None)` → `macos::clear_effects(window)`
- `macos::clear_effects` 同时调用 `window_vibrancy::clear_vibrancy()` 与
  `clear_liquid_glass()`（`tauri-2.12.5/src/vibrancy/macos.rs`）
- `window_vibrancy::clear_liquid_glass`（`window-vibrancy-0.8.1/src/macos/liquid_glass.rs:165`）
  按 `NS_VIEW_TAG_GLASS_VIEW` 找到 glass view，调 `removeFromSuperview()`
  并清理关联的 background view

**结论**：源码层面确认会清除 Liquid Glass，不是空操作。**真机渲染效果仍未验证**
（按 D5，无 macOS 设备一律记未验证）。

### R-C · reduce 下透明窗口的衬底 alpha 是否足够 → 实测发现并修复了真实缺陷

**问题**：reduce 模式撤掉原生材质后，`transparent(true)` 窗口的视觉全部由
CSS 衬底承担。若 alpha 不足，桌面会直接透出，影响可读性。

**处置**：按 WCAG 相对亮度公式，对「衬底色 alpha 合成到六类桌面背景」
计算文字对比度。桌面取纯白、纯黑、中灰、高饱和蓝、高饱和黄、纹理浅灰。

**发现的缺陷**：`.popup-container` 的 alpha 是 0.72（浅）/ 0.74（深），
而**标题栏与操作栏不在阅读衬底覆盖范围内**——语言方向标签、复制按钮、
快捷键提示的可读性只由 container 保证。实测：

| 桌面 | secondary（深色） | tertiary（深色） |
|---|---|---|
| 纯白 | 3.98:1 ❌ | 2.51:1 ❌ |
| 高饱和黄 | 4.63:1 | 2.92:1 ❌ |
| 中灰 | 6.66:1 | 4.20:1 ❌ |

浅色侧同样不达标（纯黑桌面 secondary 3.97:1、tertiary 2.55:1）。
**reduce 模式本应是"看得更清楚"，实际反而更糊。**

**修复**（提交 `ba22073`，遵循 D7 的顺序——先调文字色，不整体加厚）：
1. container 提到与阅读衬底同级：浅 0.94 / 深 0.88
2. 深色 tertiary `#98989D` → `#C7C7CC`；浅色 `#6E6E73` → `#69696E`
3. 两处文字覆盖都只在 `@media (prefers-reduced-transparency: reduce)` 内，
   normal 模式完全不受影响（已用 CDP 读计算值确认）

**修复后最差值**：深色 6.53:1、浅色 4.78:1，六类背景 × 三个文字层级全部 ≥4.5:1。

**边界**：这是**计算值**，不是壁纸采样值——本环境的终端始终遮挡桌面，
无法做真实壁纸取色。normal 模式（container 0.16 + `backdrop-filter`）的
对比度无法用纯色合成模拟，必须真机验收。

---

### R-D · 真机采样验收（2026-10-22，Windows 11 22631）

上面 R-C 的边界已补测。方法：全屏 `BitBlt`（带 `CAPTUREBLT`）才能拿到
DWM 合成后的像素——窗口区域 `BitBlt` 与 `PrintWindow` 都只返回窗口自身
渲染结果，六类背景下数值完全相同，会得出"材质没生效"的错误结论。

采样口径：每个文字叶子节点取其矩形内像素的中位数作为实际底色（字形只占
少数像素），再用 CSS 计算出的文字色算 WCAG 比值。**已交叉验证**：改用同一
行空白区采样，42 条失败项无一偏差 > 1.0，确认不是字形污染。

矩阵：6 背景（纯白/纯黑/中灰/高饱和蓝/高饱和黄/纹理浅灰）× 浅深 ×
normal/reduce = 24 组，共 140 个文字样本。

**结论一 · 材质与透明链路成立**

| 验证项 | 方法 | 结果 |
|---|---|---|
| Acrylic 真实参与合成 | 实测 vs 「CSS alpha × 桌面」纯合成预测 | normal 下偏差最高 **156/255** |
| reduce 确实撤掉材质 | 同上 | 偏差 **≤1/255**，纯 CSS 合成 |
| 窗口 `transparent(true)` | 圆角外角点 vs 桌面 | 完全一致（纹理背景差 3，为边缘抗锯齿） |

**结论二 · reduce 模式全项达标**

reduce 下最低对比度：浅色 **6.94:1**、深色 **7.69:1**，35×2 个样本全部
≥4.5:1。R-C 的文字色收窄修复在真机上有效。

> 口径说明：本轮是 R-D 的首轮采集，**140 个样本**（未含折叠态、未含
> `--text-tertiary`）。R-E 复测扩到 **168 个样本**后深色 reduce 的最差值为
> **7.48:1** —— 两者不是矛盾，是样本集不同：新增样本里有更接近阈值的
> 条目。以样本更多的 168 口径为准。

**结论三 · normal 模式存在规格违规（42/140 失败）**

失败全部集中在 normal 模式，且**只涉及次级文字**，正文与主标题从未失守：

| 文字 | 角色 | 失败数/总数 |
|---|---|---|
| 英语 / 中文 | 语言方向标签 `--text-secondary` | 7/24 |
| 译文 / 原文 | 操作栏按钮 `--text-secondary` | 7/24 |
| 点击后空格关闭 | 快捷键提示 `--text-secondary` | 7/24 |
| Google | provider 名 `--text-secondary` | 7/24 |
| 正文 / 主标题 | `--popup-text-primary` | **0/24** |

最差值：纹理桌面 + 深色 **2.09:1**、高饱和黄 + 深色 **2.15:1**、
纯黑桌面 + 浅色 **2.57:1**。最坏情况分析（底色 ±3 量化误差取不利方向）
下，灰底/蓝底的临界失败仍然失败，结论稳健。

**根因**：`.popup-reading-surface` 只覆盖正文区，标题栏与操作栏**不在其内**，
它们的可读性完全由 `--popup-glass-tint`（浅 0.10 / 深 0.16）保证。而
`--text-secondary` 是按"压在实色面板上"选的中间灰，在 Acrylic 的亮/暗
调制下失效。这与 R-C 在 reduce 模式发现的缺陷是**同一个结构性原因**：
容器 alpha 太薄，撑不住次级文字。

**未修复**：按 D7，normal 模式不加厚玻璃。可选路径见 D11。

### D11 · normal 模式次级文字的对比度处置（A 已实施；H1 已修）

**背景**：R-D 实测 42 条失败，全部是 normal 模式下压在玻璃上的次级文字，
最差 2.09:1。这是当前唯一未达标的规格项。

**当前状态**：方案 A 已落地（见 R-E）。浅色侧与两侧 reduce 全达标；
深色 normal 仍剩 18 条。评审另发现两项遗留，H1（CopyButton hover 提示）
已修（见 R-F）；H2（tertiary 与方向箭头）升为独立条目 **D12**。

**为什么不能直接套用 reduce 的修法**：reduce 把容器提到 0.94 是因为那里
原生材质已经撤掉、衬底是唯一载体。normal 下材质在场，同样加厚会把 Acrylic
闷成色板——正是 D7 要避免的结局。

**候选路径**（按 D7 的顺序，先文字后衬底）：

| 方案 | 做法 | 代价 | 风险 |
|---|---|---|---|
| A | 把 normal 的 `--text-secondary` 在浮窗作用域内单独加深/提亮（如浅色 `#515154`→`#3A3A3D`，深色 `#C0C0C5`→`#E3E3E8`），并按明暗各自配一档 | 无视觉厚度变化，最贴合 D7 | 次级层级感被压缩，需重新确认与 primary 的层次 |
| B | 给标题栏与操作栏各加一条极窄的局部衬底（同 `.popup-reading-surface` 的做法） | 视觉上多两条色带 | 与"只给正文衬底"的既有取向冲突，见 INTEGRATION_MAP |
| C | 接受现状，在文档与设置页明确 normal 模式为"偏淡优先、对比度次于 reduce" | 零改动 | 与规格 4.5:1 的硬门槛直接冲突 |
| D | normal 也撤原生材质（即默认就走 reduce 外观） | 彻底解决 | 等于放弃液态玻璃，与整个计划目标相悖 |

**建议**：A。它是 D7 第 1 步的直接应用，不改玻璃厚度，也不破坏
"只有正文有底"的分层取向。B 会引入新的视觉结构，C 与规格冲突，D 自毁前提。

**待用户决策**——这属于观感取向与层级设计的取舍，不自行决定。

### D12 · normal 模式 tertiary 文字与方向箭头（未处理，登记为已知缺口）

**背景**：D11 的评审复算发现，`--text-tertiary` 在 normal 模式下比 D11 要
修的缺陷更差，但此前未被发现 —— R-D 的 140 个样本只采集有文本的叶子节点，
SVG 箭头被跳过，折叠态语言方向又不在采样矩阵内。

**涉及节点**：

| 节点 | 来源 | 说明 |
|---|---|---|
| `.popup-titlebar__lang` | 折叠态语言方向 | 直接读 tertiary |
| `.popup-titlebar__lang-arrow` | 折叠态「→」 | tertiary **再叠** `opacity: 0.55` |
| 空闲态「等待翻译…」 | `PopupWindow.tsx` | 直接读 tertiary |
| 语言方向箭头（展开态） | `ResultView.tsx` SVG | `stroke="currentColor"` → tertiary |

**实测对比度**（normal 模式，压在玻璃上）：

| 背景 | 深色 `#98989D` | 浅色 `#6E6E73` |
|---|---|---|
| 白桌面标题栏 (139,139,139) | **1.19:1** | — |
| 纹理桌面 (132,132,133) | 1.30:1 | — |
| 黄桌面 (150,128,27) | 1.35:1 | — |
| 黑桌面操作栏 (109,108,110) | 1.82:1 | — |
| 黑桌面操作栏 (147,147,147) | — | **1.65:1** |
| 中灰桌面 (128,128,128) | — | 1.28:1 |
| 白桌面标题栏 (200,200,200) | — | 3.03:1 |

**但「箭头看不见」是误判**（2026-10-22 复算）：对比度只量亮度差，而这些
灰对灰的组合亮度差小、色度差仍在。实测 ΔE76：深色白桌面 5.8、纹理桌面
8.1、黑桌面 17.3、黄桌面 56.6；浅色黑桌面 14.7。全部高于 JND 2.3。

真实问题不是「某背景下不可读」，而是**同一元素在不同壁纸间的可见度剧烈
跳变**（ΔE76 5.8 → 56.6）。用户换一张壁纸，这一行就从「清晰的指示」变成
「很淡的痕迹」再变成「跳出来的箭头」。

**折叠态才是真问题**：`.popup-titlebar__lang-arrow` 除 tertiary 外还叠了
`opacity: 0.55`，双重衰减后深色合成色 `#929295`，对比底色 ΔE76 只有 3.2，
基本贴着 JND。

**WCAG 口径**：1.4.11 要求「理解内容所必需的图形」达 3:1。该箭头夹在两个
语言标签之间、顺序固定（源 → 目标），信息冗余 —— 按「装饰性分隔」可豁免，
按「方向指示」则 normal 模式 7 个背景下 6 个不过。这是规范解释的选择，
不是纯技术判断。

**为什么逐 token 调色是死路**（实测）：

- 浅色侧要达 4.5:1，tertiary 必须压到约 `#2A2A2C`，与 D11 提级后的
  secondary `#262628` 几乎同色 —— 层级彻底消失
- 深色侧即使纯白也只有 **3.41:1**，仍不可达 4.5:1
- alpha 提到 0.55 时 tertiary 也只有 2.52:1

**候选路径**：

| 方案 | 做法 | 代价 | 效果 |
|---|---|---|---|
| A | 改 `--text-secondary`，箭头随标签一起提级 | 箭头与标签同色，失去分隔感 | 7 个背景下 6 个过 3:1（深色白桌面 2.92 仍差一点） |
| B | 给标题栏加局部衬底（同 D11 方案 B） | 多一条色带，与「只给正文衬底」的取向冲突 | 可与 D11 的深色 18 条一并解决 |
| C | 折叠态去掉 `opacity: 0.55` | 折叠态箭头与语言标签同权重 | 深色 ΔE76 3.2 → 5.8，成本极低 |
| D | 改 `--popup-text-primary` | 与正文同色，11px 行里偏重 | 7 个背景全部过 3:1 |

**建议**：C 先做（一行改动，解决最贴 JND 的那个），A/B/D 留待与 D11 的
深色 18 条一并决策 —— 它们本质是同一个开关。

**状态**：未处理。登记为已知缺口，不作为 v0.8.0 的阻塞项 —— 它不影响
功能可用性，且发行说明面向用户，不宜罗列内部可读性细节。

---

### R-E · 方案 A 实施与复测（2026-10-22）

**实施**：`globals.css` 在 `:root:where(:not(.reduce-transparency))` 下把浮窗
作用域内的 `--text-secondary` 浅色 `#515154` → `#262628`、深色 `#C0C0C5` →
`#EDEDF0`。外层套 `:where()` 是为了把特异性压回 (0,1,0)，与本文件既有的
`.popup-container` 覆盖约定对齐（`:not()` 里的类选择器会算特异性，直接写
`:root:not(...) .popup-container` 是 (0,2,0)，会让将来照约定补的规则被静默压掉）。

**初版曾用 `@media not all and (prefers-reduced-transparency: reduce)` 包住，
评审后已移除**。原因：该写法只在引擎「认识这个特性名」时才返回 true。
Chromium 认识（实测 `not all and (prefers-reduced-transparency: reduce)` 为
true，而 `not all and (prefers-reduced-data: reduce)` 为 false），但 WebKit
是否认识无从得知；MQ L4 规定未知特性求值为 unknown、`not unknown` 仍是
unknown、整条降级为 `not all`（false）。若 WebKit 不认识，整块会在 macOS
上静默失效。改为只依赖 JS 挂的 class 后，四个窗口引擎行为一致，且规则可被
jsdom 解析。

排除 reduce 的方式改为：normal 块用 `:where(:not(.reduce-transparency))` 排除
应用内开关；**系统媒体查询路径则在 reduce 块内显式恢复基值**
（`--text-secondary: #515154 / #C0C0C5`）—— 系统偏好不改变 class，两边的
`:not()` 都成立，必须显式覆盖回来。恢复规则与 normal 块特异性同为 (0,1,0)，
靠源码顺序决胜（reduce 块在后）。

八种组合（系统偏好 × 应用开关 × 浅深）已在 Chromium 实测，层叠结果全部正确。

深色没取纯白：`CopyButton` 的 hover 色当时读的是**全局** `--text-primary`
（深色 #F5F5F7），不是浮窗的 `--popup-text-primary`（#FFFFFF）。`#EDEDF0`
与前者是 1.07:1（改前 #C0C0C5 是 1.66:1）。**这是本改动的已知代价**：hover
颜色提示浅色由 2.13:1 降到 1.11:1、深色由 1.66:1 降到 1.07:1，对应 ΔE76
4.39 / 2.81，都还高于 JND 2.3，但明显弱于改前。已在 R-F 用独立 token 修复。

**更正（2026-10-22 复算）**：本节初版写「取纯白会降到 1.00:1，hover 的颜色
提示彻底消失」，是错的。#FFFFFF vs #F5F5F7 实测 **1.089:1 / ΔE76 3.46**，
仍高于 JND —— 不会彻底消失，只是很弱。当时的判断错在用对比度比值衡量近白区
的差异：亮度比在 L* 接近 100 的区域严重低估感知差异，这里应当用 ΔE76。
同一节的另一个错误是把 `hover:bg` 写成「背景变化仍在」，实测它在玻璃上已被
稀释到 ΔE76 2.30（浅）/ 2.69（深），基本等于不存在 —— 见 R-F。

**复测结果**（同一套 6 背景 × 4 模式，168 个样本）：

| 模式 | 修复前最差 | 修复后最差 | 状态 |
|---|---|---|---|
| 浅色 normal | 2.57:1 | **4.92:1** | ✅ 全部达标 |
| 浅色 reduce | 6.94:1 | 6.94:1 | ✅ 未受影响 |
| 深色 reduce | 7.48:1 | 7.48:1 | ✅ 未受影响 |
| 深色 normal | 2.09:1 | **2.92:1** | ⚠️ 仍有 18 条未达标 |

失败从 42 条降到 18 条，全部集中在深色 normal。

**深色 normal 的物理上限（本次实测确认）**

18 条失败的实测底色分布：

| 底色 | 纯白可达上限 | 出现场景 |
|---|---|---|
| (139,139,139) | 3.41:1 | 白桌面 · 语言标签 |
| (132,132,133) | 3.74:1 | 纹理桌面 · 语言标签 |
| (150,128,27) | 3.89:1 | 黄桌面 · 语言标签 |
| (128,128,128) | 3.95:1 | 白桌面 · 操作栏 |
| (139,116,5) | 4.57:1 | 黄桌面 · 操作栏 |
| (109,108,110) | 5.22:1 | 纹理桌面 · provider 名 |

**前 4 类底色下，纯白文字也只有 3.41–3.95:1，任何文字色都到不了 4.5:1。**
这不是配色选得不够亮，是底色本身太亮。后 2 类（(139,116,5) 与
(109,108,110)）纯白能到 4.57 / 5.22 —— 但取纯白会与 primary 同色，
hover 提示归零（见 R-E），所以这两类在方案 A 的约束下也放弃了。

**为什么深色底色会这么亮**：Windows 11 22H2+ 的 Acrylic 走
`DWMWA_SYSTEMBACKDROP_TYPE`，**忽略 tint 色、跟随系统明暗**。已实测确认
`DWMWA_USE_IMMERSIVE_DARK_MODE=1`、`BACKDROP_TYPE=3`（Acrylic）均已正确施加，
所以不是 bug。该材质在**白色桌面**上合成为 `(139,139,139)` 中灰——
比浅色模式在同样桌面上的 `(232,232,232)` 暗得多，但比深色主题需要的
深底亮得多。tint 的 0.16 被系统材质压缩到几乎不起作用。

**要跨过这 4 类底色有三条路**（都越出方案 A）：

| 方案 | 做法 | 代价 |
|---|---|---|
| B | 给标题栏与操作栏加局部衬底（同 `.popup-reading-surface`） | 视觉上多两条色带，与"只有正文有底"的取向冲突 |
| E | 深色 normal 下把 `--popup-glass-tint` 提高 | 见下方代价实测 |
| C | 接受现状并如实记账 | 与 4.5:1 硬门槛冲突 |

**方案 E 的代价此前被高估了。** 原写「≥0.55 等于退回色板」，实测不成立：
从 `(139,139,139)` 反解 tint 之下的原始 Acrylic 亮度 ≈160，按 sRGB 合成：

| tint alpha | 合成底色 | `#EDEDF0` | 纯白 | 不同桌面间的差异幅度 |
|---|---|---|---|---|
| 0.16（现状） | 139 | 2.92 | 3.41 | 177 |
| 0.40 | 107 | **4.56** | 5.33 | 126 |
| 0.55 | 87 | 6.18 | 7.23 | **95** |

alpha 提到 **0.40 就跨过 4.5:1**（增量 0.24，不是原估的「≥0.55」）；即使
0.55，不同桌面之间仍有 95 的差异幅度，玻璃感依然存在，不是色板。
原表述把政策上限（不抬高 alpha）写成了物理上限。

**结论**：方案 A 在浅色侧完全达标、reduce 两侧完全达标；深色 normal
在**当前 tint 0.16 的取向约束下**不可达，剩余 18 条需 B 或 E 才能解决。
这是「所选 alpha 的属性」，不是材质的物理属性 —— 抬高 alpha 就能跨过，
代价是玻璃变厚，该代价未经用户确认。

#### 遗留（评审发现，H1 已修，H2 升为 D12）

**H1 · `CopyButton` 的 hover 颜色提示变弱 —— 已修（R-F）**

方案 A 把 secondary 推近 primary，`hover:text-[var(--text-primary)]` 的
颜色变化随之压缩：浅色 2.13:1 → 1.11:1（ΔE76 23.71 → 4.39），深色
1.66:1 → 1.07:1（ΔE76 18.82 → 2.81）。

**初版判断「换色无解」是错的**，理由有二：

1. 当时只考虑在 `--text-secondary` 侧换色（那确实无解），没有考虑给
   `CopyButton` 的 hover 目标单独开一个 token。浮窗作用域内可以让 hover
   走 `--popup-text-primary`，深色即 #FFFFFF，ΔE76 6.37。
2. 初版把 `hover:bg` 当成「仍在的兜底提示」，实测它在玻璃上被稀释到
   ΔE76 2.30 / 2.69 —— 正好压在 JND 上，几乎等于不存在。真正有效的兜底
   不是它，而是把它提到有效强度。

处置见 R-F。

**H2 · `--text-tertiary` 与方向箭头 —— 未处理，见 D12**

涉及折叠态语言方向、空闲态提示、SVG 方向箭头。内容已移入独立条目 D12，
避免埋在 D11 的遗留小节里难以检索。

> 注意：reduce 的两条路径**都**显式覆盖了 `--text-tertiary`
> （`#69696E` / `#C7C7CC`），说明该 token 在薄衬底上撑不住是已知问题，
> 唯独 normal 模式此前没管。

---

### R-F · H1 修复（2026-10-22）

**方案 ① + ③，不采用字重 / 透明度。**

**① 浮窗专用 hover token。** 新增 `--copy-hover-text`，`:root` 值为
`var(--text-primary)`（history 窗口行为一字不变），浮窗作用域内改为
`var(--popup-text-primary)`。`CopyButton.tsx:49` 的
`hover:text-[var(--text-primary)]` 改为 `hover:text-[var(--copy-hover-text)]`。

为什么不能直接用 `--popup-text-primary` 替换：深色下它（#FFFFFF）与全局
`--text-primary`（#F5F5F7）不同，直接换会顺手改掉 history 窗口的 hover。
浅色下两者同为 `#1D1D1F`，所以浅色侧实际收益是 0 —— 浅色的 hover 提示
仍是 ΔE76 4.39，只是不再有「比目标色更接近」的额外损失。

修复后：深色 normal ΔE76 2.81 → **6.37**（改前基线 18.82）。

**③ 浮窗 hover 背景提级。** 浮窗作用域内 `--hover-bg` / `--active-bg`：
浅 `0.04/0.08 → 0.10/0.16`，深 `0.06/0.10 → 0.12/0.20`。

| | 不透明窗口 | 玻璃浮窗（旧值） | 玻璃浮窗（新值） |
|---|---|---|---|
| 浅 hover ΔE76 | 3.46 | **2.30** | 5.79 |
| 深 hover ΔE76 | 6.79 | **2.69** | 5.36 |
| 浅 active ΔE76 | 6.9 | 4.62 | 9.32 |
| 深 active ΔE76 | 11.4 | 4.60 | 8.76 |

旧值在玻璃上被稀释 34% / 60%，ΔE76 正好落在 JND 上。新值恢复到不透明
窗口的提示强度。reduce 路径必须还原成全局值 —— 那里的衬底是 0.94/0.92
近实色，没有稀释，沿用提级值会变成 ΔE76 8.46 / 11.22 的重压块。

**不采用字重**：`译文`/`原文` 两按钮并排，500 → 600 会让中文字面宽度变化
把第二个按钮右推 1–2px，每次 hover 都抖一次。**不采用透明度**：rest 侧
降到 0.85 会把深色 normal 的常驻对比度从 2.92 退回 2.54，等于抵消 D11。

**验证**：8 种组合（系统偏好 × 应用开关 × 浅深）在 Chromium 与 jsdom
双引擎下全部正确；history 窗口无 `.popup-container` 祖先时 token 保持全局
值（已用 CDP 确认）；`tsc --noEmit` 通过；33 个测试全过。

