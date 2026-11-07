# QuickTranslate Agent Progress

## Current status
- Branch: main
- Base commit: b64b04e（Merge pull request #80）
- App version: 0.9.0
- Current task: Task 08 Provider 管理中心第一轮（完成）；剩余项依赖 AI 解禁或产品决策
- Working tree: 干净
- 执行依据: docs/QUICK_TRANSLATE_AI_AGENT_DEVELOPMENT_PLAN.md（2026-11-07 修订）
- 测试清单: docs/TEST_INVENTORY.md

## Completed

### 2026-11-07 — Task 08 Provider 管理中心（第一轮：可独立验证项）
- 核验结论（已具备，无改动）：
  - 实施 2「连接测试不污染熔断」：`validate_credentials` 不走熔断计数，专项测试锁定（translator/mod.rs「计划第 29 节」，Task 03 已核验）。
  - 实施 3「Key 获取入口」：四个配置型 Provider 均有 setupUrl（腾讯云 CAM / DeepL Pro API / 百度翻译开放平台 / 有道智云），设置页与引导页渲染；open_url 有白名单校验（commands/system.rs 测试）。
  - 实施 5「替换凭证重置健康」：`updating_credentials_resets_a_stuck_circuit`（Task 03 已核验）。
  - 实施 7「不在前端硬编码服务商错误解析」：ErrorView 只按 `error_code()` 契约分类，不解析上游原始错误（本轮 ErrorView 测试顺带锁定「契约文案优先于原始 message」）。
- 改动：
  - **状态词表抽出 + 测试锁定**（实施 1）：`providerStateLabel`/`STATE_TONE_CLASS` 移入 `lib/providerState.ts`；契约测试覆盖「未配置优先于健康状态（二者不混淆）」、open/degraded 按错误类别细分四词、same_language 成功不映射为错误词。
  - **密钥显示/隐藏切换**（实施 4）：CredentialField 对 password 型字段加眼睛按钮（aria-label + aria-pressed）；mousedown preventDefault 不抢焦点——blur 即提交语义下，抢焦点会把半截密钥当完整凭证持久化。+5 组件测试（含「输入半截密钥切换显示不触发提交」）。
- 未做（依赖外部条件）：实施 8/9 与供应商目录分类依赖自定义/AI Provider 能力（要求 19 暂缓）；「恢复默认配置」功能尚不存在，做时须遵守不清凭证/历史。
- Files: src/lib/providerState.ts（新增）、src/lib/__tests__/providerState.test.ts（新增）、src/windows/settings/{SettingsWindow.tsx,__tests__/CredentialField.test.tsx}
- Tests: vitest 64/64（+11）；tsc PASS；build PASS
- Commit: ee17782 / 647ba95

### 2026-11-07 — Task 07 浮窗交互与可访问性（核验 + 补测试）
- 核验结论（计划 §7 实施项 1–6 与全部验收项，现状已具备，无需改代码）：
  - **状态机**：idle/loading/success/error 四态显式管理；sr-only live region 常驻播报（此前只挂折叠态的缺陷已修过）；缓存/无需翻译结果有「本地缓存」「无需翻译」标识（PROVIDER_LABELS cache/none 条目）；截断有「已截断」标签；敏感过滤命中不弹窗 + toast 带「仍然翻译」动作（Task 04 设计，filter_gate 在建窗之前）。
  - **错误恢复**：ErrorView 按 `SETTINGS_FIX_CODES`（AUTH_ERROR/QUOTA_EXHAUSTED）分流——修复动作在配置里的指向「前往设置」（直达 provider tab），其余告知真实重试路径「重新复制文字即可重试」。
  - **长文本**：译文区 maxHeight 300px 滚动 + break-words；窗口尺寸 place_popup 纯函数 clamp 280–520 × 40–480（19 个既有测试覆盖边界）。
  - **键盘/可访问性**：Esc/Space/Enter + CopyButton 可 Tab（可见文本即 accessible name）+ TrafficLights 无障碍契约测试（3 条曾为真实缺陷）。
  - **主题/透明**：浅深双主题 CSS 变量体系 + reduce_transparency 应用内开关叠加系统媒体查询（macOS WKWebView 不支持该媒体查询的回退已实现）。
