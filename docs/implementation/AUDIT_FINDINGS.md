# QuickTranslate 代码审计发现清单

**基线**：`main` / `0.8.0`（最近可见提交 `3df3d1e3`，2026-10-22）  
**说明**：这是静态源码审查，不代表已在本机或目标 OS 上复现。每项均需按本文列出的测试确认行为。

严重度：
- **P1**：破坏产品核心承诺、跨平台核心路径或可能令最新请求永久不显示，应在扩展功能前处理。
- **P2**：边界行为、错误状态不准确、内容保真/隐私/可维护性风险；建议在第一轮核心修复后处理。
- **待验证**：有合理触发路径，但还需用测试或平台运行时实证，不能先宣称为已复现缺陷。

## 汇总表

| ID | 优先级 | 领域 | 结论 | 证据文件 |
|---|---|---|---|---|
| QT-001 | P1 | macOS / 光标定位 | macOS 光标函数当前固定返回 `(0, 0)` | `src-tauri/src/system/clipboard/mod.rs`、`supervisor.rs`、`translation_flow.rs` |
| QT-002 | P1 | macOS / 剪贴板去重 | 无序列号时的吸收分支会将 `last_text` 设为 `None`，未变化的旧文本可能再次被当作新复制 | `clipboard/mod.rs`、`clipboard/worker.rs`、`clipboard/controller.rs` |
| QT-003 | P1 | 并发请求 / UI 事件 | loading 事件未受 generation 闸门约束，旧请求可在新请求之后发出 loading 事件并污染前端 request id | `translation/coordinator.rs`、`translation_flow.rs`、`PopupWindow.tsx` |
| QT-004 | P1 | 平台兼容性 | `open_url` 固定调用 Windows `explorer`，在 macOS 上不可用 | `src-tauri/src/commands/system.rs` |
| QT-005 | P1 | Provider 验证 / 配额 | `validate_credentials` 走真实 `translate`，与“不消耗翻译配额”的产品文案不符 | 各 provider 文件、`domain/translator/mod.rs`、`README.md` |
| QT-006 | P2 | 语言映射 | DeepL/Tencent 将 `zh-tw` 映射成 `ZH`/`zh`，丢失繁体目标意图 | `deepl.rs`、`tencent.rs`、其它 provider 对照 |
| QT-007 | P2 | 浮窗 / 错误处理 | popup 复用或新建后 `show()` 错误被忽略，仍发 loading 并报告显示成功 | `system/translation_flow.rs`、`translation/coordinator.rs` |
| QT-008 | P2 | 存储 / 健康状态 | history/cache 清理失败只写日志，随后仍可能将 storage 报告为 healthy | `system/persistence.rs`、`runtime/status.rs` |
| QT-009 | P2 | 文本质量 / 数据保真 | 归一化把常见多空格、缩进、换行折叠成单空格，终端日志/代码输入会丢失结构 | `clipboard/mod.rs`、`translation/request.rs`、README 场景描述 |
| QT-010 | P2 | 配置一致性 | 批量配置校验和副作用失败被 warn 后忽略，调用仍返回成功；未知键/非法值缺统一拒绝策略 | `commands/config.rs`、`domain/config.rs` |
| QT-011 | P2 | 隐私 / 日志 | 通用 HTTP 错误构造与日志记录可能输出完整响应 body；需阻断敏感响应内容进入日志 | `domain/translator/mod.rs`、`error_class.rs` 调用链、`system/translation/coordinator.rs` |
| QT-012 | P2 / 待验证 | 网络 / 请求长度 | Google provider 通过 GET query 发送长文本；5000 个 CJK 字符可能生成过长 URI | `google.rs`、`translation/request.rs`、`policy.rs` |
| QT-013 | P2 / 待验证 | 剪贴板竞态 | 单槽 `ExpectedWrite` 仅按文本哈希匹配，无法区分内容相同的旧文本/用户新复制和应用写入 | `clipboard/controller.rs`、`commands/system.rs`、`clipboard/worker.rs` |
| QT-014 | P2 | CI / 供应链 | Release 中 `npm audit` 为 `continue-on-error: true`，高危发现不会阻塞发布；权限范围可进一步收窄 | `.github/workflows/release.yml` |

