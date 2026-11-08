# 审计整改进度（PROGRESS）

状态图例：NOT_STARTED / IN_PROGRESS / FIXED_NEEDS_TEST / VERIFIED / EXTERNAL_VALIDATION_REQUIRED / BLOCKED

## 总览

| ID | 优先级 | 摘要 | 状态 | 备注 |
|---|---|---|---|---|
| QT-001 | P1 | macOS 光标位置固定 (0,0) | EXTERNAL_VALIDATION_REQUIRED | 修复+纯函数单测完成（f4878f1）；macOS 实机锚点验证待人工 |
| QT-002 | P1 | macOS 关闭浮窗后旧剪贴板内容再触发 | EXTERNAL_VALIDATION_REQUIRED | 修复+回归完成（bf9a689）；macOS Pasteboard 实机行为待人工 |
| QT-003 | P1 | loading 事件未受代际闸门约束 | VERIFIED | commit_popup_show 协议 + 6 个受控交错回归测试全绿（6a928d2） |
| QT-004 | P1 | open_url 固定调用 Windows explorer | VERIFIED | url parser + 白名单 + 平台分发 + 6 个校验测试（5cd0069）；macOS 打开行为 NOT RUN |
| QT-005 | P1 | 凭证验证实际执行翻译，与文案矛盾 | VERIFIED | DeepL 走用量接口 + trait 配额契约 + UI/README 如实化 + 4 个 HTTP 回归（443a74b）；真实服务商行为 NOT RUN（显式 opt-in 项） |
| QT-006 | P2 | zh-tw 目标映射丢失（DeepL/Tencent） | VERIFIED | 官方代码表核对 + 表驱动契约测试（95d9b50） |
| QT-007 | P2 | popup show() 失败仍报告成功 | VERIFIED | submit_show 上报 + Failed 契约（QT-003 测试锁定）；窗口 API 平台行为 NOT RUN（8c26e2a） |
| QT-008 | P2 | 存储清理失败未反映到健康状态 | VERIFIED | 触发器故障注入 3 测试（07fe73a） |
| QT-009 | P2 | 归一化破坏代码/日志结构 | VERIFIED | 角色分离 + build_request_text 9 用例（30332e1）；真实语料质量属人工评估 |
| QT-010 | P2 | 配置批次静默部分成功 | VERIFIED | allowlist + all-or-nothing + 副作用如实上报，8 测试（12cf8a1）；auto_start OS 行为 NOT RUN |
| QT-011 | P2 | 上游响应 body 进入日志 | VERIFIED | 哨兵测试 7 类状态码零泄漏（a52626b） |
| QT-012 | P2/待验证 | Google GET 长 URI 风险 | VERIFIED | 编码长度实测 + 414 快速失败护栏（2a625b7）；真实端点接受范围 EXTERNAL_VALIDATION_REQUIRED |
| QT-013 | P2/待验证 | ExpectedWrite 哈希匹配竞态 | VERIFIED | 基线代数协议 + 全时序回归（退化即复现，519011e）；真实剪贴板时序属人工项 |
| QT-014 | P2 | Release npm audit continue-on-error | VERIFIED | 最小权限 + 豁免门禁 + SHA pin（72b49c4）；门禁脚本本地双向验证 |

## 迭代日志

### 迭代 0（2026-10-27）— 基线

- 核对 git 状态与版本基线：HEAD `3df3d1e` / 0.8.0，与审计基线一致（见 BASELINE.md）。
- 逐条核对 QT-001/002/003/009 与当前源码，均成立（证据见 VERIFICATION_MATRIX.md）。
- 基线检查：npm ci PASS、npm test 33/33 PASS、npm run build PASS、cargo fmt --check PASS；cargo test --lib 与 clippy 后台运行中。

### 迭代 1（2026-10-27）— Phase 1 完成