- 补充：ErrorView 恢复操作分流组件测试（+5）——锁定「错误操作直接引导修复」这条验收的行为契约；未知错误码回退契约文案、不泄漏后端原始消息。
- 未自动化（需实机视觉验证，如实记录）：深/浅色与降低透明下的对比度观感、emoji/RTL/混合语言实际渲染、DPI/多屏实摆位（place_popup 逻辑已有单测，实机定位未验）。
- 记录结论（不改动）：计划实施 4 的「重新翻译」按钮——当前重试路径是重新复制（剪贴板监控自动重译），浮窗被动→激活的交互模型下按钮增益有限且需新增后端命令与暂存语义；验收未硬性要求，标为可选增强待产品决策。
- Files: src/windows/popup/__tests__/ErrorView.test.tsx（新增）
- Tests: vitest 53/53（+5）；tsc PASS
- Commit: 9b37be9

### 2026-11-07 — Task 05 设置与配置一致性
- 核验结论：批量原子性（QT-010 all-or-nothing，8 测试）与即时生效模型已具备；敏感过滤开关提示 Task 04 已加。发现两个真实缺口并修复：
  - **重开设置页显示旧值**：托盘切换翻译源/目标语言/开关剪贴板监控是绕过命令管线的直写路径且不广播；设置窗口 hide 而非销毁、草稿只在挂载时加载一次。修复：后端在持久化与副作用落地后广播 `config-changed`（载荷与 get_config 同构、凭证保持脱敏）；设置页监听后按字段合并，凭证字段（掩码占位符）与 auto_start（注册表才是权威）不合并。
  - **快速切换同键竞态**：前一次请求迟到的失败会无条件回滚，把后一次乐观写入打回旧值（后端却以后一次为准）。修复：回滚加守卫（草稿仍持有本次写入的值才允许就地回退，`mayRollBackToPrevious`）+ 失败后拉取 get_config 对账（`mergeExternalConfig` 统一跳过凭证/auto_start/在途键），界面最终收敛到后端真值。
  - **剪贴板监控开关补隐私说明**（计划 Task 05 第 6 项）：说明本机判定边界、翻译时的发送对象、与敏感过滤的衔接。
- 合并规则纯逻辑抽到 `src/lib/configSync.ts`（+11 单测：凭证跳过、auto_start 跳过、在途跳过、undefined 忽略、回滚守卫、CREDENTIAL_KEYS 完备性）。
- Files: commands/config.rs、system/tray.rs、src/lib/configSync.ts（新增）、src/lib/__tests__/configSync.test.ts（新增）、src/windows/settings/SettingsWindow.tsx
- Tests: `cargo test --lib` → 366 passed；fmt/clippy PASS；tsc PASS；vitest 48/48（+11）；build PASS
- Commit: 10cebfc / 1e57eb9 / 86e25d5
- 记录结论（不改动）：`set_clipboard_monitor_enabled`（前端无调用方）与 `complete_onboarding`（onboarding_completed 设置页不展示）的直写不广播，不影响设置页正确性；浮窗的主题/材质同步沿用专有事件通道，不需要 config-changed。

