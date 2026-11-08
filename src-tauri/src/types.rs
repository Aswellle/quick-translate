// src-tauri/src/types.rs
// 跨层共享的数据结构，用于 IPC 序列化与内部通信

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 翻译结果（Rust → 前端 via event）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub source_text: String, // 原始待翻译文本
    pub translated_text: String,
    pub detected_source_lang: String, // ISO 639-1
    pub target_lang: String,
    pub provider: String, // 实际使用的翻译源
    pub duration_ms: u64,
    pub truncated: bool, // 是否因超长被截断
}

/// 翻译源元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,   // "deepl" | "google"
    pub name: String, // "DeepL" | "Google Translate"
    pub requires_api_key: bool,
    pub is_available: bool, // 是否已配置且可用
}

/// 翻译源运行时健康状态（用于托盘菜单与诊断）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub requires_api_key: bool,
    pub is_available: bool,
    pub health_state: String, // "healthy" | "degraded" | "open" | "half_open" | "unknown"
    /// 最近一次错误的类别："transient" | "rate_limit" | "auth" | "quota" |
    /// "permanent" | "same_language" | null。设置页据此把「暂时不可用」
    /// 细分成限流 / 认证失败 / 额度用尽（计划第 28 节）。
    pub last_error_class: Option<String>,
}

/// 历史记录来源（计划 Task 09：手动输入与剪贴板自动触发可区分，
/// 但走同一翻译核心与落盘管道）。
pub const RECORD_SOURCE_CLIPBOARD: &str = "clipboard";
pub const RECORD_SOURCE_MANUAL: &str = "manual";

/// 历史记录条目（DB → 前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRecord {
    pub id: String,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub provider: String,
    pub created_at: i64, // Unix ms
    pub duration_ms: Option<i64>,
    pub is_starred: bool,
    /// 请求来源："clipboard"（自动触发）| "manual"（用户手动发起）。
    /// v7 之前的历史行由迁移统一置为 clipboard。
    pub source: String,
}

impl TranslationRecord {
    /// 从翻译结果构造历史记录
    pub fn from_result(
        result: &TranslationResult,
        source_text: &str,
        target_lang: &str,
        source: &str,
    ) -> Self {
        TranslationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            source_text: source_text.to_string(),
            translated_text: result.translated_text.clone(),
            source_lang: result.detected_source_lang.clone(),
            target_lang: target_lang.to_string(),
            provider: result.provider.clone(),
            created_at: now_unix_ms(),
            duration_ms: Some(result.duration_ms as i64),
            is_starred: false,
            source: source.to_string(),
        }
    }
}

/// 历史记录查询参数（前端 → Rust）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub search: Option<String>,     // LIKE 子串搜索关键词
    pub limit: i64,                 // 默认 50
    pub offset: i64,                // 分页偏移
    pub starred_only: Option<bool>, // 仅显示收藏
}

impl Default for HistoryQuery {
    fn default() -> Self {
        HistoryQuery {
            search: None,
            limit: 50,
            offset: 0,
            starred_only: None,
        }
    }
}

/// 使用统计结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResult {
    pub total_records: u64,
    pub total_chars: u64,
    pub by_provider: HashMap<String, u64>,
    pub last_7_days: u64,
    pub last_30_days: u64,
    /// 近 24 小时翻译次数（仪表盘「最近 24h」卡片）
    pub last_24h: u64,
    /// 逐日翻译次数，按时间升序；末位是今天（部分）。
    /// 覆盖范围由 get_stats 的 days / get_stats_range 的区间决定
    /// （默认与历史上的 daily_7d 相同为近 7 天）。每日边界按本地时区
    /// 计算 —— 仪表盘趋势图的横轴。
    pub daily: Vec<DailyCount>,
}

/// 单日翻译计数（daily 的元素）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCount {
    /// 当日 0 点的 Unix 毫秒（本地时区）
    pub date_ms: i64,
    pub count: u64,
    /// 当日翻译的源字符总量（悬浮提示的详细数据）
    pub chars: u64,
}

