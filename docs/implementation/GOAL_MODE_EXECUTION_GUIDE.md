# QuickTranslate：AI Agent Goal 模式长程执行指南

> 目标：让 Codex 或 Claude Code 以可审计的方式持续修复 QuickTranslate 当前审计范围内的问题，直到达到明确的完成条件，而不是做到一半就只给计划或总结。本文与 `AGENT_MASTER_PROMPT.md`、其余审计文档配套使用。

## 1. Goal 模式不等于“无限自动运行”

Goal 模式把**完成条件**设为持续目标，并在每一轮结束时评估是否已经满足。它不会自动扩大工具权限、解决所有外部阻塞，也不应被用来追求“把项目做到完美”这种没有边界的目标。目标必须同时规定：交付结果、可检查证据、不得破坏的约束，以及真正无法继续时怎么停。

本仓库建议将 Goal 范围限定为：

- 完成 `AUDIT_FINDINGS.md` 中 QT-001 至 QT-014 的代码级修复、回归测试和必要文档/CI 更新。
- 按 `IMPLEMENTATION_PLAN.md` 完成相应实施阶段与最终回归检查。
- 将 `SECURITY_PRIVACY.md`、`ARCHITECTURE_AND_DECISIONS.md`、`TEST_PLAN.md` 中与本轮缺陷直接相关的约束落实到实现中。
- **不自动纳入** `FEATURE_ROADMAP.md` 中的所有可选新功能（例如 OCR、在线同步、AI 改写或大量新 Provider）。它们是独立产品范围，应在审计整改验收后另开 Goal。

## 2. 启动前准备

1. 在仓库根目录解压整套文档，例如：

   ```text
   docs/implementation/quick-translate-agent-dev-docs/
   ```

2. 确认以下文件均能被 Agent 读取：
   - `README.md`
   - `AUDIT_FINDINGS.md`
   - `IMPLEMENTATION_PLAN.md`
   - `ARCHITECTURE_AND_DECISIONS.md`
   - `TEST_PLAN.md`
   - `SECURITY_PRIVACY.md`
   - `FEATURE_ROADMAP.md`
   - `AGENT_MASTER_PROMPT.md`
   - `GOAL_MODE_EXECUTION_GUIDE.md`

3. 从**仓库根目录**启动 Codex / Claude Code。先确认当前分支、HEAD 与工作区是否存在用户修改；绝不能 reset、checkout、stash、覆盖或清理用户已有变更。
4. 在当前 Agent 会话中确认可使用 `/goal`。如果是 Claude Code，使用 `/goal`；如果是 Codex，在支持 Goal 的当前客户端/CLI/IDE 会话中使用 `/goal`。具体命令可随版本更新，以产品内命令帮助为准。
5. 提供充足的读取、编辑和测试权限，但不要为了无人值守而自动批准一切。网络写操作、发布、推送、删除用户数据、读取真实密钥等不属于本 Goal 授权范围。

## 3. 推荐的 Goal 提示词

在仓库根目录的当前会话中，把下面整段作为 `/goal` 的目标条件。目标文本既是开始任务的指令，也是完成判断依据；不用先让 Agent 单独输出一份计划，然后就停在那里。

