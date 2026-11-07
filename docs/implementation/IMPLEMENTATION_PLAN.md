# QuickTranslate 分阶段实施计划

## 执行总原则

- 先可靠复现、再改代码；一条缺陷一个小范围变更，不做一次性“大重构”。
- 每个阶段结束都要交付：改动摘要、文件清单、测试命令与真实结果、未解决项、是否需要人工实机验证。
- 不允许用“代码看起来正确”代替验证。若因本地 OS 不支持而不能运行平台测试，要把它标为 `NOT RUN`，并给出需要在哪个平台执行的手工步骤。
- 不在第一轮同时加入 OCR、AI 重写、在线同步等新功能。

## Phase 0 — 安全起步与建立当前基线

### 目标
保护用户本地工作，确认 Agent 使用的代码确实与当前审计基线一致。

### 操作步骤

1. 执行 `git status --short --branch`、`git rev-parse HEAD`、`git log -5 --oneline`；记录是否存在脏工作区和当前分支。
2. 若存在用户未提交修改，禁止 stash、reset、checkout 覆盖或自动格式化这些文件。先明确把审计范围限制到可安全编辑的代码。
3. 确认 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 版本一致；审计记录基线是 `0.8.0`。如果实际 HEAD 已变，不要硬套发现，重新检查对应源函数。
4. 运行现有基线检查：
   - `npm ci`
   - `npm test`
   - `npm run build`
   - `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
   - `cargo test --manifest-path src-tauri/Cargo.toml --lib`
   - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
5. 记录当前失败/环境缺项；不能把基线失败归因于新改动。若缺少 Rust target、系统 SDK、C/C++ 构建工具或网络，记明错误并尽量执行不依赖缺项的检查。

### 交付
`docs/audit/BASELINE.md` 或 Agent 工作摘要：HEAD、dirty files、命令、结果、未运行平台项。

## Phase 1 — P1 核心正确性修复

此阶段推荐依次独立实施 QT-003、QT-002、QT-001、QT-004、QT-005。顺序考虑并发 bug 易于用纯逻辑快速锁定；平台功能可分开验证。每项独立测试/提交。

### 1.1 QT-003 loading generation race

**预计涉及**：
- `src-tauri/src/system/translation/coordinator.rs`
- `src-tauri/src/system/translation_flow.rs`
- `src/windows/popup/PopupWindow.tsx`（仅在事件契约/第二道保护确实需要时）

**任务**：
1. 先写能控制异步交错顺序的回归测试：A 已生成 request id 但卡在配置 await；B 成为当前请求且 emit loading；A 继续。测试应证明当前实现失败或能以 mock 复现事件顺序。
2. 设计 generation gate 的提交范围：显示/重定位 popup 和 emit loading 都不能被过期请求改变；不要仅在创建翻译 task 时检查 generation。
3. 将 loading/result/error 的“当前请求”语义统一，避免检查后再 await 的竞态。
4. 增加 A/B 交错与快速连复制压力测试；确保锁不跨网络 await。
5. 运行 Rust 单测与前端测试。

**完成标准**：任何旧 request 无法改变当前 UI request id、窗口定位或 loading/result/error 状态；仅最新请求的成功/失败能到 UI。

### 1.2 QT-002 macOS clipboard backend token

**预计涉及**：
- `src-tauri/src/system/clipboard/backend.rs`
- `src-tauri/src/system/clipboard/mod.rs`
- `src-tauri/src/system/clipboard/worker.rs`
- `src-tauri/src/system/clipboard/controller.rs`

**任务**：
1. 在 `ClipboardBackend` contract 层表达平台可用的剪贴板变化 token；检查当前 trait/mock 实现，不要在 worker 里新增一组平台 `cfg` 特例。
2. 为 macOS 设计可信的 clipboard change count；若系统 API 在当前实现方式中不能访问，说明替代策略及相同文本再复制的限制。
3. 保持 hide/reset 的语义为“吸收当前已有内容”，不要简单把 `last_text=None` 作为默认回退。
4. 为当前未变化、相同文本重新复制、应用复制原文/译文、用户快速复制新文本、写入失败、暂停恢复、worker 重启写 mock tests。
5. 在 macOS 实机手测；不可实测时在交付报告标出 `NOT RUN`，不宣称通过。

**完成标准**：关闭 popup 不会自行重新弹；手工再复制遵从明确行为；Windows 不回归。

### 1.3 QT-001 macOS cursor coordinate

**预计涉及**：`clipboard/mod.rs`、`system/translation_flow.rs` 和必要的平台依赖配置。

**任务**：
1. 实现真正的 macOS 全局鼠标坐标获取，不保留 `(0,0)` 假成功。
2. 将坐标原点/单位换算收敛到纯函数，单独测试；不要把 NSWindow/AppKit 主线程限制带进剪贴板 worker。
3. 对监视器/工作区负坐标、DPI/scaling、屏幕边缘做几何测试。
4. 如果新加依赖，确认对 macOS deployment target、许可和构建产物的影响；避免随意引入大依赖。

**完成标准**：macOS 真机在多个屏幕位置的浮窗锚点正确；无法读坐标时可诊断且合理降级。

### 1.4 QT-004 cross-platform open URL

**预计涉及**：`src-tauri/src/commands/system.rs`、`src-tauri/Cargo.toml`、`src-tauri/capabilities/default.json`，以及需要修改的前端 command wrapper。

**任务**：
1. 先搜 `open_url` 的所有调用点与现有 Tauri 插件。
2. 选择 Tauri opener 或经过安全边界审查的平台实现；不要执行拼接 shell 字符串。
3. 使用标准 URL parser + 明确的 `http/https` scheme allowlist。
4. 若新增 Tauri plugin，核对 capability permission 最小化与平台兼容性。
5. 对合法、不合法和恶意输入做纯逻辑单测；在 Windows/macOS 验证系统默认浏览器行为。

**完成标准**：两个支持平台的外链打开正常；非法 scheme 无任何执行副作用。

### 1.5 QT-005 凭证验证契约

**预计涉及**：各 `domain/translator/*` provider、`domain/translator/mod.rs`、设置页及 README。

**任务**：
1. 给每个 provider 列明当前验证请求：端点、HTTP method、是否真实翻译、潜在配额/速率限制影响；先审查官方 API 文档再决定实现。
2. 能用账户/认证端点校验时，独立实现 credentials validation，不要进入生产翻译 fallback 链。
3. 对没有独立低成本校验途径的 provider，UI 必须说明将执行一条短测试翻译，或让用户明确同意再发请求。
4. 不要承诺“零配额”，除非端点及计费规则可以支撑该保证。
5. 确认手动验证不污染健康熔断状态，也不把密钥/请求签名写入日志。

**完成标准**：每家服务商的验证行为与 UI 文案一致，错误类别准确，测试可判定是否调用 translate。

## Phase 2 — P2 状态准确性与数据保真

### 2.1 QT-007 popup show error propagation

- 明确 `show_popup_loading()` 的成功/失败契约。
- 不忽略 `show()` 的错误；区分“请求已提交”和“窗口可见”的保证。
- 对 window 操作抽象出足够小的测试 seam，不要把整个 Tauri 运行时 mock 化。
- 断言 runtime degraded/healthy 与真正结果一致。

### 2.2 QT-008 persistence retention health

- 清理失败必须转成 storage health code。
- 评估 history 与 cache cleanup 的 retry policy；不能无限重试。
- 成功恢复后应清除 degraded 状态。
- 注入失败的测试应验证运行时 status、日志字段及翻译主链路非阻塞。

### 2.3 QT-006 target language mappings

- 对照当前各 provider 的官方语言代码，建立测试表而非散落在 `match` 中但无验证。
- 保持 `zh` / `zh-tw` 的含义区别；如 provider 不支持，不应静默改目标语种。
- 检查 fallback 候选是否需要考虑目标语种能力。
- 加全映射单测与请求 payload 契约测试。

### 2.4 QT-009 separate raw text, normalized key, request text

- 引入最小必要的数据类型/字段，至少区别：原始文本、dedupe key、请求文本和截断标志。
- 不可让 `normalize_text` 同时决定日志/历史保存的文本表示。
- 保持 clipboard debounce/dedup 可稳定运行；不得用纯文本相同判断覆盖掉用户再次复制的语义。
- 加普通文本、代码、日志、JSON/YAML、表格、Unicode、CRLF 的回归套件。
- 若结构化文本识别启发式较复杂，先实现保守规则并记录已知限制，不要在此阶段引入大型 NLP/AI 依赖。

### 2.5 QT-010 config batch result consistency

- 用 allowlist + typed validators 在持久化前完整验证。
- 定义 all-or-nothing 或明确 partial result；不能吞掉错误后返回无条件 `Ok(())`。
- 检查数据库事务、内存配置、provider 实例、自启动和 event side effect 的顺序。
- 保持 masked credential 的 no-op 保护。
- 在设置 UI 展示准确的保存错误/部分成功详情。

## Phase 3 — 隐私与网络健壮性

### 3.1 QT-011 redacted provider errors

- 为用户显示错误和 tracing 日志建立独立格式化逻辑。
- response body 设置上限，提取可用的 provider error code，不把全文直接放到错误文本。
- 用 fake secrets fixture 测试所有可见日志、diagnostic bundle 和错误 UI。

### 3.2 QT-013 expected-write race

- 使用 mock backend 复现 copy original（剪贴板原文已相同）及 app/user 同时写入。
- 再选用 `(hash, sequence/change token, deadline)` 或统一 writer 协议；先测试再改 controller。
- 如果短时间内无法完整消除并发 ambiguity，记录降级策略与测试边界，而不是用更长 TTL 吞掉用户事件。

### 3.3 QT-012 Google long URL test

- 验证请求 URI 的实际编码长度和目标端点接受范围。
- 只在实证后确定改用阈值、错误提示或分段；不要在无语义定义时直接按固定字符数拆段。

### 3.4 QT-014 CI release hardening

- 分离 build/test/release/manifest job 权限。
- 设置高危 audit fail gate 与经过审计的豁免文件。
- 评估 pin actions SHA、Cargo/npm 依赖审计和 SBOM。
- 验证 latest.json、`.sig`、Windows/MSI/NSIS、macOS `.dmg` release 流程仍一致。

## Phase 4 — UX、可诊断性与功能路线

该阶段必须等 Phase 1 完成。先实现不改核心架构的可解释性与隐私控制，再考虑新的翻译能力。

建议切分：

1. **诊断导出**：用户主动生成、敏感文本/key 默认脱敏的本地报告；包含 app 版本、OS、provider 状态、worker 重启、稳定错误码与最近时间，不默认包含剪贴板正文。
2. **隐私控制**：不记录历史/隐身模式、清理历史入口、敏感文本规则、未来可选的按应用排除；要验证跨平台识别 foreground app 的能力和权限限制。
3. **语言/Provider 清晰度**：provider 能力声明、目标语言映射、fallback 来源透明度、服务限制提示。
4. **无障碍**：全键盘操作、焦点/读屏语义、减少透明度/对比度行为在真实 Windows/macOS 上测试。
5. **扩展 provider**：先设计 trait/错误分类/请求策略契约；兼容 OpenAI API、自托管或本地 provider 必须以安全 endpoint、隐私提示和 SSRF/密钥保护为前提。不得为了“支持任何 URL”而无审查地允许内网 URL 或任意 scheme。
6. **OCR/图像翻译**：单独 PRD 与性能/隐私威胁模型，不能作为普通文本翻译的顺手附加项。

## Phase 5 — 最终回归与发布准备

1. 完成 [`TEST_PLAN.md`](./TEST_PLAN.md) 中所有自动化门禁。
2. 执行 Windows x64 与 macOS Apple Silicon CI；macOS cursor/pasteboard、native opener、材质/窗口可见性须实机手测。
3. `npm audit` 与 Rust 依赖安全报告留档。
4. 检查版本号同步、README 中“验证不耗配额”、平台支持、语言映射与错误行为描述。
5. 查看 `git diff --check`、`git status`、完整 diff；排除格式化带来的无关噪音和任何凭证/本地路径。
6. 不自行打 tag/发布 release/合并 PR，除非用户明确授权。

## 变更提交建议

按以下粒度提交，单个 commit 尽量只对应一个 ID：

- `fix(macOS): use real cursor coordinates for popup`
- `fix(clipboard): prevent stale clipboard retrigger on macOS`
- `fix(translation): gate loading events by current generation`
- `fix(platform): open allowed external URLs cross-platform`
- `fix(provider): align credential validation with quota behavior`
- `fix(popup): report visibility failures accurately`
- `fix(storage): report retention cleanup failures`
- `fix(translation): preserve structured clipboard text`
- `fix(config): validate batches before applying side effects`
- `fix(privacy): redact upstream response details in logs`
- `test(provider): cover target language mapping and long requests`
- `ci(security): tighten release permissions and audit gates`

提交信息可以按团队习惯本地化；以上只用于建议每次改动的独立边界。
