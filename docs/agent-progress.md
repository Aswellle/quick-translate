# QuickTranslate Agent Progress

## Current status
- Branch: main
- Base commit: b64b04e（Merge pull request #80）
- App version: 0.9.0
- Current task: 全局快捷键（完成，待实机验证）；Task 10 术语表（DeepL/百度/有道/腾讯控制台引用 + 管理 UI 完成，真实 API 未验证）；下一步真实凭证验证或新的产品任务
- Working tree: 干净
- 执行依据: docs/QUICK_TRANSLATE_AI_AGENT_DEVELOPMENT_PLAN.md（2026-11-07 修订）
- 测试清单: docs/TEST_INVENTORY.md

## Completed

### 2026-11-08 — 快捷键：重新翻译上一次
- coordinator 记住最近一次通过敏感过滤并实际发起的原文（内存态，不落盘、不进日志；被拦截内容不记入），剪贴板与手动路径都写入。
- 新增 `retranslate_last` 动作（默认 CmdOrControl+Alt+R）：复用 `execute_at_position` 的完整入口；无内容时提示。
- Tests: cargo test --lib 406 passed；fmt/clippy PASS；vitest 88/88；tsc/build PASS
- Commit: 7e919c7 / 5bbc0f3
- 未验证：OS 注册与快捷键行为需实机验证。

### 2026-11-08 — 快捷键：显示/隐藏翻译浮窗
- 新增 `toggle_popup` 动作（默认 CmdOrControl+Alt+H）：可见时走 popup_watch 的唯一关闭路径；不可见时重新显示并重置看守基线；从未翻译过时提示而非创建空白浮窗。设置页第四行自动出现。
- Tests: cargo test --lib 406 passed；fmt/clippy PASS；vitest 88/88；tsc/build PASS
- Commit: 6a4f83d / a352335
- 未验证：OS 注册与快捷键行为需实机验证（同其余动作）。

### 2026-11-08 — 腾讯云术语库引用（TermRepoIDList）
- 参数核实：腾讯云 SDK 的 `TextTranslateRequest` 与官方更新历史确认 2024-07-04 起支持 `TermRepoIDList`（术语库 ID 列表），不是此前社区资料中的 `TermBaseId`。
- Changes: `TranslateOptions` 增加 `tencent_term_repo_ids`；请求体仅非空时序列化 `TermRepoIDList`（默认形状不变）；配置键为逗号分隔字符串（去空白/去重/最多 10 项）；术语表页提供腾讯云术语库 ID 输入（失焦提交）。
- Tests: cargo test --lib **406 passed**（+2）；fmt/clippy PASS；vitest **88/88**（+1）；tsc/build PASS
- Commit: 2ee7095 / 3a150f4
- 未验证：真实腾讯云请求中的术语库效果（需已创建术语库的账号）。


### 2026-11-08 — 百度/有道控制台术语库引用
- Changes:
  - `TranslateOptions` 增加 `baidu_use_term_base` / `youdao_vocab_id`；配置随请求读取，改配置立即生效，不重建 provider。
  - 百度开启后追加 `needIntervene=1`（官方接入文档的「我的术语库」开关）；有道配置后追加 `vocabId`。默认请求形状完全不变。
  - 配置键贯通读取/缓存/重载与命令校验；剪贴板与手动路径都传入。
  - 术语表页按当前翻译源显示入口：百度开关、有道 vocabId 输入（失焦提交）；**腾讯云参数名未从官方 API 参考确认，不提供配置，只保留说明**。
- Tests: cargo test --lib **404 passed**（+3）；fmt/clippy PASS；vitest **87/87**（+1）；tsc/build PASS
- Commit: 990bd05 / c907902
- 未验证：真实百度/有道请求中的术语库效果（需真实凭证与已开通术语库的账号）。


