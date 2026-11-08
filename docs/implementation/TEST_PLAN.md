# QuickTranslate 测试与验收计划

## 1. 本地基础门禁

运行前先确认 README/CI 对所需 OS 的说明，以及当前仓库 `package-lock.json` 与 Cargo lock 文件状态。不要自动修改 lockfile 以“修复”未相关依赖。

```bash
npm ci
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Windows/macOS 的 release target 还应从对应 CI/job 执行与仓库一致的构建验证。上述命令在某 OS 上不可执行时，报告中标为 `NOT RUN` 并写明原因，不得标作 PASS。

## 2. P1 缺陷回归测试矩阵

| ID | 测试类型 | 测试输入/操作 | 预期 |
|---|---|---|---|
| QT-001 | Rust 纯函数 + macOS 手测 | 鼠标位置在主屏、次屏、左侧/上方负坐标屏、不同缩放屏；靠近各边缘 | 位置换算正确，popup 不固定落在 (0,0)，不裁切 |
| QT-002 | worker mock + macOS 手测 | popup close 后不再复制；复制同文本；暂停时复制后恢复；重启 worker | 关闭不自触发；同文本复制遵守明确平台行为；恢复/重启不重新翻译旧内容 |
| QT-003 | coordinator deterministic concurrency test | A 等待配置；B 先显示 loading；A 恢复并尝试显示/loading | A 不可改变窗口位置/UI request identity；B 的 result/error 不被前端丢弃 |
| QT-004 | URL parser unit + Windows/macOS manual | HTTP/HTTPS、空/畸形 URL、`file:`、`javascript:`、`data:` 和异常协议字符串 | 合法 URL 打开默认浏览器；非法 URL 被拒绝且无进程副作用 |
| QT-005 | provider contract tests + UI test | 每家 provider 验证凭证；注入 translate call counter | “不耗配额验证”不得意外走 translate；需测试翻译的 provider 提示用户并需确认 |

## 3. P2 回归矩阵

| ID | 测试 | 预期 |
|---|---|---|
| QT-006 | 对所有 provider 语言映射做 table-driven tests | `zh` 与 `zh-tw` 不意外折叠；不支持的映射有明确处理 |
| QT-007 | mock window show success/failure | show 失败返回失败/降级状态，不 emit 成功 loading，不报告 healthy |
| QT-008 | persistence 依赖故障注入 | history/cache cleanup 失败后 storage 降级，错误码正确；恢复成功回到 healthy |
| QT-009 | 原文、去重键与请求文本的测试矩阵 | 代码/日志/缩进/段落/CRLF/CJK/emoji 按声明策略保真，截断字符数正确 |
| QT-010 | config batch tests | 未知 key/非法值被整体拒绝或明确 partial result；副作用失败不伪装成功 |
| QT-011 | fake provider response body with secret sentinels | 日志/诊断中不包含凭证、签名和用户原文，仍有可用错误码 |
| QT-012 | URL encoding length test + optional integration test | 5000 CJK/emoji 预期长度被测量；超过 provider 安全限额时有明确行为 |
| QT-013 | mock backend deterministic race | expected write 不会吞掉用户真正的新复制，重复复制原文不会形成应用自触发循环 |
| QT-014 | workflow static check + test release simulation | audit 高危门禁有效；job 权限最小化；更新清单/签名校验不回归 |

## 4. 并发测试建议

不要靠大量随机 `sleep` 的 flaky 测试捕捉 QT-003。优先把交错点变成受控的可注入等待：

1. 测试请求 A 生成 ID 后暂停在 generation gate 前。
2. 启动请求 B、确认其 generation 已更新并完成其 loading event 的提交。
3. 恢复 A。
4. 记录所有 window position changes 与 emitted loading/result/error IDs。
5. 断言 A 没有提交任何过期状态，B 仍能提交自己的结果。

若不能在 Tauri unit test 环境建立真实 window，则把决定“是否允许提交”抽成纯 coordinator 状态操作，对提交动作传入测试 closure；另外在 UI 层测试 listener 丢弃策略。不要通过模拟网络请求来测试 generation 算法。

## 5. Clipboard mock 测试结构

`ClipboardBackend` 应能模拟：

- 返回文本、空 clipboard、暂时性读取失败、致命句柄错误；
- 稳定不变的 change token、变更 token、平台无法提供 token；
- 同文本再次复制但 token 变化；
- 应用写入前后 token 的变化；
- 用户在应用标记 expected write 后复制另一段文本；
- 暂停/恢复、reset request、supervisor worker restart。

mock 中应对每个操作给出明确步骤序列，不要只用真实系统剪贴板做单元测试。平台真机用于验证 mock 无法覆盖的 API 坐标、权限、焦点和剪贴板 change count 行为。

## 6. 语言与 Provider 契约测试

为 provider 编写纯函数映射表，至少包括：

- 简体中文 `zh`；
- 繁体中文 `zh-tw`；
- `en`, `ja`, `ko`, `fr`, `de`, `es`, `ru`, `pt`, `it`, `ar`；
- 空字符串、大小写变体、未知语言代码。

还应对不同 provider 的 JSON/form/query payload 做构造层测试，证明实际请求目标语言与 mapping 对齐。集成测试不得强制调用付费 API；默认使用 mock HTTP server。需要 live provider 验证时，标明其可能消耗配额，并只作为显式 opt-in 手工测试。

## 7. 日志与隐私测试

构造假响应 body，例如包含以下哨兵值：

- `TEST_API_KEY_1234567890`
- `TEST_SIGNING_SECRET_abcdef`
- `PRIVATE_USER_CLIPBOARD_TEXT_DO_NOT_LOG`

触发 provider HTTP 错误后收集 tracing 输出，断言这些哨兵不会出现。不要用真实 key 或用户剪贴板内容做测试。错误 UI 允许显示安全的、经过截断的服务商错误摘要，但不能出现认证凭证和完整原文。

## 8. UI / Accessibility 实机清单

### Windows 10/11

- 系统缩放 100%、150%、200%；多屏与负坐标显示器；底部/侧边任务栏。
- popup 首次显示、loading→result、loading→error、折叠/展开、窄/宽切换、长译文和长原文。
- 键盘 Tab/Enter/Space/Escape 与鼠标复制/拖动；快捷键不得吃掉用户在其他应用输入的字符。
- 应用深色/浅色/system 主题变化，降低透明度模式，浮窗可见性错误和 runtime 状态。
- explorer/默认浏览器打开 HTTPS 链接。

### macOS (Apple Silicon)

- macOS 当前最低支持版本及最新受支持系统；标准和 Retina scaling；主屏之外的屏幕。
- 鼠标锚定位置；Pasteboard close/reset/同文本重新复制；暂停/恢复监控。
- 系统默认浏览器 URL opener；app 主题与 NSWindow appearance 同步；Liquid Glass/Popover fallback。
- 辅助功能/屏幕阅读器基本语义，降低透明度，键盘激活行为与 popup non-activating 模式。

## 9. Release 验收

- `package.json`、`Cargo.toml`、`tauri.conf.json` 版本一致；tag 版本与文件版本一致。
- CI 的 TypeScript、frontend tests、Rust fmt/clippy/tests、Windows/macOS build verify 均通过。
- Release workflow 的 dependency audit 有真实门禁或有书面豁免；不能被 `continue-on-error` 静默放行。
- Windows MSI/NSIS 与 macOS DMG 生成成功；更新 `.sig` 与 `latest.json` 条目完整。
- 运行时打开外链、自动更新检查与升级路径分别测试。
- 完整 diff 中不含 `.env`、API key、私有 URL/令牌、测试机个人信息或无关生成文件。

## 10. 每阶段报告格式

Agent 每完成一个 phase 都应提供：

1. **已完成**：对应 QT IDs 与可见行为变化。
2. **源码修改**：新增/修改文件和每个改动的目的。
3. **测试结果**：逐条列命令、exit code/结果；不可用项标 `NOT RUN`。
4. **回归风险**：Windows/macOS 差异、IPC/数据 schema 变化、SQLite migration 影响。
5. **未完成/阻塞**：不隐瞒未复现项，不把假设写成修复完成。
6. **下一阶段**：仅推荐下一小批任务，不自动扩大范围。
