# 术语表（Glossary）Provider 能力评估

> 目的：开发计划 Task 10 要求「先确定 Provider 是否支持 glossary 原生能力；原生能力优先」。本文记录 2026-11-08 的文档级核验结果，作为数据层与 Provider 接入方案的依据。
>
> 结论基准是**公开文档**，未使用真实凭证做 contract test。每条结论标注置信度；标「需复核」的条目在动代码前必须再次核对官方文档或 mock/真实请求。

## 结论汇总

| Provider | 原生 glossary | 应用方式 | 术语由谁管理 | 置信度 |
|---|---|---|---|---|
| DeepL | ✅ 支持 | `/v2/glossaries` 创建 glossary → 翻译请求带 `glossary_id` | 应用可通过 API 创建/删除（v2 无编辑，改条目 = 删旧建新） | 高（官方文档） |
| 腾讯云 TMT | ✅ 支持 | 文本翻译请求带术语库 ID（社区资料写作 `TermBaseId`，需复核参数名/形态） | 控制台创建术语库并维护条目；API 侧只引用 ID | 中（官方文档确认「接口可配置术语库 ID」，参数名未从 API 参考页确认） |
| 百度翻译开放平台 | ⚠️ 有限支持 | 通用文本翻译接口加 `needIntervene=1`（使用「我的术语库」） | 控制台维护术语；API 侧只是一个开关 | 中（官方接入文档提及，需复核） |
| 有道智云 | ✅ 支持 | 文本翻译请求带 `vocabId`（术语表 out_id） | 控制台上传术语表；API 侧只引用 ID | 中高（官方文档列出 `vocabId`） |
| Google Cloud Translation | ❌ 当前 Provider 不支持 | glossary 属 v3（Advanced）能力，需要项目/服务账号鉴权 | — | 高（v3 文档明确 Basic v2 无 glossary） |

## 分项说明

### DeepL

- 创建：`POST /v2/glossaries`，必填 `name`、`source_lang`、`target_lang`、`entries`、`entries_format`（`tsv` 默认，也支持 `csv`）。一个 glossary 绑定一个语言对。
- 更新：v2 只有创建 / 查询 / 删除，没有编辑接口 → 条目变化时删旧建新。
- 使用：翻译请求传 `glossary_id`；文本翻译时 `source_lang` 可省略，DeepL 自动检测并应用匹配的词典；文档翻译必须显式 `source_lang`。
- 限制：glossary 最大 10 MiB；单条原文/译文与名称最大 1024 UTF-8 字节；单账号最多 1000 个 glossary；泰语不支持。
- 创建后需检查 `ready` 标志，为 false 时等待后再用。
- 来源：<https://developers.deepl.com/docs/api-reference/glossaries>

**对本项目的含义**：DeepL 是唯一可以由应用端完整管理术语（增删改 → 同步 glossary）的现有 Provider。可实现「本地术语表 → DeepL glossary 同步 → 翻译时带 glossary_id」，无需用户去第三方控制台。

### 腾讯云 TMT

- 控制台可创建术语库（名称规则、语言方向、是否双向），支持单条添加与 CSV/XLSX 批量导入（≤500KB，大批量需等待生效）。
- 官方术语库文档明确「调用文本翻译接口时，在请求参数中配置术语库 ID」；社区资料写作 `TermBaseId`，但动态渲染的 API 参考页无法静态读取，**参数名与类型（单个 ID / ID 列表）需复核**。
- 腾讯另有独立的术语库管理 API（产品 664 / 1300，`CreateGlossary` 等），与 TMT 是否共用同一术语库体系**未确认**。
- 来源：<https://cloud.tencent.com/document/product/551/107926>（术语库使用）、<https://cloud.tencent.com/voc/tasks/13657>（社区确认可传术语库 ID）

**对本项目的含义**：可实现「控件填腾讯云术语库 ID → 翻译时带上」，但术语内容只能由用户在腾讯云控制台维护；应用内术语表无法直接同步。实现前必须核对官方 API 参数。

### 百度翻译开放平台

- 当前使用的通用文本翻译接口（`api.fanyi.baidu.com/api/trans/vip/translate`）支持 `needIntervene=1`：开通「我的术语库」后，控制台维护的术语会在翻译中生效（提交后约 10 分钟生效）。
- 千帆版本（另一套 API）有 `termIds` 参数（最多 10 个术语库），**与本项目当前接入的开放平台接口不是同一套**，不能混用。
- 来源：<https://api.fanyi.baidu.com/doc/23>、<https://ai.baidu.com/ai-doc/MT/4kqryjku9>

**对本项目的含义**：只能提供「启用控制台术语库」开关（`needIntervene=1`），应用内无法管理术语；需要明确告知用户术语在百度控制台维护。参数细节实施前复核。

### 有道智云

- 文本/网页翻译接口支持可选参数 `vocabId`（术语表 `out_id`），控制台上传术语表后引用；官方文档注明术语表主要支持英中互译，其他语言方向以控制台为准。
- 来源：<https://ai.youdao.com/DOCSIRMA/html/trans/api/wyfy/index.html>

**对本项目的含义**：同腾讯/百度——应用侧只能引用用户已有的术语表 ID，不能管理条目。

### Google Cloud Translation

- 当前 Provider 走 v2 Basic（API Key）。glossary 仅 v3 Advanced 支持（`glossary_config`），需要项目与 Google Cloud 鉴权（服务账号/ADC），并且 glossary 必须与模型同区域。
- 来源：<https://cloud.google.com/translate/docs/intro-to-v3>、<https://cloud.google.com/translate/docs/samples/translate-v3-translate-text-with-glossary>

**对本项目的含义**：当前架构下标记为「不支持术语表」。升级到 v3 属于鉴权模型变更（大型改动），不在术语表首轮范围内；不得用占位符伪装成原生支持。

## 对 Task 10 实施的设计约束

1. **应用内术语表只对 DeepL 真正生效**（应用创建/同步 glossary）。其余 Provider 的「术语表」是引用控制台术语库 ID/开关，UI 必须如实区分，不能让用户以为本地条目会生效。
2. **不支持的 Provider 显示「该服务商不支持术语表」**（计划允许的显式标记），绝不静默忽略本地术语。
3. **缓存键必须包含术语表指纹**（计划决策 4）：本地术语表内容 + 启用状态 + 目标 Provider 的 glossary 标识共同决定缓存键，否则改术语后仍返回旧译文。
4. **不做无差别字符串替换**（计划决策 1）；占位符保护方案只作为后续独立评估项，且必须先有「占位符被服务商改写」的测试。
5. **术语数据本地保存并纳入备份导出**（计划决策 5）；日志不得包含术语原文。
6. 大小写敏感与词边界：DeepL glossary 的匹配语义以官方为准（v2 tsv 每行原文/译文），UI 提供选项前需确认其实际生效范围，不能假装支持。

## 未决/待复核清单

- 腾讯 `TextTranslate` 的术语库参数名与形态（单 ID / ID 列表）。
- 腾讯术语库管理 API（产品 664/1300）是否可管理 TMT 控制台术语库。
- 百度 `needIntervene` 的适用套餐与生效延迟。
- 有道 `vocabId` 的支持语言方向与术语表大小限制。
- DeepL v2 glossary 的 `ready` 轮询策略与删除失败的重试语义。