### 2026-11-08 — Task 10 术语表：DeepL 同步、缓存指纹与管理 UI
- Changes:
  - **引擎上下文**：新增 `TranslateOptions`（显式源语言 / glossary_id / 指纹）；trait 默认 `translate_with_options` 委托给 `translate`，其余 Provider 零改动；DeepL 覆盖后以 form 参数下发 `source_lang` 与 `glossary_id`。
  - **DeepL glossary 同步**（`system/glossary_sync.rs`，schema v9 `deepl_glossary_sync`）：按语言对创建/复用 glossary；指纹未变复用持久化映射；变化时先建新、更新映射、尽力删除旧 glossary；`ready=false` 轮询约 1s；同步失败只降级为「本次不应用」并记日志，绝不阻塞翻译。仅「术语表开启 + 当前源是 DeepL + 显式源语言 + 该语言对有条目」时工作。
  - **缓存键纳入术语指纹**：应用术语表的译文与未应用的互不命中；空指纹保持旧键格式，既有缓存不失效；离线回退按实际应用指纹查询。
  - **手动翻译**新增源语言选择（默认自动检测），命令新增 `source_lang` 参数并校验受支持语言。
  - **设置页「术语表」Tab**：语言对选择、启用开关、条目增删改、区分大小写、JSON/CSV 导入（文件选择）与导出（复制到剪贴板）、按语言对清空；当前 Provider 能力如实说明（腾讯/百度/有道只能引用控制台术语库，Google 不支持）。`ToggleRow`/`SettingsRow` 抽到 `SettingsControls` 避免循环依赖。
- Tests: cargo test --lib **401 passed**（+7：DeepL 请求参数、glossary 创建请求形状、缓存指纹隔离、同步映射读写/TSV/键构造）；fmt/clippy PASS；vitest **86/86**（+4）；tsc/build PASS
- Commit: 67ecf0e / 1d95e1c
- **未验证（如实记录）**：DeepL glossary 的真实 API 调用（无真实凭证，请求形状仅 mock 层验证）；术语表在真实翻译中的效果（服务商对术语的匹配语义、大小写行为）需 opt-in contract test；macOS 未验证。
- 未做：腾讯/百度/有道的「引用控制台术语库 ID/开关」入口；剪贴板路径的显式源语言设置（当前按 DeepL 限制不应用术语表）。


### 2026-11-08 — Task 10 术语表：本地数据层（schema v8）
- Changes:
  - schema v8：`glossary_entries`（source_term/target_term/note/case_sensitive + 时间戳），原文 NOCASE 唯一索引；迁移测试更新到 v8。
  - `domain/glossary.rs`：仓库 CRUD（重复大小写不敏感拒绝、更新不撞其他条目、存储不可用时列表为空/写入报错）、全量替换的批量原子性、CSV/JSON 导入导出（引号/逗号/换行、可选表头、批内重复校验）、术语表指纹（排序后 SHA-256，说明字段不参与，禁用/空表为空串）。
  - 配置键 `glossary_enabled`（默认开启，无条目时等价空操作）；AppState 注入仓库；9 个 IPC 命令（CRUD/清空/导入导出）。**尚未接入任何 Provider，尚未做前端 UI。**
- Tests: cargo test --lib 393 passed（+13）；fmt/clippy PASS
- Commit: c6db99d
- 后续（同日，9cbf0e0）：用户确认**按语言对管理**模型后，v8 表直接扩展 `source_lang/target_lang`（尚未发布，不追加 v9），仓库/命令/导入导出全部按语言对作用域，语言清单统一到 `types::SUPPORTED_LANGS`；测试 394 passed（+1）。
- **新发现的硬约束（已解决）**：DeepL 官方文档明确「glossary 不能用于源语言自动检测」。当前剪贴板/手动翻译都是自动检测源语言，因此要真正应用 DeepL 术语表，必须：
  1) 翻译请求带**显式源语言**（手动窗口加源语言选择；剪贴板路径需要用户指定源语言或保持不应用）；
  2) 术语条目按**语言对**组织（DeepL glossary 是一对源/目标语言一个词典，v3 多语言 glossary 仍要求请求语言对精确匹配）。
  用户已确认「按语言对管理」（方案 A），数据层已按此调整（见 9cbf0e0）。
- 未做：DeepL glossary 生命周期同步（创建/删除/失效）、缓存键接入指纹、前端管理 UI、其余 Provider 的「引用控制台术语库」入口。

### 2026-11-08 — Task 10 术语表：Provider 原生能力评估（完成评估，未动代码）
- 新增 `docs/GLOSSARY_CAPABILITY_ASSESSMENT.md`：按计划要求先核验原生 glossary 能力，结论（文档级，未用真实凭证）：
  - **DeepL**：支持，且是唯一可由应用端完整管理术语（创建/删除 glossary、翻译带 glossary_id）的现有 Provider；v2 无编辑接口，改条目 = 删旧建新；文本翻译可自动检测语言并应用匹配词典。
  - **腾讯云**：支持引用控制台术语库（参数名/形态待复核 API 参考页）；术语条目只能在腾讯云控制台维护。
  - **百度**：通用接口仅 `needIntervene=1` 开关（控制台「我的术语库」），应用不能管理条目；千帆版 `termIds` 属另一套 API。
  - **有道**：支持 `vocabId` 引用控制台上传的术语表；主要英中方向。
  - **Google**：当前 v2 Basic（API Key）不支持 glossary；v3 Advanced 需服务账号鉴权，属架构变更，首轮标记不支持。
