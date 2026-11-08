# QuickTranslate 测试清单（Task 01 交付物）

> 建立目的：让后续每次修复都有可对照的测试依据。本文记录当前每个模块的测试覆盖范围与已识别缺口，作为 Task 02–06 工作的起点。
> 基线日期：2026-11-07 · 版本 0.9.0 · commit `b64b04e`

## 基线命令（全部实跑通过）

| 门禁 | 命令 | 基线结果 |
|---|---|---|
| Rust 单测 | `cargo test --manifest-path src-tauri/Cargo.toml --lib` | 347 passed / 0 failed |
| Rust 格式 | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS |
| Rust lint | `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` | PASS |

> 说明：`cargo clippy --lib --tests -- -D warnings` 当前有 16 个**既有** test 目标告警（测试代码 clone/常量断言等，0.8.0 基线时记录为 13 个，v0.9.0 期间又新增 3 个）。CI 门禁与仓库脚本的 clippy 均只针对 `--lib` 目标，不构成阻塞；批量清理属独立整理事项，不在功能任务中顺带处理。
| 前端测试 | `npm test`（Vitest） | 37 passed / 8 文件 |
| 前端类型 | `npx tsc --noEmit` | PASS |
| 前端构建 | `npm run build` | CI 覆盖（本地待跑） |

CI（`.github/workflows/ci.yml`）已覆盖：前端检查 × 3 平台、fmt × 3 平台、clippy 与单测 × Windows/macOS、release 构建验证 × 双平台。CI 不依赖真实 Provider 凭证。

> 进展更新：Task 04（+33）、Task 06（+19）、Task 09 来源类型（+2）、Task 09 缓存回退统一（+4）、缓存读写开关（+4）、全局快捷键（+4）、术语表数据层（+14，含语言对隔离）、DeepL 同步与缓存指纹（+7）、百度/有道控制台术语库（+3）、腾讯 TermRepoIDList（+2）、快捷键重试（+1）、评审整改（+7：DeepL 同步硬超时 +1、缓存作用域令牌 +2、手动路径敏感过滤 +4）、历史筛选增强（Rust +3：来源/语言对/翻译源过滤 2、筛选选项派生 1）、翻译风格（Rust +5：请求形态裁剪 2、脏值兜底 1、风格进缓存令牌 2）后 Rust 单测现为 **422 passed**；前端为 **100 / 18 文件**（+3：手动窗口敏感内容确认；+5：手动窗口目标语言时序 2 例、事件名契约 3 例；+4：FilterBar 筛选行契约）。

## Mock 注入能力（现状确认）

- **翻译 Provider 注入**：`domain/translator/mod.rs` 测试模块内已有 `MockProvider`（可编排 `Outcome::Ok/Timeout/...` 序列）、`SharedMock`（`Arc` 共享调用计数）与 `engine_with()` 构造器 —— 翻译引擎、fallback 链、熔断交互均可确定性驱动，无需真实网络。
- **HTTP 层**：QT-005 整改中引入手写 mock HTTP server 回归（凭证验证路径）；错误脱敏有哨兵测试（伪造 Key/签名/原文 × 7 类状态码）。
- **剪贴板**：`clipboard/worker.rs` 测试内 mock backend 支持 `TextAndSeq`（文本 + 序列号），可精确控制写入落地时机；`controller.rs` 有 `fast()`/`never_stops()` 测试构造器。
- **并发时序**：coordinator 测试用 `oneshot` channel + `wait_until` 轮询做受控交错，不依赖真实 sleep 时序。

## Rust 测试清单

### 翻译主链路

| 模块 | 数量 | 已覆盖 | 缺口 |
|---|---|---|---|
| `domain/translator/mod.rs` | 37 | fallback 链、超时重试、熔断交互、错误 Display 脱敏哨兵、语言映射表驱动、引擎统计、任务中止终结 fallback 链 | Provider contract tests 对真实 API 形状的覆盖（Task 03/14） |
| `domain/translator/health.rs` | 13 | 状态机流转、冷却阶梯（含长冷却）、半开单探测、Retry-After 边界 | 并发探测互斥已有；无待办 |
| `domain/translator/error_class.rs` | 9 | 错误分类表 | 无待办 |
| `domain/translator/policy.rs` | 10 | 请求策略、8s 预算、剩余预算校验 | 无待办 |
| `system/translation/coordinator.rs` | **22** | 代际闸门、迟到结果丢弃、install 拒绝并中止、窗锁串行、commit_popup_show 全协议、缓存回退判定、过滤门控开关语义、拦截内容暂存生命周期、上一次翻译保留 | 无待办（取消跨 fallback 切换由 translator 的 `aborted_task_never_continues_the_fallback_chain` 锁定，d3b3f97） |
| `commands/translate.rs` | **5** | snake_case 前端契约、手动路径过滤门（命中拦截且解释不回显原文、确认放行、开关尊重、普通文本放行） | 命令层端到端（需 AppState mock，现有测试覆盖纯逻辑门） |

