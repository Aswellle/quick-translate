# QuickTranslate：AI Agent 实施文档总入口

> **目的**：将当前代码审计结论转化为 Codex / Claude Code 可以分阶段、可测试、低返工地实施的任务包。
>
> **仓库**：[`Aswellle/quick-translate`](https://github.com/Aswellle/quick-translate)  
> **审计基线**：默认分支 `main`；本次读取的最新提交为 `3df3d1e3`（版本升级到 `0.8.0`，提交日期 2026-10-22）。  
> **审计方式**：GitHub Connector 读取当前仓库 README、包配置、CI/release workflow，以及 clipboard、translation coordinator、provider、config、crypto、database、persistence、popup 等核心源文件并进行静态追踪。  
> **限制**：本轮未在真实 Windows/macOS 设备上运行应用，也未执行仓库的本地构建/测试。本文将“从源代码可确认的行为”和“需测试进一步证明的风险”分开描述，不把静态判断表述为已实机复现。

## 推荐阅读顺序

1. [`AUDIT_FINDINGS.md`](./AUDIT_FINDINGS.md) —— 缺陷清单、证据、影响、修复方向与复现方式。
2. [`IMPLEMENTATION_PLAN.md`](./IMPLEMENTATION_PLAN.md) —— 分阶段的具体实施次序、改动边界、依赖关系和交付条件。
3. [`ARCHITECTURE_AND_DECISIONS.md`](./ARCHITECTURE_AND_DECISIONS.md) —— 当前代码结构、必须保持的不变量及建议架构决策。
4. [`TEST_PLAN.md`](./TEST_PLAN.md) —— 单元测试、跨平台验收、并发竞态和发布门禁。
5. [`SECURITY_PRIVACY.md`](./SECURITY_PRIVACY.md) —— 剪贴板/凭证/日志/网络/发布供应链风险。
6. [`FEATURE_ROADMAP.md`](./FEATURE_ROADMAP.md) —— 正确性之后的功能扩展路线。
7. [`AGENT_MASTER_PROMPT.md`](./AGENT_MASTER_PROMPT.md) —— 可直接复制给 AI Agent 的总控提示词。

## 当前结论

本项目已有较成熟的基础工程机制：Rust 错误分类与翻译源熔断、8 秒总体请求预算、latest-wins 翻译代际、剪贴板 worker/supervisor、限长持久化队列、SQLite schema migration、API 凭证本地加密、跨平台 CI。**不建议以“重写架构”的方式改造**；应优先修复跨平台行为、并发事件次序、错误上报准确性和配置事务语义。

按优先级建议：

- **P1（优先完成）**：QT-001 macOS 光标定位；QT-002 macOS 关闭浮窗后旧剪贴板内容可能再次触发；QT-003 旧 loading 事件可能覆盖新请求；QT-004 `open_url` 硬编码 `explorer`；QT-005 凭证验证调用真实翻译，与“不耗配额”的文案矛盾。
- **P2（紧接着）**：QT-006 繁体中文目标映射丢失；QT-007 popup 显示失败被忽略；QT-008 存储限额清理失败仍报告 healthy；QT-009 文本归一化破坏代码/日志格式；QT-010 配置批量写入可能静默部分成功；QT-011 日志可能带入上游响应 body；QT-012 Google GET 长 URL 风险；QT-013 clipboard expected-write 残余竞态；QT-014 release 安全门禁强化。

## 使用约定

- Agent 必须先检查 `git status`、当前分支、HEAD 和本地未提交修改；不得覆盖或清理用户现有工作。
- 以仓库现有 Rust / Tauri / React / TypeScript 技术栈为准。先复现，再修复；不要先做大规模重构。
- 每个缺陷独立加回归测试，再进行最小修复；完成一个阶段后运行该阶段要求的测试，再进入下一阶段。
- 没有测试结果、设备日志或 CI 证据时，不得在报告中声称“已验证通过”。
- 不得把用户剪贴板原文、译文、API Key、签名串或未经脱敏的服务端响应写入普通日志、遥测或测试 fixture。
- 新功能不得抢在 P1 修复之前进入主实现；可先记录在路线图，不得顺手增加不相关依赖。

## 源码导航

| 子系统 | 主要入口 |
|---|---|
| 剪贴板监控 | `src-tauri/src/system/clipboard/mod.rs`、`controller.rs`、`worker.rs`、`supervisor.rs`、`backend.rs` |
| 翻译请求编排 | `src-tauri/src/system/translation/coordinator.rs`、`request.rs`、`src-tauri/src/system/translation_flow.rs` |
| Provider 引擎 | `src-tauri/src/domain/translator/mod.rs`、`policy.rs`、`error_class.rs`、`health.rs`，以及各 provider 文件 |
| 配置 / 凭证 | `src-tauri/src/commands/config.rs`、`src-tauri/src/domain/config.rs`、`src-tauri/src/infra/crypto.rs` |
| 数据库 / 历史 / 缓存 | `src-tauri/src/infra/database.rs`、`src-tauri/src/domain/history.rs`、`src-tauri/src/domain/cache.rs`、`src-tauri/src/system/persistence.rs` |
| 浮窗 UI / 几何 | `src/windows/popup/PopupWindow.tsx`、`src/lib/domUtils.ts`、`src/hooks/useTauriEvent.ts`、`src-tauri/src/system/popup_geometry.rs` |
| Tauri 命令 / 权限 | `src-tauri/src/commands/system.rs`、`src-tauri/src/capabilities/default.json`、`src-tauri/tauri.conf.json` |
| CI / Release | `.github/workflows/ci.yml`、`.github/workflows/release.yml` |


- `GOAL_MODE_EXECUTION_GUIDE.md`：Goal 模式持续执行指南，含完整 Goal 提示词、进度文件规范、暂停/恢复提示词与完成验收门槛。