- 据此确定实施约束：应用内术语表只对 DeepL 真正生效；其余 Provider 如实显示「引用控制台术语库/不支持」，不得静默忽略或假装生效；缓存键必须含术语表指纹；不做无差别字符串替换；术语不进日志。
- 未动业务代码；待复核清单见文档末尾（腾讯参数名、百度套餐限制、有道语言方向、DeepL ready 轮询策略）。
- Files: docs/GLOSSARY_CAPABILITY_ASSESSMENT.md（新增）
- Commit: 见下一条提交

### 2026-11-08 — 全局快捷键（首次实现，计划 Task 11）
- Changes:
  - 引入 `tauri-plugin-global-shortcut`（Rust 端注册/分发；离线环境锁定 2.3.2）。前端只编辑加速键并读取状态，不走插件 JS API。
  - 三个动作全部复用既有入口：**翻译剪贴板** → `coordinator::execute_at_position`（敏感过滤、代际闸门、历史/缓存落盘与剪贴板路径完全一致；监控暂停时也可手动触发一次）；**打开手动翻译** → 托盘同一个窗口实例；**暂停/恢复监控** → 与设置页共用 `apply_clipboard_monitor_switch`。
  - 注册失败只记录状态与日志、不影响启动；改动时先注册新值、成功后才注销旧值（冲突时旧快捷键保持可用）。
  - 加速键作为普通字符串配置键（`hotkey_translate_clipboard` / `hotkey_open_manual` / `hotkey_toggle_monitor`）走 descriptor 表，启动按配置注册；`get_hotkey_status` 返回配置值/实际生效值/失败原因。
  - 设置页「快捷键」分组：录制（window 捕获阶段，不依赖按钮焦点）、Esc 取消、Backspace 清除禁用、注册状态与冲突原因展示；`lib/hotkeys.ts` 负责键盘事件 → 加速键与按平台格式化。
- Tests: cargo test --lib 380 passed（+4）；fmt/clippy PASS；vitest 82/82（+12）；tsc/build PASS
- Commit: d41e66f / 3ae9b2c
- **未验证（如实记录）**：快捷键的 OS 注册、冲突提示与三个动作的端到端行为需要实机验证 —— 本机未启动 Tauri 应用实跑；macOS 完全未验证（`on_shortcut` 在主线程内联执行的处理已核读 tauri-runtime-wry 源码，但平台行为未实测）。默认加速键 `CmdOrControl+Alt+T/M/P` 可能与其他应用冲突，冲突时设置页会显示真实原因。
- 依赖去留：`@tauri-apps/plugin-global-shortcut`（JS 包）当前未被使用（本实现走 Rust 端）。保留还是移除留待后续小步（移除需同步 package-lock）。
- 未做：快捷键录制暂不校验跨平台键位差异。（「显示/隐藏浮窗」6a4f83d、「重新翻译上一次」7e919c7 已补齐）

### 2026-11-08 — 本地缓存读写开关（完成，Task 06 遗留项收尾）
- 背景：此前只能「清空缓存」，不想在磁盘留存译文副本的用户没有持续生效的选项。
- Changes:
  - `TranslationCache` 增加原子开关：关闭后 `get`/`put` 直接返回（不读不写），已有条目保留，重新开启恢复可用；清空缓存不受开关影响。语义定为「暂停」而非「销毁」，避免误点丢数据。
  - 配置键 `cache_enabled`（默认 true，不改变既有行为）贯通布尔键三条路径 + 命令层 BOOL_KEYS + 副作用分支（切换活体实例）+ 启动初始化（旧库无此键按开启处理）。
  - 设置页「行为」分组新增开关，说明关闭后不再读写、已有条目保留可单独清空。
  - +4 Rust 测试（暂停语义与恢复、关闭期间清空、离线回退尊重开关、配置三路径往返）。
- Tests: cargo test --lib 376 passed（+4）；fmt/clippy PASS；tsc PASS；vitest 70/70；build PASS
- Commit: 4f4941e / 8bda27b
- 未做：术语表（Task 10）— 原生 glossary 依赖 Provider 能力抽象（属暂缓的 AI Provider 工作），先评估再实施；全局快捷键（Task 11）— 需先决策 `@tauri-apps/plugin-global-shortcut` 依赖去留（实现快捷键还是移除依赖），见 Blockers。

