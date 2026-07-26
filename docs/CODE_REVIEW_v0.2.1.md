# QuickTranslate 代码库综合审查报告

> **审查范围**：`v0.1.2..v0.2.1`（PR #1–#9，17 文件，+585/−147 行）
> **审查日期**：2026-07-26
> **方法**：10 角度多代理审查（5 正确性 + 3 清理 + 1 架构深度 + 1 规范）→ 去重 → 逐项读码验证 → 独立扫尾
> **结论**：15 项确认发现（9 正确性 + 6 清理/架构），其中 3 项建议下个补丁版本内修复

---

## 执行摘要

本轮变更（浮窗红绿灯、空格关闭、剪贴板自触发防护、配置解码修复、更新端点修复）整体质量良好，三个"静默失败"级历史缺陷被正确根治。但新引入的交互层存在**一组共性问题**：窗口尺寸恢复路径不完整、全局快捷键与焦点按钮争抢、事件订阅闭包过期。三者都源于同一设计张力 —— 折叠/宽度状态引入后，"谁负责在何时恢复窗口尺寸"没有单一归属。

| 严重度 | 数量 | 概述 |
|:--|:--|:--|
| P0（用户可见功能缺陷） | 3 | 44px 卡死、按钮键盘激活失效、loading 宽度回弹 |
| P1（边界/竞态缺陷） | 4 | 标志时序竞态族、写失败标志残留、宽高不匹配、提示永不可见 |
| P2（低概率/开发期缺陷） | 2 | 非 Windows 行为矛盾、StrictMode 双 IPC |
| 清理/架构 | 6 | 组件复制、双解码器、三处尺寸契约、双格式配置表等 |

## 审查方法与可信度说明

- **成功的 finder 代理**：Angle B（删除行为审计）、Angle C（跨文件追踪）、Reuse+Simplification、Altitude+Conventions，共产出 21 个候选。
- **失败的 finder 代理**：Angle A/D/E/Efficiency 共 9 次尝试全部死于环境级 subagent 输出上限（`CLAUDE_CODE_MAX_OUTPUT_TOKENS=6000`），与提示词、代理类型、范围无关。这些角度由主审查者直接逐行读码覆盖，其结论中的 3 项后被 B/C 代理独立复现，交叉验证成立。
- **验证方式**：全部候选逐项读码验证（非代理投票），13 项 CONFIRMED、2 项 PLAUSIBLE、1 项机制归因 REFUTED（CSS transition 不触发于百分比 used-value 变化，但其失败场景由布局竞态成立，已并入 F6）。
- **值得记录的误判纠正**：主审查者初判"`preventDefault` 可避免焦点按钮与全局快捷键冲突"，Angle B 证明方向相反 —— `preventDefault` 正是杀死按钮原生激活的原因（见 F2）。多角度冗余的价值在此实证。

---

## P0 — 用户可见功能缺陷

### F1 · 折叠后从 error/loading/idle 展开，窗口卡死在 44px

**位置**：`src/windows/popup/PopupWindow.tsx:92`（四方独立确认）

量高效应只在 `result` 非空时运行：

```tsx
if (!result || collapsed) return;
```

而 `handleToggleCollapse` 只在**折叠方向**主动调 `resizePopup`，展开完全依赖该 effect。`translationStore` 的 `setLoading`/`setError` 都会置 `result = null`，于是：翻译报错 → 折叠（44px）→ 展开 → effect 提前返回 → 窗口停在 44px，ErrorView 在 `overflow-hidden` 下被裁切且无滚动条，直到下一次成功翻译才恢复。

**修复方向**：展开分支也走统一的尺寸恢复路径 —— 把"依当前 status 计算目标高度"收敛为单一函数（error/idle 用内容测量、loading 用 `LOADING_H`），折叠/展开/事件三处都调它。

### F2 · 全局 Enter/Space 拦截杀死所有按钮的键盘激活

**位置**：`src/windows/popup/PopupWindow.tsx:128-139`

`isEditableTarget()` 只排除 `INPUT/TEXTAREA/SELECT/contentEditable`，不含 `BUTTON`。键盘用户 Tab 到复制按钮或红绿灯后：

