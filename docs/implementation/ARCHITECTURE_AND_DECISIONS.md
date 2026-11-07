# QuickTranslate 架构说明与实施决策

## 1. 当前系统结构

QuickTranslate 是一个以 Tauri 2 为桌面壳、Rust 负责剪贴板/网络/数据库/平台 API、React 19 + TypeScript 负责多窗口 UI 的桌面常驻应用。核心交互是系统剪贴板文本变更 → debounce → provider 翻译 → popup loading/result/error → 本地 history/cache 落盘。

```text
OS Clipboard
  └─ clipboard backend (platform API)
      └─ worker (poll, normalize/dedupe, debounce)
          └─ supervisor (rebuild failed backend/worker)
              └─ TranslationSink
                  └─ TranslationCoordinator
                      ├─ request ID / generation (latest-wins)
                      ├─ popup show + loading event
                      └─ translation task
                          └─ TranslationEngine
                              ├─ ordered provider registry
                              ├─ retry + overall budget
                              ├─ circuit breaker / health
                              └─ fallback providers
                                  └─ result/error event
                                      └─ popup UI stores/components
                          └─ bounded PersistenceWriter queue
                              ├─ history table
                              └─ exact translation cache
```

## 2. 关键模块职责

### Clipboard subsystem

- `clipboard/mod.rs`：系统剪贴板原语、文本规范化、光标位置、平台 change token。
- `backend.rs`：将平台读剪贴板操作包装为 `ClipboardBackend`；mock backend 让 worker/supervisor 在无 OS runtime 的情况下被测。
- `worker.rs`：轮询、防抖、去重、应用写入吸收及向 translation sink 触发文本。
- `controller.rs`：暂停/恢复/重置与 expected-write 状态、健康快照。
- `supervisor.rs`：在句柄初始化/worker 致命失败后重启与退避。

**决策 D1：平台差异应进入 backend contract。** Worker 不应由 `cfg(target_os=...)` 四处分支控制语义，也不应以无变化 token 冒充可靠序列号。

### Translation coordinator

- `request.rs`：`RequestId`、generation 与单次 request 数据。
- `coordinator.rs`：新请求取代旧请求、旧任务取消、结果提交的 generation gate、历史落盘入队。
- `translation_flow.rs`：窗口创建/定位/大小、loading/result/error 事件呈现。

**决策 D2：所有 UI 状态变更事件必须遵循 latest-wins。** 当前已对 result/error 加 gate，但 loading 也会改变前端 request identity 和 popup state，因此必须纳入同一协议。

**决策 D3：generation check + commit 之间不能有 await。** 如果做成“check current → await → emit”，旧请求仍可能在空档被更新请求取代。需要同步提交边界，或在前端/后端使用明确单调 generation token。

### Provider engine

- `TranslationProvider` trait：provider 的 translate / info / validate_credentials。
- `TranslationEngine`：provider 注册表、active provider、fallback order、预算调度、健康/熔断。
- `policy.rs`：8 秒整次预算、单 provider timeout/retry policy。
- `error_class.rs`、`health.rs`：分类重试、fallback 和 circuit-breaker 的错误类别与健康状态。

**决策 D4：重试、fallback、凭证验证是三种不同策略。** 真实翻译调用不能被称为“免费认证检查”；手动验证也不应污染常规翻译熔断状态。

### Persistence and data

- `infra/database.rs`：SQLite migration、WAL、完整性检查、损坏恢复与配置抢救。
- `domain/config.rs`：配置读写、内存缓存、API 凭证加密/脱敏。
- `domain/history.rs`：历史分页、搜索、收藏、限额清理、统计。
- `domain/cache.rs`：失败/离线时可以使用的精确历史翻译缓存。
- `system/persistence.rs`：有界 channel 和单 worker，避免数据库写入阻塞翻译主路径。

**决策 D5：持久化失败不得影响已返回的译文，但也不得谎报健康。** UI 的翻译成功与 history/cache 的写入成功是两件事。队列溢出、写入失败、保留清理失败要分别有稳定 diagnostics 语义。

### Frontend

- `PopupWindow.tsx`：loading/result/error 生命周期、窗口尺寸、点击激活/拖动、快捷键、主题与透明度同步。
- `useTauriEvent.ts`：Tauri event 订阅清理，并通过 ref 使用最新 handler。
- `domUtils.ts`：统一键盘消费目标与拖拽禁区语义。
- `commands.ts`：统一 Tauri invoke 封装和 TypeScript 数据契约。

**决策 D6：不要重新复制旧的 popup event/resize 实现。** 当前组件已有 generation ID 的前端二次校验、最新 handler ref 和两阶段尺寸测量，应通过新增测试扩展而非将逻辑退回分散的监听器。

## 3. 必须保持的系统不变量

