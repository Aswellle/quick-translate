# QuickTranslate 安全、隐私与数据处理要求

## 1. 数据流与信任边界

QuickTranslate 的核心功能需要读取系统剪贴板。用户复制的文本可能来自邮件、内部文档、代码仓库、终端、客户资料或凭证。翻译时，这些文本可能发送到当前 provider 或 fallback provider；翻译结果与原文可能写入本机 SQLite history/cache。API credentials 存在本地，配置层通过加密存储并在 UI 中脱敏。

因此需要区别以下数据类型：

| 数据 | 可能去向 | 默认处理要求 |
|---|---|---|
| 剪贴板原文 | 本地内存、选定的翻译服务商、可选历史 | 只在触发翻译后按设置传输；日志永不记录正文；历史应可关闭/清理 |
| 译文 | 本地 UI、历史、缓存 | 本地持久化应按用户设置与保留上限执行 |
| Provider API Key / Secret | 本地加密配置、请求头/签名过程 | UI 仅显示掩码；日志、错误、诊断包、测试 fixture 不得泄露 |
| Provider 响应 body | 错误分类/诊断 | 结构化提取，限制大小并脱敏；不完整写入常规日志 |
| runtime diagnostics | 本地设置页/用户主动导出 | 仅使用稳定错误码、计数与时间；默认不包含输入文本/凭证 |
| URL / provider endpoint | 系统浏览器或网络请求 | 解析与 allowlist；未来自定义 endpoint 必须有 SSRF/私网地址审查 |

## 2. 现有保护措施与不能假设的事项

已经在代码中看到的措施：

- `ConfigService.get_all()` 对凭证脱敏。
- 凭证使用 AES-256-GCM，并具有 per-install secret 的初始化与旧密钥迁移逻辑。
- SQLite 使用参数绑定，history query 对 `limit` / `offset` 做限制。
- Tauri CSP 限制默认资源来源；capability 为窗口命令与插件权限列表。
- 翻译缓存仅在所有 provider 不可用时作为回退，不默认覆盖在线新结果。
- runtime health/logging 大多使用稳定事件名与错误码。

但不能由此自动推断：

- 所有日志都已脱敏；HTTP response body 仍可能进入 error display/logging。
- “凭证加密”代表密钥不会在同用户权限下被提取；per-install secret 与应用数据同处用户权限边界，应根据威胁模型诚实描述。
- provider validation 不消耗配额；当前多个 provider 的验证实现走 `translate()`。
- 剪贴板内容绝不离开设备；用户触发云端翻译时，文本会传给对应的第三方服务商。
- 所有平台上关闭/暂停行为一致；macOS clipboard change token 与光标坐标存在明确待修问题。

## 3. 必须保持的凭证约束

1. 前端只获得脱敏后的 `AppConfig`。不要新增 command 将完整 provider credentials 返回到 WebView。
2. masked credential no-op 约束不能被绕过。`********suffix` 一类 UI 回传值不得覆盖真实凭证或重建 provider。
3. 不把 API Key 拼到 URL query（如 provider 支持 header/body，应按其官方认证方式处理）。
4. 不将 `Authorization`、签名字符串、secret id/key 或带完整请求参数的 debug 信息写入常规日志。
5. Provider 更新应继续走构造新实例再替换的路线；不要就地改变正在被请求使用的凭证对象。
6. 凭证验证要明确其网络行为和配额影响；不允许声称零成本而实际发送真实翻译请求。

## 4. 剪贴板隐私控制建议

P1 修复完成后，建议按小而独立的功能增加以下控制：

- **不记录历史模式**：翻译仍可显示，但不写入翻译历史；是否写精确缓存也应由产品语义明确，不要默认为仍缓存。
- **清理历史与缓存**：分清删除历史、清空缓存、清理所有本地翻译数据的区别。
- **敏感文本保护**：给密码/令牌/私钥/访问 token 等典型模式提供本地检测开关；检测应尽量减少误报并让用户能理解为什么未自动翻译。
- **按应用排除**：在可实现的平台上允许用户排除密码管理器、银行、终端或其他敏感应用；先确认跨平台获取前台应用标识的 OS 权限与限制，不可假称所有平台都能无权限识别。
- **翻译源提示**：设置页明确显示当前 active provider、fallback provider，以及触发时文本可能发送给哪些服务商。
- **手动确认模式**：在用户希望谨慎处理敏感文本时提供暂停/单次确认，而不是无条件自动调用远端 provider。