### 剪贴板链路

| 模块 | 数量 | 已覆盖 | 缺口 |
|---|---|---|---|
| `system/clipboard/worker.rs` | 19 | 去重、latest-wins、序列号语义、空/短文本、暂停恢复、吸收语义、关闭不重触发 | 无待办（应用排除属平台能力待评估，见下文） |
| `domain/clipboard_filter.rs` | 25 | 12 类高置信度规则的逐条正反例、词边界、多行日志、规则优先级、解释文案不含原文 | 真实世界凭据样本的人工抽样复核（无法自动化） |
| `system/clipboard/controller.rs` | 12 | 暂停/恢复、在途请求管理 | 无待办 |
| `system/clipboard/supervisor.rs` | — | 连续失败退避、非自旋 | 无待办 |

### 数据与持久化

| 模块 | 数量 | 已覆盖 | 缺口 |
|---|---|---|---|
| `domain/cache.rs` | **24** | TTL、LRU、键构造、回声过滤、清空缓存仅删缓存条目、离线回退结果组装（provider=cache / truncated 透传 / 回声拒收 / 未命中）、读写开关（暂停不删除 / 关闭期间清空 / 回退尊重开关）、作用域令牌（维度顺序、术语上下文隔离、翻译风格位隔离） | 无待办 |
| `domain/glossary.rs` | **13** | 按语言对的 CRUD、语言对隔离、大小写不敏感重复拒绝、批内重复与批量原子性、CSV/JSON 导入导出（引号/逗号/换行/可选表头/其他语言对拒绝）、指纹（顺序无关、说明不参与、禁用为空） | Provider 应用（DeepL glossary 同步）与缓存键接入未实现 |
| `domain/history.rs` | **22** | query 分页（`rowid` 第二排序键）、LIKE 转义、中文子串检索、toggle_star/delete_by_id/clear_all、FIFO 只删最旧未收藏、参数钳制、starred 过滤、收藏持久性、来源类型（clipboard/manual）往返、来源/语言对/翻译源筛选（query/count 同构、正交组合、脏值容错、筛选选项派生）；万条规模分页/搜索索引与 500ms 预算、万条 FIFO | 无待办 |
| `system/persistence.rs` | 9 | 有界队列、丢弃策略、单 worker、入队失败诊断计数 | 关闭时 drain 语义已核验：channel 排空后退出（见 agent-progress Task 06），无显式退出等待——设计如此，非缺陷 |
| `infra/database.rs` | **6** | 损坏抢救、配置行恢复、v1→最新迁移升级保留记录、重复迁移幂等、版本行逐版本记账（当前 v8） | 无待办（任何新 schema 变更前先加迁移测试） |
| `domain/config.rs` | **16** | 描述符表完备性、批次 all-or-nothing、类型校验、布尔键三路径往返（含 cache_enabled/glossary_enabled/baidu_use_term_base）、控制台术语库键往返、translation_style 三值校验（未知值拒绝） | 无待办 |
| `infra/crypto.rs` | 12 | AES-GCM、掩码、错误处理 | 无待办 |

### 系统与运行时

| 模块 | 数量 | 已覆盖 |
|---|---|---|
| `runtime/status.rs` | 14 | 组件状态聚合、指纹去重 |
| `commands/system.rs` | 10 | open_url 白名单校验 |
| `system/tray.rs` | — | 托盘状态行组装 |
| `system/hotkeys.rs` | **4** | 动作清单唯一性、默认加速键非空且不重复、默认值贯通 AppConfig、初始注册状态 | Windows 实机已验证五个动作的 OS 注册与分发（2026-11-08）；冲突提示与 macOS 未验证 |
| `system/glossary_sync.rs` | **4** | 同步映射读写/冲突更新、按语言对删除、TSV 控制字符转义、pair key 构造 | DeepL glossary 真实 API 调用需 opt-in contract test；同步动作整体受 3s 硬超时（`deepl::GLOSSARY_SYNC_TIMEOUT`，慢速 mock 测试在 deepl.rs） |
| `util/backoff.rs` | 5 | 指数退避、抖动边界 |
| `error.rs` | — | URL 脱敏 |

