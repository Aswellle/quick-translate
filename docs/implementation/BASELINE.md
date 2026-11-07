# 审计整改基线（BASELINE）

生成时刻：2026-10-27（本地时间）

## Git 状态

- 分支：`main`，与 `origin/main` 同步
- HEAD：`3df3d1e331fef15d2d1e7e90a06084d3466fc9ca`（chore: 版本号提升至 0.8.0 (#72)）
- 最近 5 次提交：
  - `3df3d1e` chore: 版本号提升至 0.8.0 (#72)
  - `e2d1b98` fix: 修复玻璃浮窗内次级文字与 hover 提示的可读性 (#71)
  - `7f2272e` feat: 浮窗无障碍、macOS 外观同步与降低透明度响应 (#70)
  - `87fa444` feat: 接入 macOS 原生 Liquid Glass 并分离玻璃层与阅读层 (#69)
  - `7d10376` Merge pull request #68 from Aswellle/feat/stats-dashboard-and-popup-polish

## 版本基线

- `package.json`: 0.8.0
- `src-tauri/tauri.conf.json`: 0.8.0
- `src-tauri/Cargo.toml`: 0.8.0

与 AUDIT_FINDINGS.md 声明的基线（`main` / `0.8.0` / `3df3d1e3`）**一致**，审计发现可直接对当前源码逐条核对。

## 工作区状态

- 未跟踪：`.zcodeignore`、`docs/implementation/`（审计文档包，用户提供）、`quick-translate-agent-dev-docs/`
- 已跟踪文件无任何未提交修改 —— 不存在被覆盖风险

## 基线检查结果（改动前）

| 命令 | 结果 |
|---|---|
| `npm ci` | PASS（依赖安装成功） |
| `npm test` | PASS（5 个测试文件 / 33 个测试全部通过） |
| `npm run build` | PASS（tsc + vite 构建成功） |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | PASS（exit 0） |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | 见 PROGRESS.md 基线记录（后台运行） |
| `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` | 见 PROGRESS.md 基线记录（后台运行） |

## 平台环境

- 当前机器：Windows 10（win32 10.0.22631 x64）
- 可执行：npm 全套、cargo 全套（Windows target）
- 不可执行：macOS 实机测试（QT-001 / QT-002 / QT-004 的 macOS 人工验证路径）

## 审计发现与当前源码的核对结论

逐条核对在进入每个 QT ID 的修复前进行并记录于 VERIFICATION_MATRIX.md。
初步核对（2026-10-27）：

- QT-003：成立。`coordinator.rs` 中 result/error 走 `run_if_current`，但 `execute_at_position` 调用的 `show_popup_loading`（translation_flow.rs:199-254）emit loading 前无任何代际检查。
- QT-002：成立。`absorb_current`（worker.rs:66-78）在 `seq() == None` 时返回 `(None, None)`；`is_new_content` 对 `last_text == None` 恒返回 `true`（worker.rs:106）。
- QT-001：成立。`get_cursor_position`（clipboard/mod.rs:85-90）macOS 分支返回 `(0.0, 0.0)` 并留有 TODO。
- QT-009：成立。`normalize_text`（clipboard/mod.rs:44-70）两种分支都以 `split_whitespace()` 折叠空白。

## 基线 clippy 既有失败记录（2026-10-27 补充）

`cargo clippy --lib --tests -- -D warnings` 在基线即有 13 个错误（工作树 3df3d1e 实测）：
cache.rs `len_without_is_empty`、translator/mod.rs 测试 7 处 `cloned_ref_to_slice_refs`、
error.rs、popup_geometry.rs 常量断言等。均为基线既有，不属于 14 项 QT 范围。
审计要求的门禁命令 `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
（lib 目标）在基线与整改后均为绿。