/// 浮窗定位信息（Rust → 前端 event）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopupPosition {
    pub x: f64,
    pub y: f64,
    pub monitor_width: u32,
    pub monitor_height: u32,
}

/// 翻译 loading 事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationLoadingPayload {
    pub position: PopupPosition,
    /// 本次翻译请求的完整 UUID。前端据此做 latest-wins 二次校验
    /// （计划第 6 节）：迟到的结果事件直接丢弃，不进状态。
    pub request_id: String,
}

/// 翻译结果事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResultPayload {
    pub result: TranslationResult,
    /// 见 TranslationLoadingPayload.request_id
    pub request_id: String,
}

/// 翻译错误事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationErrorPayload {
    pub code: String,
    pub message: String,
    /// 见 TranslationLoadingPayload.request_id
    pub request_id: String,
}

/// 完整应用配置（用于设置面板）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub target_lang: String,
    pub provider: String,

    // ── 翻译源凭证（前端展示时脱敏）──
    pub deepl_api_key: String,
    pub tencent_secret_id: String,
    pub tencent_secret_key: String,
    pub baidu_app_id: String,
    pub baidu_secret_key: String,
    pub youdao_app_key: String,
    pub youdao_app_secret: String,

    pub auto_start: bool,
    pub history_limit: i64,
    pub theme: String,
    pub fallback_enabled: bool,
    /// 是否已完成首次引导设置向导
    pub onboarding_completed: bool,
    /// 是否启用剪贴板监控自动翻译
    pub clipboard_monitor_enabled: bool,
    /// 剪贴板敏感内容过滤开关（domain::clipboard_filter）。
    ///
    /// 默认开启：疑似凭证（私钥/JWT/云服务令牌等高置信度形态）在发送给
    /// 翻译服务之前被本地拦截。关闭意味着用户自行承担凭证外发风险，
    /// 设置页文案必须如实说明这一点。
    pub sensitive_filter_enabled: bool,
    /// 减少透明效果（应用内开关）。
    ///
    /// 为什么需要它：`prefers-reduced-transparency` 媒体查询在 WKWebView
    /// 上不受支持（WebKit bug 175497，Safari 26.0 仍未实现），macOS 上
    /// 前端读不到系统偏好，整条 reduce 链路会静默空操作。这个开关给
    /// 无法被媒体查询覆盖的平台一个确定性的回退。
    ///
    /// 语义是「叠加」而非「覆盖」：媒体查询命中时始终生效（那是用户的
    /// 系统级无障碍设置），本开关只是在它之外再提供一条应用内途径。
    pub reduce_transparency: bool,
    /// 历史记录导出的默认保存目录。空串 = 每次导出弹原生保存对话框；
    /// 非空 = 直接写入该目录（不再询问）。设置页可修改/清除。
    pub export_dir: String,
    /// 翻译结果的本地缓存开关（默认开启）。
    ///
    /// 关闭只暂停读写，不删除已有条目 —— 「清空缓存」仍是独立操作。
    /// 设为开关而不是只提供「清空」：不想在磁盘上留存译文副本的用户，
    /// 需要一个持续生效的选项，而不是每次翻译后手动清一次。
    pub cache_enabled: bool,
    /// 全局快捷键的加速键（空串 = 禁用该动作）。
    ///
    /// 语法由 tauri-plugin-global-shortcut 解析（如 "CmdOrControl+Alt+T"）。
    /// 这里只存字符串：合法性由注册结果裁决，注册失败会在设置页显示
    /// 真实原因且不影响应用启动。
    pub hotkey_translate_clipboard: String,
    pub hotkey_open_manual: String,
    pub hotkey_toggle_monitor: String,
    /// 术语表总开关（计划 Task 10）。
    ///
    /// 关闭时术语条目不参与任何翻译请求与缓存键；条目本身保留。
    /// 只有支持原生 glossary 的 Provider 会真正生效（当前为 DeepL），
    /// 其余 Provider 在设置页如实标记为不支持。
    pub glossary_enabled: bool,
    /// 百度通用接口：使用控制台「我的术语库」（请求参数 needIntervene=1）。
    /// 术语内容只能在百度控制台维护，应用只负责开关。
    pub baidu_use_term_base: bool,
    /// 有道智云控制台术语表 ID（请求参数 vocabId）。空串 = 不使用。
    pub youdao_vocab_id: String,
    /// 腾讯云控制台术语库 ID 列表（逗号分隔；请求参数 TermRepoIDList）。
    /// 空串 = 不使用。
    pub tencent_term_repo_ids: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            target_lang: "zh".to_string(),
            provider: "google".to_string(),
            deepl_api_key: String::new(),
            tencent_secret_id: String::new(),
            tencent_secret_key: String::new(),
            baidu_app_id: String::new(),
            baidu_secret_key: String::new(),
            youdao_app_key: String::new(),
            youdao_app_secret: String::new(),
            auto_start: false,
            history_limit: 200,
            theme: "system".to_string(),
            fallback_enabled: true,
            onboarding_completed: false,
            clipboard_monitor_enabled: true,
            sensitive_filter_enabled: true,
            reduce_transparency: false,
            export_dir: String::new(),
            cache_enabled: true,
            hotkey_translate_clipboard: "CmdOrControl+Alt+T".to_string(),
            hotkey_open_manual: "CmdOrControl+Alt+M".to_string(),
            hotkey_toggle_monitor: "CmdOrControl+Alt+P".to_string(),
            glossary_enabled: true,
            baidu_use_term_base: false,
            youdao_vocab_id: String::new(),
            tencent_term_repo_ids: String::new(),
        }
    }
}