- 按 Enter → 全局 handler `preventDefault`（Enter 激活发生在 keydown，可被取消）→ 触发的是**折叠切换**而非按钮
- 按 Space → 同理被拦截 → 触发的是**关闭浮窗**

所有按钮沦为仅鼠标可用。**修复方向**：`isEditableTarget` 加入 `BUTTON`（或改判 `e.target !== document.body`），让焦点在控件上时全局快捷键让位。

### F3 · wide 模式下 loading 阶段宽度回弹 400px

**位置**：`src/windows/popup/PopupWindow.tsx:64-69` + `src/hooks/useTauriEvent.ts:39`

`handleLoading` 闭包捕获 `width`，但 `useTauriEvent` 的 effect deps 是 `[event, ...deps]` 且调用方未传 deps —— 监听器**挂载时注册一次**，永远持有 `width=400` 的初始闭包。切到 520px 后每次新翻译，loading 阶段都回弹到 400px，成功后才被量高效应纠正。

**修复方向**：优先用 ref 转发最新 handler（`useRef` + effect 内 `ref.current(e)`），避免把 handler 加入 deps 引发反复退订/重订。

---

## P1 — 边界与竞态缺陷

### F4 · `app_wrote_clipboard` 布尔标志的三重竞态族

**位置**：`src-tauri/src/system/clipboard_monitor.rs:170`（B/C 与主审查者三方交汇）

标志只是一个裸布尔，未绑定"预期写入的内容/序列号"，产生三个吸收错文本的窗口：

1. **写入前被消费**：monitor 的 `get_text()` 恰在 `mark_app_write()` 之后、OS 写入落地之前执行 → 吸收的是旧文本、标志被烧掉 → 下一轮把 app 自己的写入当新内容翻译 —— 本 PR 要防的自触发在竞态窗口内复活
2. **用户复制被吞**：用户在同一 500ms 轮询窗口内先点复制按钮、再复制新文本 → absorb 分支吸收的是用户的复制 → 翻译静默丢失
3. **暂停期存活**：suspend 分支只清 `pending_*` 不清此标志 → 暂停期间点复制按钮，恢复后第一次真实复制被吞

**修复方向**：`mark_app_write` 改为记录 `(expected_text_hash, seq_at_write)`，absorb 分支只在读到的内容匹配时消费；suspend 分支顺带清标志。

### F5 · `copy_to_clipboard` 先设标志后写入，写失败则标志残留

**位置**：`src-tauri/src/commands/system.rs:16-20`

剪贴板被其他进程占用导致 `write_clipboard_text` 返回 Err 时，`mark_app_write()` 已经执行 —— 残留标志会吞掉用户下一次真实复制。**修复方向**：仅在写入成功后设标志（顺序对调即可，与 F4 的内容绑定方案合并修复更佳）。

### F6 · wide 切换时单次 resize 传入不匹配的宽高

**位置**：`src/windows/popup/PopupWindow.tsx:96`

量高效应在 rAF 内测 `offsetHeight` 时 DOM 仍按**旧宽度**排版（OS 级 resize 是异步 IPC），却与**新宽度**捆绑在同一次 `resizePopup(width, offsetHeight)` 调用里。520→400 时文本重排变高 → 底部被裁；400→520 时留白。无后续补测。**修复方向**：宽度切换拆两步 —— 先只改宽，等 `onResized` 事件或双 rAF 后再测高补第二次调用。

### F7 · 「空格 关闭」提示永不可见

**位置**：`src/windows/popup/ResultView.tsx:74`

`hidden sm:flex` 的 `sm:` 断点是 640px，而浮窗视口被钳制在 280–520px —— 为可发现性而加的提示自身永不渲染。**修复**：去掉响应式前缀，直接 `flex`。

## P2 — 低概率与开发期缺陷

### F8 · 非 Windows 平台"重复复制再触发"承诺失效

`clipboard_seq()` 在非 Windows 恒返回 0，关闭浮窗后重复复制同文本永不再触发 —— 与 `reset_last_text()` 文档注释的承诺矛盾。产品当前 Windows-only，但 `cfg` 降级路径与注释的矛盾是活的。**修复方向**：修正注释，或非 Windows 回退到旧的"清空 last_text"语义。