---

## QT-001 — macOS 光标位置固定为原点

**优先级**：P1 · **证据强度**：源码可直接确认；需要 macOS 实机确认最终窗口表现。  
**源码**：[`clipboard/mod.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/mod.rs) 的 `get_cursor_position()`；[`supervisor.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/supervisor.rs) 的 `TauriSink::emit()`；[`translation_flow.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation_flow.rs) 的 `compute_popup_position_dpi()`。

### 现象与原因

macOS 分支的 `get_cursor_position()` 目前返回 `(0.0, 0.0)`，并留下 `TODO: 接入 core-graphics`。剪贴板 sink 在触发翻译时直接读取该值并传给 popup 定位逻辑。因此“浮窗出现在光标旁边”这项核心体验在 macOS 上没有获得真实鼠标坐标；最终表现可能落在错误屏幕边缘/原点附近。

### 执行要求

1. 为 macOS 实现真实全局鼠标位置读取，优先采用稳定的系统 API / 项目可接受依赖；不要把 Windows 的物理像素坐标模型直接套到 macOS。
2. 将 `NSEvent.mouseLocation` 的左下原点语义，与 Tauri monitor/work-area 的坐标系统一转换；明确逻辑像素、物理像素和显示器缩放因子的责任边界。
3. 覆盖单屏、多屏、主屏左侧/上方存在负坐标的显示器、不同缩放比以及靠近屏幕边缘的光标位置。
4. 读取坐标失败时使用可解释的 fallback，并记录稳定错误码；不要用 `(0,0)` 假装成功且静默继续。

### 验收

- macOS 上在每台显示器的四个象限复制文本，浮窗锚点应与鼠标位置一致。
- 浮窗完整位于正确显示器工作区，不被菜单栏、Dock 或边缘裁切。
- 单元测试覆盖坐标转换纯函数；实机测试覆盖真实 API。
- Windows 路径不能回归。

## QT-002 — macOS 关闭浮窗后旧剪贴板内容可能再次触发

**优先级**：P1 · **证据强度**：跨函数控制流可确认；需在 macOS 验证实际重复弹出。  
**源码**：[`clipboard/mod.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/mod.rs) 的 `clipboard_seq()`；[`worker.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/worker.rs) 的 `absorb_current()`、reset 分支和 `is_new_content()`。

### 现象与原因

Windows 通过 `GetClipboardSequenceNumber()` 获取序列号。非 Windows 的 `clipboard_seq()` 返回 `None`。而 `absorb_current()` 仅在序列号可用时才读取文本，因此 macOS 的 hide/reset 路径得到 `(None, None)`。worker 随后从剪贴板读到仍未变化的旧文本；`is_new_content(last_text=None, ...)` 会返回 `true`，防抖结束后可能再次执行翻译并重新弹窗。当前代码注释也承认无序列号的降级可能因残留内容重新弹出。

### 执行要求

1. 优先让平台 backend 提供可靠的变化代数/变更 token（macOS 可评估 Pasteboard `changeCount` 等系统能力）；让 worker 不通过“是否 Windows”去猜语义。
2. 如果某平台无法可靠检测同文本再次复制，应定义显式降级策略，并避免 close/reset 自身造成自动重触发。优先保证“关闭就是关闭”，再单独评估“再次复制完全相同文本能否触发”。
3. 把平台能力抽象到 `ClipboardBackend` trait 中，保持 worker 的算法可在 mock backend 中确定性测试。
4. 处理暂停、恢复、supervisor 重启、应用自身复制按钮与用户复制相同文本等路径。

### 必测场景

- macOS：复制文本 → 弹出 → 关闭 → 2 秒内不再复制 → 浮窗不得自动重新出现。
- 关闭后用户再次明确复制同一文本：按已定义的平台语义正确触发或给出明确的能力限制。
- 暂停期间复制，恢复监控不应立即翻译暂停期间遗留文本。
- Windows 现有序列号路径保持原有行为。

## QT-003 — 旧请求的 loading 事件可覆盖新请求

**优先级**：P1 · **证据强度**：有明确异步交错窗口；需要新建确定性回归测试确认。  
**源码**：[`coordinator.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation/coordinator.rs) 的 `execute_at_position()`；[`translation_flow.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation_flow.rs) 的 `show_popup_loading()`；[`PopupWindow.tsx`](https://github.com/Aswellle/quick-translate/blob/main/src/windows/popup/PopupWindow.tsx) 的 `handleLoading()`。

### 交错过程

1. 请求 A 调用 `supersede()` 获得 ID A。
2. A 在读配置等 `.await` 点暂停。
3. 请求 B 获得 ID B，显示 loading(B)，并安装 B 的翻译任务。
4. A 恢复后仍可调用 `show_popup_loading()`；该函数发 loading 事件前没有检查 ID A 是否已过期。
5. A 的 `install()` 会因已过期而失败，但前端早已将 `requestIdRef` 改成 A。
6. B 的结果抵达时，前端因 ID 不匹配丢弃它，浮窗可能停留在 loading 状态。

结果/错误事件使用 `run_if_current()`，但 loading 事件尚未统一受最新代际控制。

### 执行要求

- 所有会改变 UI 当前请求身份的事件（loading/result/error）必须共享同一 latest-wins 不变量。
- 在产生 loading 事件及显示窗口前检查该请求仍为当前代；检查与提交必须有原子化语义，不能做成“先判断、后 await、再 emit”。
- 如果依赖 `run_if_current()` 的锁内回调，回调不得 `.await`；审查锁持有时间，避免把网络等待放进同步锁。
- 前端可保留第二道校验，但不应被迫自己推导请求先后顺序；必要时加入后端单调 generation/sequence，事件 payload 的契约要同步更新。
- 避免 A 的 `show_popup_loading` 调用在 B 已显示后将 popup 重新定位到 A 的坐标。

### 验收

以测试 hook/barrier 控制交错：A 开始并暂停 → B 开始并完成显示 → 释放 A → 断言只允许 B 的 loading/result 出现在 UI；A 不得改变当前 request ID、浮窗位置或状态。还要测试 A 先完成、B 后完成等正常序列。

## QT-004 — `open_url` 使用 Windows `explorer`，macOS 调用失败

**优先级**：P1 · **证据强度**：平台 API 使用方式可直接确认。  
**源码**：[`commands/system.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/commands/system.rs) 的 `open_url()`。

