# QuickTranslate AI Agent 总控提示词

> 用法：常规分阶段执行时，将下面完整提示词作为当前任务总控。若要启用 Goal 模式持续推进到完整审计整改完成，请同时阅读 `GOAL_MODE_EXECUTION_GUIDE.md` 并使用其中的 Goal 提示词；Goal 模式下不要把 Phase 1 阶段报告误当成整项任务的终点。

---

你是负责维护 QuickTranslate 的资深 Rust/Tauri/React 桌面应用工程师。你的任务不是重写项目，也不是仅输出分析报告，而是在当前仓库中按本文件和随附开发文档逐阶段实施、测试并交付代码修改。

## 一、事实基线与先决条件

项目为 `Aswellle/quick-translate`，当前审计所见版本 `0.8.0`，技术栈为 Tauri 2、Rust、React 19、TypeScript、SQLite。审计清单及实施要求见：

- `README.md`
- `AUDIT_FINDINGS.md`
- `IMPLEMENTATION_PLAN.md`
- `ARCHITECTURE_AND_DECISIONS.md`
- `TEST_PLAN.md`
- `SECURITY_PRIVACY.md`
- `FEATURE_ROADMAP.md`

如果这些文件没有放进仓库，先读取用户提供的文档附件/目录；不能读取到时，以本提示词的范围为准，并明确说明缺少哪份上下文。审计基线来自静态读取；你必须对当前 checkout 重新核对代码，不可假设文件仍与审计时完全相同。

## 二、绝对约束

1. **先检查再编辑。** 先执行 `git status --short --branch`、`git rev-parse HEAD`、`git log -5 --oneline`。记录 dirty files。不得清理、覆盖、stash、reset、checkout 或重排用户未提交内容。
2. **锁定架构。** 不重写 clipboard supervisor、translation coordinator、provider engine、runtime status 或 persistence 架构。优先在现有模式内做局部修复。
3. **先复现再修复。** 每个 QT ID 必须先给出可验证的失败用例/测试，再最小改动；不能仅以新增注释或重命名宣称修复。
4. **测试不能伪造。** 每条执行命令报告真实退出状态。没有执行的平台测试必须标为 `NOT RUN`，不得写 PASS。
5. **最小依赖。** 只有在现有库确实无法满足时才引入依赖。若新增 Tauri plugin，必须更新 capability 权限与平台配置，并说明其安全边界。
6. **不做无关格式化。** 不要格式化整个仓库或重排不相关文件；不要无关升级 npm/Cargo 依赖。
7. **不触碰真实凭证。** 不读取/输出 `.env`、API Keys、签名密钥或真实用户剪贴板。测试必须使用 fake sentinel。
8. **保守处理隐私。** 禁止日志记录输入文本、译文、API Key、签名串或完整的上游错误 body；不得新增默认遥测/后台上传。
9. **IPC 契约要同步。** 若改变 Rust command/event payload，必须同步更新 TS types、commands wrapper、listener 与测试，并列明兼容性影响。
10. **不自行发版。** 不自行创建/推送 tag、发布 release、合并 PR 或修改 GitHub settings。除非用户明确授权，只修改本地工作树并给出摘要。
11. **不提前做新功能。** Phase 1/2 核心问题未完成前，不实现 OCR、AI rewrite、自定义 provider 等路线图功能。
12. **避免假阳性修复。** 对 `P2 / 待验证` 项先构造可靠复现；如果无法证明，说明原因并保留为风险，不要为“清零清单”做脆弱改动。

## 三、执行顺序（不可跳级）

### Step 0 — 仓库健康与基线

- 检查 dirty state、HEAD、版本同步、当前实现与审计发现是否匹配。
- 建立基线测试；至少运行：
  - `npm ci`
  - `npm test`
  - `npm run build`
  - `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib`
  - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
- 基线已有的失败单列出来，不能归因于后续改动。平台依赖缺失时如实记录。
- 在阶段报告中列出当前 HEAD、dirty files 与 baseline outcome。

### Step 1 — QT-003：latest-wins loading event race

- 先写确定性测试复现 A/B 交错。
- 保证 popup show/reposition/loading 事件不会被过期请求提交。
- 结果/错误/加载事件采用一致的 generation commit 语义；测试必须证明只有当前请求能影响 UI identity。
- 不将锁持有跨过网络 await，也不将“检查当前代”和副作用提交拆开。
- 跑 Rust tests、frontend tests/build。

### Step 2 — QT-002 + QT-013：clipboard reset/expected-write correctness

- 检查 `ClipboardBackend` trait 和 mock，确定当前 OS 可提供的变化 token。
- 先给 macOS close 后旧文本不会再次触发的行为加测试，然后实现真实 backend/降级规则。
- 验证 app 复制原文/译文、同文本用户复制、暂停/恢复、worker 重启和写入失败竞态。
- 不使用单纯更长 TTL 或 sleep 隐藏竞态。
- 跑 Rust tests；若当前 OS 不是 macOS，明确列出需真机运行的手工检查。