/// 应用支持的语言代码（与前端 SUPPORTED_LANGUAGES 一致）。
///
/// 单一定义来源：配置校验（target_lang）与术语表语言对校验共用，
/// 避免两处清单漂移。
pub const SUPPORTED_LANGS: &[&str] = &[
    "zh", "zh-tw", "en", "ja", "ko", "fr", "de", "es", "ru", "pt", "it", "ar",
];

/// 获取当前 Unix 时间戳（毫秒）
pub fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Toast 通知 payload（Rust → 前端 event "toast"）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastPayload {
    pub message: String,
    /// "error" | "success" | "warning" | "info"
    pub kind: String,
    /// 显示时长（毫秒），默认 3000
    pub duration: Option<u32>,
    /// 可选动作按钮（目前只有敏感过滤拦截使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ToastAction>,
}

/// Toast 上的动作按钮：前端按 `id` 分发，`label` 是按钮文案。
///
/// 引入动作按钮的动机（计划 3.2 第 6 条）：过滤拦下内容后，用户必须有
/// 一条**显式**的「我知道风险，仍然翻译」路径 —— 否则过滤会变成无法
/// 绕过的高墙，用户只能关掉整个过滤开关。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastAction {
    /// 动作标识："translate_filtered"
    pub id: String,
    pub label: String,
}

impl ToastPayload {
    pub fn error(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "error".into(),
            duration: None,
            action: None,
        }
    }
    pub fn warning(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "warning".into(),
            duration: None,
            action: None,
        }
    }
    pub fn success(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "success".into(),
            duration: None,
            action: None,
        }
    }
    pub fn info(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "info".into(),
            duration: None,
            action: None,
        }
    }

    /// 带动作按钮的警告 toast（敏感过滤拦截的「仍然翻译」入口）。
    /// 停留时间拉长到 8 秒：这是需要用户阅读并决策的提示，不是一闪
    /// 而过的状态通知。
    pub fn warning_with_action(
        message: impl Into<String>,
        action_id: impl Into<String>,
        action_label: impl Into<String>,
    ) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "warning".into(),
            duration: Some(8000),
            action: Some(ToastAction {
                id: action_id.into(),
                label: action_label.into(),
            }),
        }
    }
}