```text
/goal 完成当前 QuickTranslate 仓库的审计整改任务，直到 AUDIT_FINDINGS.md 中 QT-001 至 QT-014 全部得到可验证的处理，并完成 IMPLEMENTATION_PLAN.md 中对应阶段与最终回归。开始前重新核对当前 checkout，不把旧审计当作当前代码事实。先读取 docs/implementation/quick-translate-agent-dev-docs/ 下所有 Markdown（特别是 AGENT_MASTER_PROMPT.md、AUDIT_FINDINGS.md、IMPLEMENTATION_PLAN.md、ARCHITECTURE_AND_DECISIONS.md、TEST_PLAN.md、SECURITY_PRIVACY.md、FEATURE_ROADMAP.md 和本指南）；若文档实际放在别处，先用仓库搜索确定位置。不要只写计划：必须修改代码、添加能证明旧逻辑失败/新逻辑通过的测试、运行检查，并迭代修复失败。

执行要求：先记录 git status --short --branch、git rev-parse HEAD、git log -5 --oneline 与版本基线；不覆盖任何用户未提交修改。建立并持续更新 docs/implementation/PROGRESS.md（每个 QT ID 标记 NOT_STARTED / IN_PROGRESS / FIXED_NEEDS_TEST / VERIFIED / EXTERNAL_VALIDATION_REQUIRED / BLOCKED）、BASELINE.md、VERIFICATION_MATRIX.md、DECISIONS.md。每次迭代先读这些交接文件与最新 diff，优先完成下一项未 VERIFIED 的任务。按实施计划先处理请求代际竞态、剪贴板去重与 app-write 竞态、macOS 光标坐标、跨平台打开外链、Provider 凭证验证契约，再处理存储健康、语言映射、文本保真、批量配置一致性、错误脱敏、长请求风险及 CI/release 安全门禁。顺序可基于当前源码证据调整，但必须记录理由并覆盖全部 QT IDs。

必须遵守：局部最小改动；先构造回归测试再修复；不重写现有架构；不做无关格式化或依赖升级；不读取/输出真实凭证、剪贴板正文或译文；不把敏感上游 response body 原样写进日志；不伪造测试结果；不忽略失败测试；不以新增注释代替修复；不擅自推送、发版、打 tag、合并 PR 或修改 GitHub 设置。网络真实 Provider 测试必须显式 opt-in。不得为满足清单而作没有证据的脆弱修复。

每项都执行“证据确认 → 回归测试（证明旧行为失败）→ 最小修复 → 相关测试 → 相关 lint/build → diff review → 更新进度/验证矩阵”。实际运行并报告 npm test、npm run build、cargo fmt --manifest-path src-tauri/Cargo.toml -- --check、cargo test --manifest-path src-tauri/Cargo.toml --lib、cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings；按可用环境补运行平台构建与安全审计。持续迭代解决可由当前环境解决的问题，不要在完成单个阶段或输出阶段报告后自行停止。

当环境不支持某平台、必须拥有硬件/服务商凭证/人工授权，或出现没有合理安全替代方案的真实阻塞时，先尝试可行的自动化、mock、静态检查与 CI 路径；仍无法验证时，标记 EXTERNAL_VALIDATION_REQUIRED 或 BLOCKED，记录证据、尝试过的路径、准确的阻塞原因和具体人工验证步骤，并继续其他未阻塞任务。不得把 NOT RUN 写成 PASS，也不得把平台未实测的实现宣称为完全跨平台验证。若连续迭代没有带来代码、测试或证据进展，先分析根因、改变验证/实现策略；不要重复相同命令或陷入无限重试。

完成条件：QT-001 至 QT-014 每项均为 VERIFIED，或仅在当前机器无法执行真实平台验证时为 EXTERNAL_VALIDATION_REQUIRED，且具有对应测试/证据和手工复核步骤；所有当前环境可运行的相关测试、格式检查、lint、build 均通过；不存在由本次修改引入的未解释失败、未经说明的忽略测试、敏感数据泄露或不相关变更；文档与实现一致；VERIFICATION_MATRIX.md、PROGRESS.md、BASELINE.md、DECISIONS.md 与最终 diff 互相一致。最后做一次独立的完整 diff/安全审查，报告修改文件、每个 QT ID 的结论、测试命令和真实结果、未运行的跨平台检查与剩余风险。只有完成这些检查后才可以结束 Goal；若仍有可在当前环境解决的未完成事项，继续修复，不要只给总结。
```

### 重要的范围说明

上面的 Goal 把“审计整改范围完成”作为终点，而不是把所有未来产品设想都塞进同一次长任务。若你确实希望在审计整改后继续开发某个产品功能，最好在第一个 Goal 验收后另建 Goal，并为新功能写单独 PRD、验收标准和风险约束。

## 4. 为什么要额外维护进度文件

Goal 状态保存在当前会话/线程中，但长任务仍可能遭遇上下文压缩、客户端退出、限额、构建失败或平台不匹配。可持续交接的事实应写入仓库，而不是只留在聊天记录。

建议 `docs/implementation/` 下维护：

| 文件 | 规则 |
|---|---|
| `PROGRESS.md` | 逐项记录 QT ID、状态、实现提交/文件、最新结果、下一步；不能只写“整体进度 80%”。 |
| `BASELINE.md` | 保存开始时的分支、HEAD、脏文件、版本、原始测试结果；不能把既有失败算成本轮引入。 |
| `VERIFICATION_MATRIX.md` | 每个验收要求对应测试命令、实测平台、证据位置、PASS/FAIL/NOT RUN。 |
| `DECISIONS.md` | 记录有影响的架构决策、采用/拒绝的修复方案、理由和未解决风险。 |

状态含义：

- `NOT_STARTED`：未开始。
- `IN_PROGRESS`：正在排查或实现。
- `FIXED_NEEDS_TEST`：代码已改，但证据链不完整，不算完成。
- `VERIFIED`：所需回归测试及相关验证已通过。
- `EXTERNAL_VALIDATION_REQUIRED`：代码级验证已完成，但需要特定 OS、真机或外部权限才能完成最终验证。
- `BLOCKED`：存在当前无法安全解决的实际阻塞，已记录原因及需要的输入。

只有 `VERIFIED` 可以计入“已验证完成”。外部验证项必须在最终报告里明确列出；若要求正式发布级的 Windows/macOS 完全验证，就必须随后在对应平台完成验证，不能用静态检查代替。

## 5. 每阶段要 Agent 产出的证据

每个 QT ID 至少记录：