除用户明确授权外，不要加入后台遥测、剪贴板正文上传或“为了改善模型质量”的数据收集。

## 5. 错误与日志脱敏规范

### 应记录

- 稳定事件名，如 `translation_request_failed`。
- 非敏感随机 request ID（优先短 ID；前端协议需要的全 ID 不应被拼进外部公共日志）。
- provider ID、HTTP status、稳定 error code、耗时、尝试次数、剩余预算、是否 fallback。
- runtime component state 与错误发生时间。

### 默认禁止记录

- `source_text`、`translated_text`、剪贴板内容、prompt 或输入片段。
- API Key、Secret、Authorization header、签名内容、cookie 或完整 URL 中的 credential 参数。
- 未截断/脱敏的 HTTP response body。
- 用户的本机绝对路径、用户名、个人文件名；必要时应匿名化。

### 具体实施规则

1. 为 provider error 构造一份安全的结构化类型，不要把原始 response body 直接放入 `Display` 后被上层统一记录。
2. 只解析服务商已知的错误字段；未知字段默认丢弃，或者只保留固定长度摘要。
3. 敏感字段 redaction 必须在所有出口共享，包括 tracing、用户可见错误、诊断包和 crash/panic 文本。
4. 在测试中用 fake sentinel 值断言日志里没有这些内容。
5. 生产日志级别默认为可诊断但不过度详细；如需用户主动启用 debug mode，要明确提示潜在信息范围，且仍不允许记录凭证。

## 6. `open_url` 与未来自定义 Provider Endpoint

当前 `open_url` 只有 `http` / `https` scheme 意图，但实际执行方式平台特定。修复时应使用 URL parser，而不是 shell 字符串。

若未来支持用户自定义 OpenAI-compatible endpoint 或本地模型 URL，必须单独做 SSRF/本地网络防护设计：

- 明确是否允许 `localhost`、loopback、RFC1918 私网、link-local 与 Unix domain socket；不要默认无限开放。
- 防 DNS rebinding、重定向到非允许网络、IPv4-mapped IPv6、非标准数值 IP 写法等常见绕过。
- 不允许任意 scheme；TLS 验证默认开启。
- 防止 API Key 跟随重定向发送到不同 origin；限制 response body 与请求时长。
- 自定义 endpoint 必须由用户明确配置，并在 UI 中说明文本发送对象。
- 不要将 endpoint 的认证 token 写入 log、SQLite debug 或导出诊断中。

## 7. CI / Release 供应链要求

- 对 release 最少权限化：仅上传资产的 job 拥有写权限；CI tests 与 manifest verify 通常不需要仓库写权限。
- 依赖审计高危结果应实际阻断发布，或者命中逐项审计、有负责人/时限的例外清单。不能用全步骤 `continue-on-error: true` 将真实风险永远变成提示。
- 评估 pin GitHub Actions 到 immutable commit SHA，并以 Dependabot/Renovate 等方式更新 pin。
- 使用 lockfile 可重复安装；升级依赖应单独审查 breaking changes，特别是 Tauri、`reqwest`、crypto 和系统平台依赖。
- 评估 `cargo audit`/`cargo deny`、npm audit 与 SBOM。工具加入前确认 false positive 与维护成本，不能让安全扫描变成永久红灯无人处理。
- 不在 workflow log 输出签名私钥；签名密钥只通过 secrets 注入；避免在 PR fork 场景向不可信代码暴露 signing secret。
- Release 必须验证 Windows/macOS 产物、签名与 `latest.json` 平台条目，不能只看某一个 job 绿色。

## 8. 威胁模型与边界

QuickTranslate 是用户本机软件。建议优先保护：

1. 用户无意中复制的敏感信息不被额外暴露到日志/历史；
2. provider credentials 不被 UI、错误字符串、日志和诊断包泄露；
3. 非法 URL 和未来自定义网络 endpoint 不触发不受控访问；
4. 外部 provider 返回的异常内容不通过日志/HTML 渲染引入脚本或数据外泄风险；
5. updater 与 release pipeline 的签名/权限/依赖风险得到可复核控制。

本项目不是系统级 sandbox。已被当前 OS 用户账户或同权限恶意进程完全控制时，不能保证它们无法读取进程内存/用户级文件；文档与 UI 不要夸大机器绑定加密的安全边界。