### 2026-11-07 — Task 06 缓存、历史和数据管理（核心项）
- Changes:
  - **修复真实缺陷**：`ORDER BY created_at DESC` 缺第二排序键，同毫秒记录的翻页顺序不确定（SQLite 排序的偶然行为，非契约）。三处查询 + FIFO 清理补 `rowid` 第二键；探测代码确认当前行为后以测试锁定契约。
  - **历史查询契约测试**（+12）：CRUD（toggle_star/delete/clear）、FIFO 只删最旧未收藏、LIKE 转义（`%`/`_` 字面量）、中文子串检索、limit/offset 钳制、starred 过滤、收藏持久性。
  - **大数据量测试**（计划 6.6/6.9）：1 万条下首页/深分页走 `idx_records_created_at` 索引、CJK 搜索、500ms 预算断言；万条规模 FIFO 精确删除且收藏记录存活。
  - **迁移升级测试**（+3）：v1→最新升级保留既有记录（新列默认值可用）、重复迁移幂等、版本行恰好记一次账。
  - **持久化丢弃计数**（计划 3.3）：队列满导致的写入丢弃从静默 warn 变为可观测指标 `persistence_dropped_count`，状态面板展示「落盘丢弃 N 条」。
  - **独立的清空缓存入口**（计划 3.3）：`clear_translation_cache` 命令 + 设置页操作行，带二次确认对话框（说明影响范围：历史不受影响），与清空历史互相独立。
  - **落盘 worker 关闭语义核验**（计划 6.9，纯核验无改动）：worker 循环 `while let Some(job) = rx.recv().await`——tokio mpsc 的 `recv()` 仅在通道关闭且缓冲排空后才返回 None，因此 sender（PersistenceWriter）drop 时 worker 会先消化完队列内剩余写入再退出（有限 drain）。进程被硬杀时最多丢 32 条缓冲写入：有界、WAL 不损坏、丢弃已有诊断计数。未加显式退出等待钩子——进程退出不保证 async 任务被轮询，等待钩子是伪保障。
- Files: domain/history.rs、domain/cache.rs、infra/database.rs、system/persistence.rs、runtime/{status,lifecycle}.rs、commands/history.rs、lib.rs、src/lib/commands.ts、src/windows/settings/SettingsWindow.tsx
- Tests: `cargo test --lib` → 366 passed（+19）；clippy/fmt PASS；tsc PASS；vitest 37/37；build PASS
- Commit: 564b733 / 7f81ab7 / 5462e19 / 3e47bd9
- 未做（留待后续）: 缓存禁用/启用开关（默认保持开启，涉及隐私说明文案，属产品决策项）；「清空缓存」是否加入托盘菜单。

### 2026-11-07 — Task 04 剪贴板隐私防护与过滤引擎
- Changes:
  - 新增 `domain/clipboard_filter.rs`：12 条高置信度规则（PEM 私钥、JWT、AWS Key ID、GitHub/Slack/Stripe/Google/OpenAI/Anthropic 令牌、腾讯云 SecretId、HTTP Authorization 头、DeepL 免费版密钥）。纯函数、零依赖（字节级扫描，未引入 regex）、零 I/O。
  - 负例策略：普通散文、代码片段、裸 UUID、`sk_test_`、短令牌、嵌入长词的中段命中一律放行；规则各有正反例测试，含「词边界」与「多行日志」场景。
  - 翻译入口前置拦截：`coordinator::execute_at_position` 在 supersede/建窗/启动任务**之前**调用 `filter_gate`；命中即 toast 解释（不含原文）+ 日志只记规则码与长度。
  - 手动覆盖：`translate_filtered_text` 命令，只能消费 coordinator 暂存的最近一条被拦内容（取走即清、新复制即失效），前端由 toast 上的「仍然翻译」按钮触发。
  - 配置：`sensitive_filter_enabled`（默认开启）贯通 4 条路径（descriptor 表外的三处手写分支 + 命令层 BOOL_KEYS），并有往返测试。
  - 诊断：`clipboard_filtered_count` 计数（纯记账，不含原文）。
- Files: domain/clipboard_filter.rs（新增）、system/translation/coordinator.rs、domain/config.rs、commands/config.rs、commands/translate.rs、types.rs、runtime/{status,lifecycle}.rs、system/updater.rs、lib.rs、src/lib/commands.ts、src/components/{Toast,ToastManager}.tsx、src/windows/settings/SettingsWindow.tsx
- Tests: `cargo test --lib` → 347 passed（+33）；clippy `--lib -D warnings` PASS；fmt PASS；`npx tsc --noEmit` PASS；`npm test` 37/37 PASS；`npm run build` PASS
- Result: 过滤发生在任何网络请求之前（结构保证：早退路径上无 I/O）；普通文本不误拦。
- Commit: 2f0049d / bec7a0f / cee1d25 / 469fc86
- 未做（按计划明确要求）: 应用排除列表 —— 当前剪贴板 API 在两平台都无法可靠获得来源进程，标为平台能力待评估，待产品决策。