- QT-003 `6a928d2`：commit_popup_show 提交协议（窗锁串行 + 窗锁内代际检查 + run_if_current 原子提交 loading）。6 个确定性交错回归测试。243→249 lib tests。
- QT-002 `bf9a689`：macOS NSPasteboard changeCount 作为剪贴板变化代数；无代数平台吸收语义改为「关闭就是关闭」（记录文本）；mock backend 支持 TextAndSeq；3 个 worker 回归测试。期间发现并修复测试脚本自身的设计死锁（pending 被持续替换导致永不触发）。
- QT-001 `f4878f1`：CGEventGetLocation（CG 全局显示空间，与 tao Monitor::position 同坐标系）+ 显示模式像素宽/CG 宽推导缩放比，全部线程安全 API（NSScreen 需要 MainThreadMarker，worker 线程不可用）。6 个纯函数几何测试全平台运行。tao/dpi/runtime-wry 源码考证记录于 DECISIONS.md。
- QT-004 `5cd0069`：url crate 解析 + http/https 白名单 + host 必填；平台分发 explorer/open/xdg-open；6 个校验测试。WHATWG 单斜杠语义按标准放行（测试曾误判为畸形输入）。
- QT-005 `443a74b`：DeepL /v2/usage 零配额校验（Authorization 头）；trait 新增 validation_uses_translation；腾讯/百度/有道如实声明；设置页提示 + README 中英修正；4 个手写 mock server HTTP 回归。

门禁状态：cargo lib tests 262 通过、fmt/clippy 干净、前端 33 tests + tsc + build 通过。

### 迭代 2（2026-10-27）— Phase 2 完成

- QT-007 `8c26e2a`：show() 错误如实上报（submit_show），Failed → runtime Degraded；set_position/set_focusable 保持 best-effort 并带稳定错误码。
- QT-008 `07fe73a`：HISTORY/CACHE_RETENTION_FAILED 纳入存储健康；BEFORE DELETE RAISE(ABORT) 触发器做确定性故障注入（预置超限旧行保证删除真实发生）。
- QT-006 `95d9b50`：DeepL zh-tw→ZH-HANT、腾讯 zh-tw→zh-TW + ko→kr（官方文档核对）；表驱动映射测试。
- QT-009 `30332e1`：原文/去重键/请求文本三角色分离；build_request_text 保守结构启发式。过程中发现重排 move 时误删 last_seq 同步导致 pending 无限替换的回归，被既有测试捕获并修复。
- QT-010 `12cf8a1`：配置键 allowlist（16 键）+ 类型/范围校验 + 整批 all-or-nothing + 副作用失败如实返回。
- 附带发现：`cargo clippy --lib --tests -- -D warnings` 在基线就有 13 个 test-target 告警（cache len/is_empty、测试代码 clone 等），不属于整改范围，记录于 BASELINE.md；审计要求的 clippy 门禁（lib 目标）全程绿。

### 迭代 3（2026-10-27）— Phase 3 完成

- QT-011 `a52626b`：错误分类不再携带响应体；白名单 error_code 提取（≤16 位字母数字）；3 个哨兵测试（伪造 Key/签名/用户原文 × 7 类状态码）。
- QT-013 `519011e`：ExpectedWrite 增加 baseline_seq，写入落地（代数前进）才消费；4 个 controller 单测 + 1 个 worker 全时序回归（共享代数 mock 精确控制落地时机；退化回纯哈希立即复现双重触发）。
- QT-012 `2a625b7`：reqwest 同源 form 编码精确计算 URL 长度，>7500 字节客户端快速失败 ProviderRejected(414)（Permanent，不熔断、继续 fallback）；5 个测量/边界测试。
- QT-014 `72b49c4`：顶层 contents:read + 仅构建 job 持 write；npm audit 高危门禁（6 项构建期传递依赖豁免、新增即红）；10 处 action 引用固定到不可变 SHA（GitHub API 解析）。

### 最终回归（2026-10-27）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS |
| `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`（审计原命令） | PASS |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | 297 passed / 0 failed（基线 237 → +60） |
| `npm test` | 33 passed / 0 failed |
| `npm run build` | PASS |
| `git diff --check` | PASS |
| 忽略测试扫描 | 无 `#[ignore]` / `.skip` |
| 版本一致性 | package.json / tauri.conf.json / Cargo.toml = 0.8.0 |
| Windows 平台构建 | 见 VERIFICATION_MATRIX（`npm run tauri build`） |
| npm audit | 留档 docs/implementation/npm-audit-baseline.json（6 高危全部构建期依赖，已入豁免清单） |