1. **Latest-wins**：旧请求不能改变最新 UI 身份，不能显示旧译文/旧错误，不能把 popup 移回旧坐标。
2. **关闭可预期**：隐藏 popup 本身不能触发新的翻译；剪贴板里的旧文本不会因 reset/supervisor restart/resume 被当成用户新复制。
3. **被动浮窗不抢焦点（Phase 8b，含一处已记录的例外）**：翻译弹窗以 `focusable(false)` 显示、不夺走用户正在输入的焦点；用户点击后才转交互态。**例外（2026-10-30 记录）**：错误态浮窗出现时自动 `activatePopup()` —— 错误需要用户处置而被动浮窗收不到键盘事件，Esc/空格会全部落进底层应用；成功态仍保持被动态。该例外依赖 Windows 对后台进程 `SetForegroundWindow` 的前台权限放行，若实测激活失败只是退回旧行为（点击路径仍可用），不算缺陷。
3. **平台契约诚实**：macOS 与 Windows 的坐标、pasteboard change token、系统 URL opener 和窗口可见性应由真实平台 API 支撑。
4. **输入文本可追溯**：区分原始输入、去重 key、provider request text、显示文本与存档文本，不能意外把信息有损规范化当作原文。
5. **Provider policy 有单一归属**：provider 的请求超时/重试/熔断由 engine/policy 控制；provider 不要偷偷增加额外重试层。
6. **预算有界**：整次翻译仍受总预算限制，新增 provider/验证动作要定义超时和额度语义。
7. **错误状态可信**：`healthy` 只在实际对应功能成功时使用；缓存保留清理失败、窗口 show 失败不得被吞掉。
8. **数据库不阻塞翻译结果**：history/cache 走有界队列，禁止把慢 DB await 放在用户可见结果前。
9. **凭证保密**：前端只收到脱敏凭证；普通日志不包含 API Key、签名串、复制文本或未经脱敏的 response body。
10. **配置副作用可解释**：设置界面收到成功时，必须代表已定义范围内的变更确实完成；批次部分失败要明确传达。
11. **能力声明可靠**：provider 未配置、目标语言不被支持、凭证未知状态与 provider 故障不得混为一谈。
12. **Release 证据可复核**：版本同步、CI、签名、updater manifest 和安全扫描是彼此独立的门禁，必须分别通过。

## 4. 推荐的最小类型/接口改进

这些是实现 P1/P2 时可用的设计方向，不是要求在第一天一次性新增所有类型。

### Clipboard snapshot

考虑将目前零散的 `seq()` / `get_text()` 与 expected-write state 组合为明确结构：

```rust
struct ClipboardSnapshot {
    text: Option<String>,
    change_token: Option<ClipboardChangeToken>,
}

// token 的具体表示可随平台不同，但它的语义必须是“剪贴板变更版本”，
// 不能用 Option::None 的情况在 reset 路径里自动触发“总是新内容”。
```

实现注意：在同一快照内读取文本与 change token 是否能保证一致，由平台 API 能力决定；若不是原子读取，需识别读取前后 token 变化并重试/降级。

### Translation UI event

考虑把所有事件共用 envelope：

```ts
interface TranslationEventEnvelope {
  request_id: string;
  // 如果后端引入单调 generation，可在此放 number；不要把 UUID 当作可排序数值。
  generation?: number;
}
```

Rust 仍为最终权威。`loading`、`result`、`error` 都应经过同一提交规则，事件 schema 变更需同时更新 Rust payload、frontend types、listeners 和测试。

### Input text

考虑以结构体表达三种用途：

```rust
struct ClipboardText {
    original: String,
    dedupe_key: String,
    request_text: String,
    truncated: bool,
}
```

只有当测试证明原始文本与请求文本确有必要区分时才新增；不要机械增加永远不被消费的字段。必须明确历史/缓存保存的是原文还是送翻译源的文本。

### Provider result categories

凭证验证返回值不应只用 `bool` 区分所有失败，可考虑：

```rust
enum CredentialValidation {
    Valid,
    Invalid,
    RateLimited,
    NetworkUnavailable,
    NotSupportedWithoutTranslation,
}
```

按 UI 真实需要裁剪类型；用户主动测试翻译的确认状态可以放在 UI 层。

## 5. 明确不要做的事情

- 不要为修一个 race 替换整个 coordinator。
- 不要把所有平台功能都塞进 `commands/system.rs` 的单一巨大函数。
- 不要只加 `sleep()`、更长 TTL 或重试次数来掩盖竞态。
- 不要默认对所有剪贴板文本加大历史上限，或将用户文本上传到诊断服务。
- 不要将 provider 认证错误当作网络错误无限重试；不要在 provider 内另写第二个重试 loop。
- 不要使用裸 `unwrap()`/`expect()` 处理用户可触发 I/O 错误，尤其发布 `panic = "abort"` 时。
- 不要未经证据就升级全套 npm/Cargo 依赖。
- 不要因 macOS 当前测试无法运行而把 `cfg(target_os="macos")` 代码删掉或用 Windows fallback 替代。
