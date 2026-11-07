# QuickTranslate 测试清单（Task 01 交付物）

> 建立目的：让后续每次修复都有可对照的测试依据。本文记录当前每个模块的测试覆盖范围与已识别缺口，作为 Task 02–06 工作的起点。
> 基线日期：2026-11-07 · 版本 0.9.0 · commit `b64b04e`

## 基线命令（全部实跑通过）

| 门禁 | 命令 | 基线结果 |
|---|---|---|
| Rust 单测 | `cargo test --manifest-path src-tauri/Cargo.toml --lib` | 313 passed / 0 failed |
| Rust 格式 | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS |
| Rust lint | `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` | PASS |
| 前端测试 | `npm test`（Vitest） | 37 passed / 8 文件 |
| 前端类型 | `npx tsc --noEmit` | PASS |
| 前端构建 | `npm run build` | CI 覆盖（本地待跑） |

CI（`.github/workflows/ci.yml`）已覆盖：前端检查 × 3 平台、fmt × 3 平台、clippy 与单测 × Windows/macOS、release 构建验证 × 双平台。CI 不依赖真实 Provider 凭证。

## Mock 注入能力（现状确认）

- **翻译 Provider 注入**：`domain/translator/mod.rs` 测试模块内已有 `MockProvider`（可编排 `Outcome::Ok/Timeout/...` 序列）、`SharedMock`（`Arc` 共享调用计数）与 `engine_with()` 构造器 —— 翻译引擎、fallback 链、熔断交互均可确定性驱动，无需真实网络。
- **HTTP 层**：QT-005 整改中引入手写 mock HTTP server 回归（凭证验证路径）；错误脱敏有哨兵测试（伪造 Key/签名/原文 × 7 类状态码）。
- **剪贴板**：`clipboard/worker.rs` 测试内 mock backend 支持 `TextAndSeq`（文本 + 序列号），可精确控制写入落地时机；`controller.rs` 有 `fast()`/`never_stops()` 测试构造器。
- **并发时序**：coordinator 测试用 `oneshot` channel + `wait_until` 轮询做受控交错，不依赖真实 sleep 时序。

## Rust 测试清单

### 翻译主链路

| 模块 | 数量 | 已覆盖 | 缺口 |
|---|---|---|---|
| `domain/translator/mod.rs` | 36 | fallback 链、超时重试、熔断交互、错误 Display 脱敏哨兵、语言映射表驱动、引擎统计 | Provider contract tests 对真实 API 形状的覆盖（Task 03/14） |
| `domain/translator/health.rs` | 13 | 状态机流转、冷却阶梯（含长冷却）、半开单探测、Retry-After 边界 | 并发探测互斥已有；无待办 |
| `domain/translator/error_class.rs` | 9 | 错误分类表 | 无待办 |
| `domain/translator/policy.rs` | 10 | 请求策略、8s 预算、剩余预算校验 | 无待办 |
| `system/translation/coordinator.rs` | 14 | 代际闸门、迟到结果丢弃、install 拒绝并中止、窗锁串行、commit_popup_show 全协议、缓存回退判定 | 取消发生在引擎 fallback 切换中的端到端断言（Task 02 核验项） |

### 剪贴板链路

| 模块 | 数量 | 已覆盖 | 缺口 |
|---|---|---|---|
| `system/clipboard/worker.rs` | 19 | 去重、latest-wins、序列号语义、空/短文本、暂停恢复、吸收语义、关闭不重触发 | **敏感内容过滤（缺失，Task 04）**；应用排除（缺失，Task 04，平台能力待评估） |
| `system/clipboard/controller.rs` | 12 | 暂停/恢复、在途请求管理 | 无待办 |
| `system/clipboard/supervisor.rs` | — | 连续失败退避、非自旋 | 无待办 |

### 数据与持久化

| 模块 | 数量 | 已覆盖 | 缺口 |
|---|---|---|---|
| `domain/cache.rs` | 12 | TTL、LRU、键构造、回声过滤 | 缓存开关禁用写入路径（Task 06 核验） |
| `domain/history.rs` | **3** | 仅 stats 参数钳制（days 边界、区间倒置） | **query 分页/搜索转义、toggle_star、clear_all、delete_by_id、enforce_limit FIFO 均无直测（Task 06 补）** |
| `system/persistence.rs` | 8 | 有界队列、丢弃策略、单 worker | 关闭时 drain 生命周期（Task 06 核验） |
| `infra/database.rs` | **3** | 损坏抢救、配置行恢复 | **v1→v6 迁移升级路径无测试（Task 06 补；schema 变更前必须有）** |
| `domain/config.rs` | 13 | 描述符表完备性、批次 all-or-nothing、类型校验 | 无待办 |
| `infra/crypto.rs` | 12 | AES-GCM、掩码、错误处理 | 无待办 |

### 系统与运行时

| 模块 | 数量 | 已覆盖 |
|---|---|---|
| `runtime/status.rs` | 14 | 组件状态聚合、指纹去重 |
| `commands/system.rs` | 10 | open_url 白名单校验 |
| `system/tray.rs` | — | 托盘状态行组装 |
| `util/backoff.rs` | 5 | 指数退避、抖动边界 |
| `error.rs` | — | URL 脱敏 |

## 前端测试清单（37 / 8 文件）

| 文件 | 覆盖 |
|---|---|
| `lib/__tests__/constants.test.ts` | Rust↔前端错误码契约（新增 Rust 错误码必须同步文案） |
| `lib/__tests__/domUtils.test.ts` | 键盘目标/拖拽阻断判定 |
| `lib/__tests__/export.test.ts` | Markdown/HTML 导出格式化 |
| `stores/__tests__/historyStore.test.ts` | 历史 store 分页/搜索 |
| `windows/history/__tests__/HistoryChanged.test.tsx` | 实时刷新契约 |
| `windows/popup/__tests__/PopupActivation.test.tsx` | 激活时序契约 |
| `windows/popup/__tests__/TrafficLights.test.tsx` | 交通灯交互 |
| `windows/settings/__tests__/ToggleRow.test.tsx` | 设置开关行 |

## 已识别缺口汇总（映射后续任务）

1. **敏感内容过滤引擎整体缺失** → Task 04（优先级最高的功能缺口：过滤必须发生在任何网络请求之前）
2. `history.rs` 查询/收藏/清空/FIFO 直测缺失 → Task 06
3. `database.rs` 迁移升级路径无测试 → Task 06（任何新 schema 变更的前置条件）
4. 应用排除列表缺失（依赖平台来源进程能力，需先评估） → Task 04
5. 手动翻译入口缺失 → Task 09
6. 全局快捷键：`@tauri-apps/plugin-global-shortcut` 已声明依赖但未接线 → Task 11（需先决策依赖去留）
7. 缓存禁用/清空的独立入口待核验 → Task 06

## 约定

- 不为测试而将真实 API Key 放入 CI（现状已满足，保持）。
- 新增功能必须带单元测试；并发行为用受控交错（channel/屏障），不用真实 sleep 断言时序。
- 每完成一个任务更新本清单对应的「缺口」行，并同步 `docs/agent-progress.md`。