## 前端测试清单（100 / 18 文件）

| 文件 | 覆盖 |
|---|---|
| `lib/__tests__/constants.test.ts` | Rust↔前端错误码契约（新增 Rust 错误码必须同步文案） |
| `lib/__tests__/events.test.ts` | Rust↔前端事件名契约（emit ↔ listen 全配对，kebab-case 约束） |
| `lib/__tests__/configSync.test.ts` | 外部真值合并规则（凭证/auto_start/在途键跳过）+ 快速连续写入的回滚守卫 |
| `lib/__tests__/hotkeys.test.ts` | 键盘事件 → 加速键（修饰键顺序、具名键、拒绝无效组合）+ 按平台格式化 + 动作/配置键映射 |
| `lib/__tests__/domUtils.test.ts` | 键盘目标/拖拽阻断判定 |
| `lib/__tests__/export.test.ts` | Markdown/HTML 导出格式化 |
| `stores/__tests__/historyStore.test.ts` | 历史 store 分页/搜索 |
| `windows/history/__tests__/HistoryChanged.test.tsx` | 实时刷新契约 |
| `windows/history/__tests__/FilterBar.test.tsx` | 筛选行契约：选项从库内数据派生、维度切换上抛完整筛选对象、一键清除、选项未到达时降级 |
| `windows/popup/__tests__/PopupActivation.test.tsx` | 激活时序契约 |
| `windows/popup/__tests__/TrafficLights.test.tsx` | 交通灯交互 |
| `windows/settings/__tests__/ToggleRow.test.tsx` | 设置开关行 |
| `windows/settings/__tests__/GlossaryTab.test.tsx` | 术语表按语言对加载、新增携带语言对/大小写、Provider 能力说明、百度开关、腾讯云术语库 ID 提交 |
| `windows/manual/__tests__/ManualTranslateWindow.test.tsx` | 手动窗口请求契约（参数下发、来源标识、空文本不发请求） |
| `windows/manual/__tests__/ManualSensitiveGate.test.tsx` | 敏感内容确认契约：命中弹确认框不进错误位、确认携带 force 重试、取消不再发起 |

## 已识别缺口汇总（映射后续任务）

1. ~~敏感内容过滤引擎整体缺失~~ → 已由 Task 04 完成（`domain/clipboard_filter.rs` + 入口门控 + 手动覆盖）。应用排除列表仍缺：当前剪贴板 API 无法可靠获得来源进程，需产品决策。
2. ~~`history.rs` 查询/收藏/清空/FIFO 直测缺失~~ → 已由 Task 06 完成（3 → 18 个测试 + 第二排序键修复）
3. ~~`database.rs` 迁移升级路径无测试~~ → 已由 Task 06 完成（3 → 6 个测试，v1 升级/幂等/记账）
4. 应用排除列表缺失（依赖平台来源进程能力，需先评估） → Task 04
5. ~~手动翻译入口缺失~~ → 已由 Task 09 完成（托盘菜单 + 独立小窗 + `translate_text`；schema v7 来源类型；手动路径缓存回退与剪贴板路径统一）
6. ~~全局快捷键：`@tauri-apps/plugin-global-shortcut` 已声明依赖但未接线~~ → 首次实现完成（Rust 端 `tauri-plugin-global-shortcut` + `system/hotkeys.rs`，五个动作复用既有入口，设置页可录制/禁用/查看注册状态）；Windows 实机已验证 OS 注册与五个动作（2026-11-08）；冲突提示与 macOS 未验证；JS 包已移除
7. ~~缓存禁用/清空的独立入口待核验~~ → 清空入口已由 Task 06 完成（独立命令 + 设置页操作行）；读写开关已补齐（`cache_enabled`，默认开启，关闭只暂停不删除）
8. 术语表：本地数据层 + DeepL 同步 + 百度/有道/腾讯控制台引用 + 管理 UI 已完成（mock 层验证；Windows 实机已验证术语表界面的新增/删除与落盘，2026-11-08）；真实服务商调用与术语匹配效果需 opt-in contract test

## 约定

- 不为测试而将真实 API Key 放入 CI（现状已满足，保持）。
- 新增功能必须带单元测试；并发行为用受控交错（channel/屏障），不用真实 sleep 断言时序。
- 每完成一个任务更新本清单对应的「缺口」行，并同步 `docs/agent-progress.md`。