函数注释写明支持 `http/https`，但实际只调用 `Command::new("explorer")`。`explorer` 是 Windows 可执行程序，在 macOS 上通常不存在，因此应用中打开网站、项目页或支持链接的动作可能失败。

### 执行要求

- 优先使用 Tauri 官方 opener 插件/标准平台打开器，并在新增依赖前检查当前 Tauri 版本兼容性与权限 capability。
- 如采用平台实现，Windows 与 macOS 必须各自正确实现，不允许把外部 URL 传进 shell 字符串执行。
- 使用 URL parser 解析 URL，不只做 `split("://")`；只允许 `http`、`https`，拒绝 `file:`、`javascript:`、`data:` 等 scheme。
- 根据插件使用方式调整 `Cargo.toml`、`package.json`（如果需要前端 API）和 `capabilities/default.json`，权限遵循最小授权。

### 验收

- Windows / macOS 分别测试有效 HTTPS、有效 HTTP、无 scheme、未知 scheme、空 URL 和恶意/畸形输入。
- 被拒绝的 URL 不启动任何进程；合法 URL 打开系统默认浏览器。

## QT-005 — Provider “验证凭证”实际执行翻译

**优先级**：P1 · **证据强度**：代码路径可直接确认。  
**源码**：[`translator/mod.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/domain/translator/mod.rs) 的 `validate_provider_credentials()`；`deepl.rs`、`baidu.rs`、`tencent.rs`、`youdao.rs` 的 `validate_credentials()`；[`README.md`](https://github.com/Aswellle/quick-translate/blob/main/README.md)。

### 现象与原因

引擎的验证命令调用 provider 的 `validate_credentials()`；多个 Provider 的实现通过 `translate("hi"/"hello", "zh")` 做凭证验证。这是实际翻译请求，不是独立的凭证查询端点。它至少消耗请求/限流预算；按服务商计费规则，也可能消耗字符额度。README 目前承诺“验证按钮即时校验凭证，不消耗翻译配额”，但代码没有足够机制保证这一点。

### 执行要求

1. 逐个核对服务商官方支持的认证/用量/账户信息端点和相应权限；不要假设所有 API 都存在“零配额验证”。
2. 能使用低成本认证端点时实现独立的 provider validation 方法；它应返回“有效 / 无效 / 无法判断 / 网络错误”等有区别的结果。
3. 对没有不计配额验证端点的 provider，选择准确方案：明确提示这会提交一次小额测试翻译并询问用户是否继续，或调整 UI/README 文案。不能继续无条件宣称不消耗额度。
4. 手动验证不能污染生产翻译熔断器/运行时健康（现有引擎注释已明确这个设计目标）。
5. 验证日志绝不记录 API Key、签名或完整请求参数。

### 验收

- 测试确认“仅校验”的 provider 不会调用 translate 路径；需要测试翻译的 provider 能在 UI 中明确告知用户。
- 401/403、限流、超时、网络失败和有效凭证分别有不同状态。
- README、设置页、帮助提示与真实实现一致。

## QT-006 — 部分 Provider 丢失繁体中文目标语义

**优先级**：P2 · **证据强度**：映射可直接确认；翻译结果的具体语言变体应由官方接口文档与服务端测试验证。  
**源码**：[`deepl.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/domain/translator/deepl.rs) `to_deepl_lang()`；[`tencent.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/domain/translator/tencent.rs) `to_tencent_lang()`；对照 `google.rs`、`baidu.rs`、`youdao.rs`。

目标配置允许 `zh-tw`，但 DeepL 将其映射成 `ZH`，Tencent 将它映射成 `zh`。其它 provider 明确映射 `zh-TW`、`cht`、`zh-CHT`。这至少意味着本应用没有把用户的繁体目标显式传递给这两家 provider；自动 fallback 可能让输出目标变体产生差异。

### 执行要求

- 为每家 provider 建立可测试的规范语言代码映射，并核对官方支持的目标语言代码。
- 若某 provider 确实无法指定繁体，必须明示限制；不要静默改成简体目标。
- 考虑在 fallback 策略中把目标语言能力纳入候选过滤，或者在结果 UI 标记“此翻译源无法保证繁体目标”。

### 验收

- 纯函数单测覆盖全部受支持语言与别名；包括 `zh`、`zh-tw`、未知/未来代码。
- Provider 集成契约测试能证明最终请求中的目标语言代码与配置一致。

## QT-007 — Popup `show()` 失败仍报告显示成功

**优先级**：P2 · **证据强度**：错误处理缺口可直接确认。  
**源码**：[`translation_flow.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation_flow.rs) 的 `show_popup_loading()`；[`coordinator.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation/coordinator.rs)。

复用 popup 时，对 `set_position`、`set_focusable`、`show` 的错误均使用 `let _ = ...`。新建窗口分支同样忽略 `window.show()` 的错误。随后仍发送 loading 并返回 `true`；调用者据此把 popup 运行状态上报为 healthy。若 `show()` 失败，用户看不到窗口，运行时诊断却声称正常。

### 修复要求

- 作为核心可见性操作的 `show()` 失败必须向调用者反馈；只在确实属于非关键优化的操作上选择 best-effort。
- `show_popup_loading()` 的 bool/Result 契约应明确定义：窗口成功显示才返回成功；位置调整失败可以单独降级记录，不应该掩盖显示失败。
- 仅当 loading 事件投递策略明确且窗口状态可用时，再宣告显示成功；不要只凭 `show()` 返回 OK 就声称窗口一定可见（如 API 本身不保证可见，需检查 `is_visible()` 或记录准确的“请求已提交”状态）。
- 不能让此修复破坏 QT-003 generation gate。

### 验收

用注入的 window-operation abstraction/mock 令 show 返回错误，断言 coordinator 向 runtime 报告 degraded、不会误报 healthy，用户可见错误状态有恢复出口。

## QT-008 — 存储保留上限/缓存清理失败未反映到健康状态

**优先级**：P2 · **证据强度**：错误分支可直接确认。  
**源码**：[`persistence.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/persistence.rs) 的 `process()`；[`status.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/runtime/status.rs)。

`history.insert()` 成功后若 `enforce_limit()` 失败，只打印 error；首次写入失败后重试成功的路径也一样。`cache.enforce_limits()` 失败只打印 warn。最终 `storage_error` 没有被设置，结尾会调用 `runtime.report_storage(Healthy, None)`。这样会让保留策略失效时依然显示存储健康，数据库/缓存可能持续膨胀。

### 修复要求

- 明确定义历史写入、历史清理、缓存写入、缓存清理各自的健康语义；清理失败不能被视为全部健康。
- 在 cleanup 失败时给稳定错误码（如 `HISTORY_RETENTION_FAILED`、`CACHE_RETENTION_FAILED`），并根据故障是否可恢复设置 degraded。
- 依据实际数据库错误类别评估单次重试，不要无限重试或阻塞翻译主路径。
- 在健康恢复后允许状态回到 healthy，避免一次暂态失败永久标红。

### 验收

注入 `enforce_limit` 和 `enforce_limits` 失败，断言 runtime 状态与错误码反映实际情况；随后成功路径必须能恢复状态。翻译结果的 UI 返回不能被慢数据库阻塞。

## QT-009 — 文本归一化会破坏代码/日志结构

**优先级**：P2 · **证据强度**：转换行为可直接确认；需用代表性语料验收真实质量影响。  
**源码**：[`clipboard/mod.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/mod.rs) 的 `normalize_text()`；[`request.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation/request.rs) 关于已归一化文本的说明；[`README.md`](https://github.com/Aswellle/quick-translate/blob/main/README.md) 中的终端报错/堆栈/CI 日志使用场景。

普通文本走 `split_whitespace().join(" ")`，会压平连续空格、TAB 和全部换行。结构化文本分支虽保留双换行，但各段仍用 `split_whitespace()` 折叠空白。Python 缩进、YAML 层级、堆栈对齐、表格列、终端输出和代码块可能失去结构。请求结构注释还说明 worker 下游只持有归一化文本，原始文本没有被保留。

### 修复方向

- 明确区分 **原始剪贴板文本**、**用于去重/防抖的规范化键**、**送给翻译源的请求文本**；不要用一个被破坏结构的字符串承担三种责任。
- 规范化应优先服务于去重，而不是无条件改写翻译输入和历史记录。
- 如果需要压平普通句子的换行，采用有明确定义的启发式规则；对代码、日志、列表、缩进、表格、段落保真。
- 不要求一定做完整语言/代码检测；最小可行方案也必须避免对明确结构化内容做不可逆空白折叠。
- 更新字符上限/截断逻辑，保证 original/normalized/truncated 语义一致，避免缓存键与用户可见原文不一致。

### 验收样例

- 普通文本：`hello\nworld` 可以按设计合并为自然句子。
- Python `if` 缩进、JSON/YAML、终端多行堆栈、连续空格列、含 emoji/CJK 的长文本和 CRLF：必须符合设计的保真策略。
- 用于去重的规范化文本相同，不应导致历史原文被意外覆盖或只保存损坏后的文本。

## QT-010 — 配置批次可能静默部分成功

**优先级**：P2 · **证据强度**：当前错误吞掉行为可直接确认。  
**源码**：[`commands/config.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/commands/config.rs) 的 `set_config_batch_inner()` / `validate_update()`；[`domain/config.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/domain/config.rs)。

当前批次逐项验证时，验证失败只打印 warn 后跳过；合法项仍可能持久化。后续 side effect 失败也只写 warn，批次最后仍返回 `Ok(())`。`validate_update()` 基本只验证 provider 是否存在；未知键与部分格式错误可能进入配置写路径。前端会误以为保存完整成功，导致配置 DB、provider 实例、OS 自启动状态等发生不一致。

### 修复要求

- 建立配置 key allowlist 与类型/范围验证（provider、language、theme、bool、`history_limit`、凭证等）。未知键必须拒绝。
- 明确批处理事务语义：推荐先对整批完整校验；若任意值非法则拒绝整批并返回可定位错误；或返回明确的 partial-result 对象并由 UI 展示每一项状态。
- 对“已持久化但副作用失败”的键制定补偿/重试/回滚策略，不得伪装整体成功。
- 保持掩码凭证 no-op 语义，绝不能让 `********abcd` 覆盖真正凭证。
- auto_start 需要考虑 OS 状态和 DB 持久化的补偿次序。

### 验收

- 向批次提交一个有效键 + 一个非法键：按定义整体拒绝或返回明确部分失败，不能只 warn。
- 未知配置键、无效语言代码、无效 theme、越界 `history_limit`、autostart OS 错误均有测试。
- 掩码凭证回写仍不会覆盖 DB 与内存中的真实凭证。

## QT-011 — 上游响应 body 进入普通日志的隐私风险

**优先级**：P2 · **证据强度**：日志传播路径可确认；响应内容是否包含用户文本由上游具体行为决定。  
**源码**：[`translator/mod.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/domain/translator/mod.rs) 的 `http_status_error()`；[`coordinator.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation/coordinator.rs) 的翻译失败日志；各 provider 在 HTTP 错误时的 `response.text()` 路径。

通用错误会把完整 `body` 拼进 `AppError::NetworkError`，provider 调用层和 coordinator 随后会记录错误字符串。第三方服务在异常响应中可能带请求片段、账户/请求标识、诊断数据或服务端调试内容。即使正常路径不写剪贴板文本，也不应把未经筛选的外部响应 body 直接记入常规日志。

### 修复要求

- 用户可读错误与诊断日志使用不同的数据结构；普通日志只记录 provider ID、HTTP 状态码、稳定错误码、request ID 和已截断/脱敏的诊断摘要。
- 对响应 body 设置严格字节上限，默认不存完整内容；可对已知服务商返回结构化错误代码做白名单提取。
- 清理可能出现在日志里的 API Key、签名、Authorization、请求体和复制文本。不要把用户文本放进错误对象的 `Display`，再让上层无差别记录。
- 更新导出诊断 bundle 时复用统一 redact 函数。

### 验收

用含伪造 API Key、长随机敏感片段及用户原文的 mock HTTP error body，断言日志输出不含这些敏感值，且仍可判断 provider、HTTP 状态和稳定错误码。

## QT-012 — Google GET 请求的 URI 长度风险（需验证）

**优先级**：P2 / 待验证。  
**源码**：[`google.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/domain/translator/google.rs) 的 `do_request()`；[`request.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/translation/request.rs) 的 `MAX_TEXT_CHARS=5000`。

Google provider 将完整文本放在 GET query 参数 `q`，全局上限为 5000 个 Unicode 字符。CJK/emoji 编码后的 URL 会显著膨胀，可能触发客户端、代理或服务端 URI 长度限制。当前未实机复现，因此不能宣称一定失败。

### 执行要求

- 测试长度 1、常见长度、5000 个 ASCII、5000 个 CJK、emoji/混合文本的最终 URL 字节长度和请求表现。
- 如果非官方端点只支持 GET，则为 Google 单独定义更保守的安全上限或安全分段方案；不得将分段翻译悄悄拼接造成语义丢失。
- 明确超长失败的用户提示；不能无限 fallback 后只显示泛化错误。

## QT-013 — `ExpectedWrite` 只凭文本哈希仍有竞态（需验证）

**优先级**：P2 / 待验证。  
**源码**：[`controller.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/controller.rs) 的 `ExpectedWrite`、`mark_app_write()`、`consume_if_expected()`；[`commands/system.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/commands/system.rs) 的 `copy_to_clipboard()`；[`worker.rs`](https://github.com/Aswellle/quick-translate/blob/main/src-tauri/src/system/clipboard/worker.rs) 的消费分支。

当前 expected write 以哈希绑定内容，显著改善了旧裸布尔标记，但哈希只能判断“读到的内容是否相同”，无法证明此次内容变化是应用写入而非用户动作。如果用户点击“复制原文”，而剪贴板本来已经是同一原文，worker 可能在 OS 写入完成前读取到相同文本并消耗登记；随后真正的应用写入使序列号变化时，仍可能被误判为用户重复复制。另一个边界是单槽登记无法完整表示并发应用写入。

### 执行要求

- 先写可确定复现的 mock backend 时序测试；不要只基于直觉重写 controller。
- 考虑用 `(expected text hash, baseline sequence, expected new sequence/change token, expiry)` 或统一串行化写入/消费的协议；需要把 Windows/macOS 的平台变化 token 纳入设计。
- 处理写失败撤销、重复写同一内容、用户在 app 写入同时复制新内容、短时间连续点击原文/译文按钮、暂停/恢复。
- 不得以单纯加长 TTL 来掩盖问题。

## QT-014 — Release 安全门禁与权限范围

**优先级**：P2 · **证据强度**：workflow 配置可直接确认；属于供应链防护改进，不代表已有入侵。

**源码**：[`release.yml`](https://github.com/Aswellle/quick-translate/blob/main/.github/workflows/release.yml)。

当前 release workflow 的 `npm audit --audit-level=high` 设置了 `continue-on-error: true`，因此高危依赖漏洞不会阻止发布。workflow 顶层的 `permissions: contents: write` 范围也可以进一步按 job 收窄。工作流引用了 `actions/checkout@v4`、`actions/setup-node@v4`、`actions/cache@v4` 与 `tauri-apps/tauri-action@v0` 等可移动 tag，尚未体现 immutable SHA pinning。

### 执行要求

- 将写权限限制到确实上传 Release/资产的 job；lint、测试、manifest verify 尽量使用只读权限。
- 使高/严重依赖审计能阻止发布；如需豁免，采用经评审的带理由 allowlist，而非整个步骤 `continue-on-error`。
- 评估 npm 与 Cargo 的依赖审计/SBOM；把扫描结果、版本锁文件与发布资产关联起来。
- 评估 GitHub Actions 以完整 commit SHA pinning，维护可读的版本注释和自动化升级流程。
- 不应在本任务里盲目升级全部依赖版本；先做风险评估，再将安全升级分成独立 PR。

### 验收

- 人为注入/模拟 high severity audit 失败，发布任务应阻断或命中经过审计的明确豁免。
- 无写入需求的 job 不获得 `contents: write`。
- Release 仍能正确生成并验证 `latest.json` 和各平台签名产物。

---

## 已修复的历史问题：不要重复实现

旧版 [`docs/CODE_REVIEW_v0.2.1.md`](https://github.com/Aswellle/quick-translate/blob/main/docs/CODE_REVIEW_v0.2.1.md) 是 2026-07-26 对 `v0.1.2..v0.2.1` 的历史审查，不是当前版本的完整差异列表。本次读取的当前源文件已体现多项历史修复：

- Popup error/loading/idle 折叠展开高度恢复（`PopupWindow.tsx` 的尺寸 effect 不再依赖 result 非空）。
- 键盘快捷键对按钮/链接/输入框让位（`src/lib/domUtils.ts` 的 `isKeyConsumingTarget()`）。
- `useTauriEvent()` 用 ref 调用最新 handler，并清理异步 listener 注册竞态。
- 应用 clipboard write 通过 `ExpectedWrite` 哈希登记，而非单独的裸布尔标志。
- popup 内容变化后两阶段重新测量尺寸。
- 折叠/宽度 UI 状态更新不再把 resize IPC 放在 React state updater 内。

Agent 应验证这些机制不回归，而不是按旧报告再次照搬修复。
