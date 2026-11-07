# 验证矩阵（VERIFICATION_MATRIX）

每个 QT ID 的：证据确认 → 回归测试 → 修复 → 验证命令与真实结果 → 剩余人工验证。

| ID | 当前源码证据确认 | 回归测试 | 修复位置 | 自动化验证结果 | 平台人工验证 |
|---|---|---|---|---|---|
| QT-001 | 成立 | `macos_scale_for_cursor` 6 个纯函数几何测试（f4878f1） | `clipboard/mod.rs` CGEvent 路线 | 262 lib tests 全绿；macOS 编译 NOT RUN（本机无 cc 工具链，CI macos-latest 可验） | 实机浮窗锚点 → EXTERNAL_VALIDATION_REQUIRED |
| QT-002 | 成立 | `close_does_not_retrigger_without_change_token` 等 3 个 worker 回归（bf9a689） | `clipboard/mod.rs` changeCount + `worker.rs` absorb 语义 | 51 clipboard tests 全绿 | Pasteboard changeCount 实测 → EXTERNAL_VALIDATION_REQUIRED |
| QT-003 | 成立 | `stale_show_cannot_commit_loading_after_window_ops` 等 6 个受控交错测试（6a928d2） | `coordinator.rs` commit_popup_show + `translation_flow.rs` 拆分 | coordinator 14 tests 全绿 | 不需要 |
| QT-004 | 成立（explorer 硬编码） | `validate_openable_url` 6 个校验测试（5cd0069） | `commands/system.rs` url crate + 平台分发 | 全绿 | macOS `open` 打开行为 NOT RUN |
| QT-005 | 成立（4 家全走 translate） | DeepL 4 个 mock-server HTTP 回归（443a74b） | `deepl.rs` usage 端点 + trait 契约 + UI/README | 全绿 | 真实服务商验证 NOT RUN（显式 opt-in 项） |
| QT-006 | 成立（腾讯/DeepL 折叠 zh-tw；腾讯 ko 误码） | 2×12 语言表驱动契约测试（95d9b50） | tencent.rs / deepl.rs | 全绿 | 翻译输出的实际语言变体属服务商行为，属人工项 |
| QT-007 | 成立（show 失败仍 return true） | Failed 路径契约测试（QT-003 组内） + submit_show（8c26e2a） | translation_flow.rs | 全绿 | 窗口 API 实际行为 NOT RUN（无实机） |
| QT-008 | 成立（清理失败仍报 Healthy） | 触发器故障注入 3 测试（07fe73a） | persistence.rs | 全绿 | 不需要 |
| QT-009 | 成立 | build_request_text 9 表格用例 + 角色分离断言（30332e1） | clipboard/mod.rs + worker.rs | 全绿 | 真实语料翻译质量属人工评估 |
| QT-010 | 成立（warn 后跳过、无条件 Ok） | 8 单测：allowlist/范围/整批拒绝/掩码过滤（12cf8a1） | commands/config.rs | 全绿 | auto_start 注册表行为 NOT RUN（Windows 手工） |
| QT-011 | 成立（body 进 NetworkError Display） | 3 哨兵测试 ×7 状态码（a52626b） | translator/mod.rs | 全绿 | 不需要 |
| QT-012 | 待验证→编码膨胀实证成立（CJK 9B/字、emoji 12B/字） | 5 测量/边界测试（2a625b7） | google.rs | 全绿 | 真实端点接受范围 NOT RUN → EXTERNAL_VALIDATION_REQUIRED |
| QT-013 | 成立（哈希-only 消费竞态，确定性推演成立） | 4 controller 单测 + 1 worker 全时序回归（退化即复现，519011e） | controller.rs + worker.rs | 全绿 | 真实剪贴板时序 NOT RUN（mock 已覆盖确定性部分） |
| QT-014 | 成立（write 全局授予、audit 静默、tag 引用） | 门禁脚本本地双向验证 + YAML 解析（72b49c4） | release.yml | 全绿 | 真实 release 运行 NOT RUN（下次发版验证） |

## 平台构建验证（Windows x64，2026-10-27）

| 阶段 | 结果 |
|---|---|
| Rust release 编译（LTO, opt-level=z） | PASS —— `target/release/quicktranslate.exe`（7.1MB） |
| MSI 打包（WiX candle + light） | PASS —— `QuickTranslate_0.8.0_x64_en-US.msi`（6.6MB） |
| NSIS 打包 | NOT COMPLETED —— tauri bundler 下载 NSIS 3.13 工具链时 TLS 连接被环境网络切断（沙箱限制；同一打包在 CI release 流水线每次发版都成功） |
| macOS 构建 | NOT RUN —— 本机无 macOS 工具链（CI macos-latest job 覆盖） |

## 外部验证（EXTERNAL_VALIDATION_REQUIRED）汇总

| ID | 未验证内容 | 建议人工步骤 |
|---|---|---|
| QT-001 | macOS 实机光标→浮窗锚点（多屏/负坐标/缩放/边缘） | `npm run tauri dev`，在每台显示器四象限复制文本，核对浮窗锚点与光标一致、不被菜单栏/Dock 裁切 |
| QT-002 | macOS Pasteboard changeCount 实际行为 | macOS 上复制→弹出→关闭→不复制，确认 2 秒内不自动重现；再次复制同文本观察既定降级语义 |
| QT-001/002/014 | macOS 目标编译（本机无 cc 工具链交叉 check 不可行） | CI macos-latest job（cargo clippy/test/build 已覆盖编译验证） |
| QT-004 | macOS `open` 打开默认浏览器 | 设置→翻译源→前往控制台 |
| QT-005 | 真实服务商配额行为（DeepL usage 端点零配额声明） | 显式 opt-in：配置真实 DeepL Key 验证前后各查一次 /v2/usage |
| QT-006 | 翻译输出的实际语言变体 | 配置 zh-tw 目标，经 DeepL/腾讯分别翻译，人工核对输出为繁体 |
| QT-010 | auto_start 注册表实际写入 | Windows 设置页切换开机自启，任务管理器→启动项核对 |
| QT-012 | Google 端点真实 URI 限额（护栏阈值标定） | 显式 opt-in：以梯度长度真实调用 Google 源，标定 4xx 拐点 |
| QT-013 | 真实剪贴板时序（多进程竞争） | Windows 实机：复制→弹出→点复制原文，确认不二次弹窗 |
| QT-014 | 真实 release 运行（门禁在生产流水线的表现） | 下次发版观察 npm audit 门禁与权限收紧后的工作流 |
