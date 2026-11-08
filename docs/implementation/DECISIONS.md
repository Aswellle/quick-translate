# 实施决策记录（DECISIONS）

记录偏离 IMPLEMENTATION_PLAN 建议顺序/方案的决定及理由。

## D-2026-10-27-01：执行顺序

按 IMPLEMENTATION_PLAN 推荐顺序执行：QT-003 → QT-002 → QT-001 → QT-004 → QT-005（Phase 1），随后 QT-007 → QT-008 → QT-006 → QT-009 → QT-010（Phase 2），最后 QT-011 → QT-013 → QT-012 → QT-014（Phase 3）。

理由：并发问题（QT-003）可纯逻辑锁定并有现成测试基建；QT-002 与 QT-001 同属 clipboard/平台层，相邻处理可共享 backend 契约改动；无证据支持需要调整顺序。

## D-2026-10-27-02：文档提交策略

`docs/implementation/` 整个目录在仓库中是**未跟踪**状态（用户放入的审计文档包）。整改代码提交只包含源码/测试/必要配置变更；PROGRESS.md、BASELINE.md、VERIFICATION_MATRIX.md、DECISIONS.md 保持未跟踪的工作文档，不混入代码提交。

## D-2026-10-27-03：分支策略

所有整改在本地分支进行，逐 ID 提交；不推送、不打 tag、不合并 PR（遵守任务约束）。基线分支为 `main` @ `3df3d1e`。

## D-2026-10-27-04：QT-001 坐标实现路线（CoreGraphics 而非 AppKit）

审计建议 NSEvent.mouseLocation，实际实现走 `CGEventGetLocation`：它直接返回 CG 全局显示空间坐标（主屏左上原点、单位点）——与 `CGDisplayBounds` 及 tao `Monitor::position()`（CG 点 × 该屏缩放比 = 物理像素）同一坐标系，**无需左下原点翻转**；且 CG 事件/显示查询线程安全，而 `NSScreen` 系列需要 `MainThreadMarker`，剪贴板 worker 线程拿不到。缩放比用 `CGDisplayMode pixel_width / CGDisplayBounds width` 推导（与 backingScaleFactor 同义）。坐标语义经 tao 0.37.1 / dpi 0.1.2 / tauri-runtime-wry 2.12.3 源码逐行考证（含 work_area 由 runtime-wry `monitor/macos.rs` 实现的事实）。

## D-2026-10-27-05：QT-002 吸收语义的取舍

无变化代数平台上「重复复制相同文本仍能触发」与「关闭就是关闭」互斥。按审计优先级选择后者：吸收分支无论有无代数都记录文本。代价是显式的：无代数平台同文本再次复制不触发（直到内容变化）。当前发行平台（Windows / macOS）都有代数，该降级仅影响理论上的其他平台。

## D-2026-10-27-06：QT-004 选择平台分发而非 opener 插件

审计允许两种路线。选择 explorer/open/xdg-open 分发 + url crate 校验：零新插件、零 capability 变更、diff 更小；安全边界等价（无 shell 拼接、scheme 白名单、单参数传递）。opener 插件留作未来统一收口选项。

## D-2026-10-27-07：QT-005 配额契约的诚实化形态

DeepL 用 `/v2/usage`（官方认证/用量端点，零配额）；腾讯/百度/有道无独立校验端点（官方文档核对），保留极短测试翻译但通过 `validation_uses_translation()` trait 契约 + 设置页提示 + README 修正如实声明。不再有任何「零配额」虚假承诺。

## D-2026-10-27-08：QT-010 副作用失败的补偿策略

副作用（引擎重建、托盘刷新、OS 自启动、事件广播）全部幂等，但持久化已发生、无法安全回滚。补偿 = 如实返回「已保存但 N 项生效失败（重试保存可恢复）」；用户重试保存即收敛。不伪装整体成功。

## D-2026-10-27-09：QT-012 阈值保守估计

7500 字节 ≈ 业界通用服务器/代理 URL 预算（8KB）减余量。真实端点精确接受范围需 opt-in 实测（未执行——约束禁止未授权真实 Provider 调用）。当前取值只保证不误伤正常长度（5000 ASCII ≈ 6.1KB 放行）；如实测允许更大预算可放宽。百度同样用 GET query 传文（baidu.rs），同类风险超出 QT-012 范围，记录为后续项。

## D-2026-10-27-10：QT-014 豁免清单与 SHA pin

当前 6 个高危公告全部是构建期传递依赖（browserslist×2、nanoid、source-map-js、undici、vite），不进入发行产物；依赖升级按审计要求留作独立 PR。SHA pin 覆盖 release.yml 全部 10 处引用（ci.yml 未动——审计范围是 release；后续可复制同模式）。

## D-2026-10-27-11：基线已有的 test-target clippy 告警

`cargo clippy --lib --tests -- -D warnings` 在基线 3df3d1e 就有 13 个错误（cache len/is_empty、测试代码 cloned_ref_to_slice 等），不属于 14 项 QT 的范围，未修（避免无关变更）。审计要求的门禁命令（lib 目标）全程绿。整改新增的测试代码不新增任何告警（新增 1 处已修）。