### 2026-11-07 — Task 09 统一手动翻译入口（完成）
- 第一小步（7930da0，schema v7）：`translation_records.source` 列（clipboard/manual），既有行回填 clipboard；translate_text 写 manual、coordinator 写 clipboard；前端类型同步。+2 Rust 测试（v6→v7 升级回填、来源往返契约）。
- 第二小步（1b00ab3，UI 入口）：
  - 托盘菜单「手动翻译…」→ 独立小窗（440×480，hide 不销毁，输入与结果保留）。
  - `translate_text` 命令即手动翻译核心：同一引擎（fallback/熔断/8s 预算一致）、同一落盘管道；不触碰剪贴板监控开关。
  - 窗口内：目标语言下拉（默认配置值）、字符计数、Ctrl+Enter 提交；**超上限 5000 字符禁用按钮并明确提示**（计划「不无反馈截断」；后端截断兜底，truncated → 「超出部分未翻译」）；错误按契约文案展示；缓存命中标「本地缓存」。
  - popup 若在显示，由 popup_watch 前台轮询自然关闭，无需特判。
- Tests: vitest 70/70（+6：调用契约、空文本不请求、超限禁用、truncated 提示、错误契约、缓存标识）；tsc/build PASS；cargo fmt/clippy PASS（Rust 侧为窗口创建与菜单项，无可单测逻辑）
- Commit: 7930da0 / 1b00ab3
- 验收对照：同一核心业务 ✅；不改剪贴板开关 ✅；历史可识别来源 ✅；Fallback 一致 ✅；超长文本明确提示 ✅。**行为统一已补**：手动路径的缓存回退（见下条）。
- 第三小步（51fa5f4 / 2cb3457，缓存回退统一）：
  - `should_fall_back_to_cache` / `lookup_cached_result` / `CACHE_PROVIDER` 从 coordinator 私有实现上移到 `domain/cache.rs`，剪贴板路径与手动路径共用；coordinator 改为调用共享版。
  - `translate_text` 全部翻译源失败（AllProvidersFailed）时查 TranslationCache，命中返回 provider="cache"（前端「本地缓存」），未命中返回原始错误；缓存结果不再入落盘队列（条目来自上一次翻译，重复入队只会多一条重复历史）。
  - 顺带对齐：翻译源健康上报从「仅成功时」移到 outcome 分支之前（与 coordinator 时机一致），失败时也上报。
  - +4 Rust 测试（结果组装/truncated 透传/回声条目拒收/未命中）。命令层接线本身无单测（State mock 无先例），以接线代码审读兜底，如实记录。
- Tests: cargo test --lib 372 passed（+4）；fmt/clippy PASS；vitest 70/70；tsc/build PASS
- Commit: 7930da0 / 1b00ab3 / 51fa5f4 / 2cb3457

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
- `@tauri-apps/plugin-global-shortcut`（JS 包）已无使用方（快捷键走 Rust 端）：保留 / 移除待定，移除需同步 package-lock。不阻塞其他任务。
- 应用排除列表（Task 04）：当前剪贴板 API 在两个平台均无可靠的来源进程标识，需按计划先实现敏感过滤 + 暂停，应用排除标为平台能力待评估。

## Risks not yet verified
- Platform: Windows
  - Scenario: 全局快捷键的 OS 注册、冲突提示与三个动作的端到端行为
  - Required manual test: 启动应用，逐个按下 `CmdOrControl+Alt+T/M/P`；再用其他应用占用同一组合验证冲突提示与旧快捷键保持可用
- Platform: macOS
  - Scenario: 浮窗定位、剪贴板 changeCount 语义、打开外部 URL、全局快捷键注册
  - Required manual test: QT-001/002/004 的 macOS 实机验证（上轮审计遗留，EXTERNAL_VALIDATION_REQUIRED）
- Platform: Windows + macOS
  - Scenario: 真实翻译服务商的 contract tests（当前均为 mock 层验证）
  - Required manual test: 显式 opt-in 的真实凭证测试

## Next task
- Task 10 剩余：四个 Provider 的术语表效果 opt-in 真实凭证验证（DeepL/百度/有道/腾讯）；无代码阻塞项。
- 全局快捷键与术语表实机验证（Windows 优先）；macOS 未验证项继续保留。
- 阻塞项：Task 08 剩余（供应商目录分类/AI 类目）待要求 19 解禁。