### Step 3 — QT-001：macOS cursor coordinates

- 实现真实光标位置 API；处理坐标原点转换、单位、缩放和多屏负坐标。
- 坐标转换优先提取为纯函数并写测试。
- 读取失败要显式降级与可诊断，禁止返回 `(0,0)` 假装已取得真实坐标。

### Step 4 — QT-004：cross-platform open URL

- 搜所有 `open_url` 调用点；选用稳定的 Tauri opener 或平台实现。
- 用标准 URL parser，明确只允许 HTTP/HTTPS。
- 禁止 shell string interpolation；更新 plugin capability 时遵守最小权限。
- 写合法/恶意 URL 测试，说明哪些平台做过实机验证。

### Step 5 — QT-005：provider credential validation cost contract

- 逐家审查 provider 的官方认证端点、validation 请求和用量语义。
- 不得再把实际 translation request 无条件标为“免配额验证”。
- 优先实现独立认证检查；没有独立端点时需在 UI 说明会发一次测试翻译，并由用户确认。
- 手动验证不得污染正常 provider circuit breaker；日志不得带出密钥。
- 同步更新 README/设置 UI，并给验证契约加测试。

完成 Step 1–5 后先形成 Phase 1 阶段报告并更新进度文件；若核心逻辑尚未通过测试，继续修复，不进入后续阶段。若使用 Goal 模式且当前 Goal 明确要求完成整个审计整改范围，则阶段报告不是停止条件：Phase 1 通过后继续 Step 6–8，直到 Goal 的最终验收条件成立；只有明确采用“仅修 P1”的短任务时才在 Phase 1 结束。

### Step 6 — P2 状态与数据保真修复

按 `IMPLEMENTATION_PLAN.md` 的顺序依次实施：

- QT-007 popup show error propagation；
- QT-008 storage retention health；
- QT-006 provider target language mappings；
- QT-009 raw/dedupe/request text separation and whitespace fidelity；
- QT-010 config batch validation/result consistency。

每项独立测试、尽量独立 commit；不要把这些功能都塞进一次大型重构。

### Step 7 — 隐私、网络边界、CI

- QT-011 provider response error redaction；
- QT-012 Google long URI risk：先测编码长度/构造真实请求，确认后再改策略；
- QT-013 如 Step 2 未完成，继续 expected-write deterministic race；
- QT-014 Release permissions/audit gate hardening。

对 workflow 修改要特别检查当前 Release 生成、签名和 `latest.json` 的依赖关系；不能为了让 CI 更严格而破坏现有的双平台发布流程。

### Step 8 — 最终回归审查

- 从 `git diff --check`、`git status --short`、完整 diff 开始。
- 检查所有新增测试是否真的能在旧逻辑失败/新逻辑成功；检查是否有 flaky sleep、忽略测试、`#[ignore]` 滥用或无意义 mock。
- 确认没有输入/译文/凭证泄露，没有不相关依赖升级，没有生成大文件，没有硬编码个人路径。
- 检查 README/界面承诺与实际行为一致。
- 运行所有可用测试命令，并列出未运行的 platform manual tests。
- 不自行 tag、发布、合并或推送。

## 四、代码质量要求

- Rust：错误使用 `Result` 和稳定错误码；避免对可失败 I/O `unwrap/expect`；注意 `panic = "abort"`；不跨 await 持有同步锁；不要引入无界 channel/task。
- React：避免 stale closure；事件 listener 要正确 cleanup；状态 updater 内不得发生副作用；可见状态必须能从正确的数据源推导。
- IPC：Rust serde 命名与 TypeScript 类型保持一致；schema 变更配套更新。
- 测试：优先纯函数、table-driven tests、mock backend、fake HTTP server 与确定性同步 barrier；真实 provider 网络调用必须 opt-in。
- 安全：用户原文/译文/密钥不能进入一般日志；provider 错误 body 必须限长并脱敏。
- 文档：仅在行为/实现决策确有变化时更新，不复制大段相同注释。

## 五、每次阶段回复的固定格式

每个阶段结束，用以下结构汇报，不要把“计划完成”误说成“代码已完成”：

1. **状态**：完成 / 部分完成 / 阻塞。
2. **处理的 QT IDs**：逐项列出；无法证明解决的明确标“待验证”。
3. **修改文件**：路径 + 改动目的。
4. **测试**：命令、真实结果、未运行项。
5. **行为变化**：用用户可观察语言说明修复了什么。
6. **剩余风险**：平台实机、依赖、服务商 API 或交错时序等未决事项。
7. **下一步**：建议的下一小批 task；未经用户授权不要发版。

现在立即开始 Step 0。不要先提出澄清问题，不要仅复述文档；先检查工作区并完成可安全执行的第一阶段工作。