### 2026-11-07 — Task 03 Provider 错误契约与 Fallback 完整性（核验，无代码改动）
- 核验结论：错误分类表（9 测试）、熔断状态机（13）、429 Retry-After 边界、fallback 关闭时不调用备用源、凭证更新重置熔断（含 `updating_credentials_resets_a_stuck_circuit`）、手动验证不污染熔断、`last_error_class` 暴露到 `ProviderStatus` —— 全部已有实现与测试对应。
- 唯一补充空间（诊断面板展示最近成功/失败时间）属于设置页展示增强，非契约缺口，留待 Task 08。

### 2026-11-07 — Task 02 翻译请求生命周期与竞态测试
- Changes: 新增引擎级取消语义测试 `aborted_task_never_continues_the_fallback_chain`（domain/translator/mod.rs）。用 `Outcome::Slow(Duration::MAX)` 把任务钉在主源首次尝试中，abort 后断言 A 不被重试、B（fallback 源）零调用。补齐「取消请求不会继续发起 Fallback」这条验收的结构性证明；其余验收项（迟到结果丢弃、安装拒绝、窗锁串行、commit_popup_show 协议）已有 14 个 coordinator 测试覆盖。前端 useTauriEvent 的注册竞态防护与自动解绑经核读确认无缺陷。
- Files: src-tauri/src/domain/translator/mod.rs
- Tests: `cargo test --lib` → 314 passed / 0 failed（新增 1）；`cargo fmt --check` PASS；`cargo clippy --lib -- -D warnings` PASS（--tests 目标 16 个既有 test 告警，见 TEST_INVENTORY 说明，非本次引入）
- Result: Task 02 验收项全部有测试对应。
- Commit: d3b3f97

### 2026-11-07 — Task 01 建立可重复的质量基线
- Changes: 无代码改动。核验全部基线门禁、Mock 注入能力与 CI 覆盖；建立测试清单文档与本进度文件。
- Files: docs/TEST_INVENTORY.md（新增）、docs/agent-progress.md（新增）
- Tests:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib` → 313 passed / 0 failed
  - `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` → PASS
  - `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` → PASS
  - `npm test` → 37 passed / 8 文件
  - `npx tsc --noEmit` → PASS
- Result: 基线全绿。Mock Provider / mock HTTP / mock clipboard 注入能力已确认存在（详见 TEST_INVENTORY）。
- Commit: （见 git log）
- 文档差异结论: 开发计划与代码一致，无需修订；docs/implementation/ 为 0.8.0 时期审计整改档案（QT-001~014 已全部 VERIFIED），仅作参考。

## In progress
- Task: 无

## Blockers / decisions needed
- `@tauri-apps/plugin-global-shortcut` 依赖已声明但完全未接线：Task 11 前需决策「实现快捷键」还是「移除依赖」。不阻塞其他任务。
- 应用排除列表（Task 04）：当前剪贴板 API 在两个平台均无可靠的来源进程标识，需按计划先实现敏感过滤 + 暂停，应用排除标为平台能力待评估。

## Risks not yet verified
- Platform: macOS
  - Scenario: 浮窗定位、剪贴板 changeCount 语义、打开外部 URL
  - Required manual test: QT-001/002/004 的 macOS 实机验证（上轮审计遗留，EXTERNAL_VALIDATION_REQUIRED）
- Platform: Windows + macOS
  - Scenario: 真实翻译服务商的 contract tests（当前均为 mock 层验证）
  - Required manual test: 显式 opt-in 的真实凭证测试

## Next task
- Task 08 剩余项被两处外部条件阻塞：供应商目录分类/自定义兼容接口/「AI 模型」类目依赖要求 19 解禁；恢复默认配置的产品语义待确认。可选推进：Task 09（统一手动翻译入口，纯传统翻译链路，无 AI 依赖）、Task 10（术语表）或缓存开关（待文案确认）。