### F9 · `resizePopup` 在 `setCollapsed` 函数式 updater 内执行副作用

**位置**：`PopupWindow.tsx:50-56`。StrictMode 会双调用 updater → 开发期每次折叠双份 IPC；与本 PR 自己在 `useTauriEvent` 修的正是同类问题。**修复**：副作用移出 updater。

---

## 清理与架构（C1–C6）

| # | 位置 | 问题 | 建议 |
|:--|:--|:--|:--|
| C1 | `ResultView.tsx:88` | `SourceCopyButton` 整段复制 `CopyButton`（相同 SVG/状态/1200ms 计时），且已漂移（缺 `className` 支持） | 直接 `<CopyButton text={result.source_text} label="原文" />` |
| C2 | `config.rs:230` | `decode()` 与 `ps()` 双解码器并存，仅差 fallback | `decode = ps(raw).unwrap_or_else(\|\| raw.trim().to_string())` |
| C3 | 三文件 | 尺寸契约三份拷贝：前端常量 400/520/44/160、后端 clamp 280–520/40–480、`translation_flow.rs` 的 400/300，仅靠注释同步 | 单一来源：窗口创建时由后端下发，或共享常量生成 |
| C4 | `config.rs` + `SEED_SQL` | 配置表双格式永久并存（seed 裸值 vs set() JSON 引号值），`decode()` 只是读侧垫片；CLAUDE.md 宣称的"全 JSON 编码"不变量实际不成立 | 一次性迁移 v5 重编码全部裸值行 |
| C5 | `PopupWindow.tsx:108` | 两份"是否交互元素"谓词独立维护且已分歧（BUTTON 只在拖拽名单）—— 该分歧正是 F2 的成因 | 抽单一 `isInteractive()` 帮助函数，两处共用 |
| C6 | `clipboard_monitor.rs:142` | reset 分支的 `get_text()` 冗余（`last_text` 在一切可达路径已持有相同值），每次隐藏多开一次系统级剪贴板锁；`pending_*` 重置散布四处 | 删冗余读取；抽 `reset_pending()` 帮助函数 |

**架构层观察**（Altitude 角度）：F4/F5/C6 同根 —— 自触发防护是两个布尔标志"贴"在轮询循环上，而 Windows 侧 `GetClipboardSequenceNumber` 本可作为唯一权威变更源。若未来出现第三个 app 内写剪贴板的功能（如"复制历史记录条目"），现结构需要第三个标志与第三个 absorb 分支。右深度方案：一个 `ExpectedClipboardState { text_hash, seq }` 抽象，所有 app 内写入方登记，循环只对未登记的变更触发翻译。

---

## 修复优先级建议

1. **随下个补丁版本（v0.2.2）**：F1、F2、F7 —— 三行级修复、用户日常可撞
2. **同批顺手**：F5（两行对调）、F9（移出 updater）、C1/C2（纯删重）
3. **需要设计的小方案**：F3（ref 转发 handler）、F6（两步 resize）、F4+C6（ExpectedClipboardState 重构）
4. **排期即可**：C3（尺寸单一来源）、C4（迁移 v5）、F8（注释/语义择一）

## 本轮变更中值得肯定的实践

- 配置布尔解码修复附带 4 个往返回归测试（本仓库首批 Rust 测试），且验证了"旧逻辑必失败"
- 更新端点修复用 HTTP 状态码前后对照实证，而非仅目测拼写
- `useTauriEvent` 竞态修复采用了与 `OnboardingWindow` 既有正确写法一致的模式
- 发行流程死 job 的删除以真实失败 run 为证据，且确认 `latest.json` 未被 `--clobber` 覆盖

---

*方法论备注：本报告由多代理审查管线产出 —— 4 个成功 finder（21 候选）+ 主审查者逐行覆盖 4 个失败角度（9 次 subagent 死于环境输出上限）→ 去重至 18 → 逐项读码验证（13 CONFIRMED / 2 PLAUSIBLE / 1 机制 REFUTED 但场景成立并入 F6）→ 独立扫尾未见新缺陷。交叉验证亮点：F1 四方独立发现；F2 由 Angle B 纠正主审查者误判。*


