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
}

impl TranslationRecord {
    /// 从翻译结果构造历史记录
    pub fn from_result(result: &TranslationResult, source_text: &str, target_lang: &str) -> Self {
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
        }
    }
}

/// 历史记录查询参数（前端 → Rust）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub search: Option<String>,     // FTS5 搜索关键词
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
}

/// 翻译结果事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResultPayload {
    pub result: TranslationResult,
}

/// 翻译错误事件 payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationErrorPayload {
    pub code: String,
    pub message: String,
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
        }
    }
}

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
}

impl ToastPayload {
    pub fn error(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "error".into(),
            duration: None,
        }
    }
    pub fn warning(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "warning".into(),
            duration: None,
        }
    }
    pub fn success(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "success".into(),
            duration: None,
        }
    }
    pub fn info(message: impl Into<String>) -> Self {
        ToastPayload {
            message: message.into(),
            kind: "info".into(),
            duration: None,
        }
    }
}