1. **确认依据**：实际文件路径、函数/代码路径、为什么是 bug，以及审计基线是否仍适用。
2. **旧行为复现**：测试或可重复步骤，说明旧逻辑为什么失败。无法复现时先记录不确定性，不能假装确认。
3. **修改摘要**：改了什么、为何不扩大改动范围、相关 IPC/配置/数据迁移是否同步。
4. **验证结果**：完整命令、退出码、失败输出摘要、平台与日期。没有运行的项标 `NOT RUN`。
5. **反向审查**：检查异常路径、竞态、回滚、权限、隐私和是否引入新依赖。
6. **状态转移**：只有验证证据齐全后才能从 `FIXED_NEEDS_TEST` 改成 `VERIFIED`。

## 6. Goal 运行期间如何继续引导

不要每一轮都重复完整总控提示词。用简短指令让 Agent 回到事实文件和完成条件：

### 检查是否在真正推进

```text
请读取 PROGRESS.md、VERIFICATION_MATRIX.md 与最新 git diff。指出当前最早的未 VERIFIED QT ID、该项的下一步可执行动作与验证方式，然后直接执行，不要只给计划或复述状态。
```

### 修复失败测试

```text
不要跳过或放宽失败测试。读取完整失败输出，定位根因，确认是基线失败还是本轮回归，修复后重跑该测试及相关测试，并更新验证矩阵。继续推进下一个未 VERIFIED 项。
```

### 上下文已压缩或恢复会话

```text
恢复执行前先读取 AGENT_MASTER_PROMPT.md、GOAL_MODE_EXECUTION_GUIDE.md、PROGRESS.md、BASELINE.md、VERIFICATION_MATRIX.md、DECISIONS.md，再检查 git status 与最新 diff。不要依赖聊天摘要推断完成状态；从第一个未 VERIFIED 的 QT ID 继续，保持当前 Goal，不要重做已验证且未被新改动影响的任务。
```

### 接近结束时做独立验收

```text
现在停止新增功能，只做独立验收：逐条对照 AUDIT_FINDINGS.md、IMPLEMENTATION_PLAN.md 与 VERIFICATION_MATRIX.md，检查实现、回归测试、真实命令结果、敏感数据和完整 diff。所有仍可在当前环境解决的问题都要继续修复。平台未运行的检查必须明确列出，不得宣称 PASS。验收结束后输出 QT-001 至 QT-014 的逐项结论。
```

### 如何暂停与恢复

- 需要人工查看阶段成果：先暂停 Goal，再检查 `PROGRESS.md`、`VERIFICATION_MATRIX.md`、`git diff`；审查后恢复 Goal。
- 达到使用额度或关闭客户端前：要求 Agent 更新四份进度/验证文件，写清当前 QT ID、未完成动作和下一条验证命令，然后暂停。
- 恢复时在原线程恢复原 Goal，并粘贴上面的“上下文已压缩或恢复会话”提示；不要为了接着做而清除/重设成模糊目标。
- 如果 Agent 因真实错误、认证、额度、上下文溢出而清掉或暂停了 Goal，解决实际原因后重新运行同一 Goal 文本；确认当前状态文件后继续。

## 7. 安全、权限与停止边界

允许 Agent 自动进行的工作：读取相关源码/文档、创建测试、局部编辑、运行本地测试/lint/build、检查 diff、更新进度文件。

仍应保留人工控制的工作：访问真实凭证、对外发送用户数据、未审查的破坏性数据库迁移、删除用户文件、修改系统安全策略、远端 push、发布 release、打 tag、合并 PR、修改仓库权限，以及任何产生真实翻译配额/费用的 Provider 请求。

不建议把“全自动批准所有操作”当成 Goal 的前提。Goal 负责持续推进与判断完成条件，不会也不应该替代权限审查。若权限提示影响持续运行，优先精确批准必要的本地测试命令，而不是扩大整个工作区或系统的权限。

## 8. 何时才算本次审计整改完成

只有最终报告同时满足以下条件，才可以宣告“审计整改已完成”：

- QT-001 至 QT-014 有逐项结论和可追溯证据，未验证项没有被隐藏。
- 当前环境可运行的完整测试、构建、格式与 lint 门禁全部成功；失败均已修复或被证明是原始基线问题并说明。
- Windows/macOS 对应 CI 与必须的真实平台手测已完成；若尚未完成，准确称为“代码整改完成、跨平台验收未完成”，不要称为发布就绪。
- `git diff --check` 通过；完整 diff 中没有不相关变更、密钥、个人路径、大型生成产物或缺少解释的依赖更新。
- README、设置 UI、错误信息和代码实际行为一致；所有待办与阻塞写入文件并同步到最终报告。

## 9. 官方 Goal 资料

- [Codex：Using Goals in Codex（OpenAI Cookbook）](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex)
- [Claude Code：Keep Claude working toward a goal](https://code.claude.com/docs/en/goal)

不同版本的命令、自动继续与审批行为可能变化；开始任务时应检查当前客户端帮助与 Goal 状态。
