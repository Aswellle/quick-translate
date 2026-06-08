// src-tauri/src/error.rs
// 统一错误类型，覆盖所有模块的错误场景

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ---- 翻译错误 ----
    #[error("网络连接失败：{0}")]
    NetworkError(String),

    #[error("API 认证失败：{provider}")]
    AuthError { provider: String },

    #[error("请求频率超限：{provider}")]
    RateLimit { provider: String },

    #[error("翻译额度已用尽：{provider}")]
    QuotaExhausted { provider: String },

    #[error("翻译请求超时（{timeout_secs}s）")]
    Timeout { timeout_secs: u64 },

    #[error("所有翻译源均不可用")]
    AllProvidersFailed { errors: Vec<(String, String)> },

    // ---- 输入错误 ----
    #[error("未检测到选中文本")]
    EmptyText,

    #[error("仅支持文本翻译")]
    NonTextContent,

    #[error("源语言与目标语言相同：{lang}")]
    SameLanguage { lang: String },

    // ---- 系统错误 ----
    #[error("剪贴板操作失败：{0}")]
    ClipboardError(String),

    #[error("数据库错误：{0}")]
    DatabaseError(String),

    #[error("数据库迁移失败：{message}")]
    DatabaseMigration { message: String },

    #[error("配置错误：{0}")]
    ConfigError(String),

    #[error("窗口操作失败：{0}")]
    WindowError(String),

    #[error("加密错误：{0}")]
    CryptoError(String),

    #[error("JSON 解析错误：{0}")]
    SerdeError(String),
}

// Tauri command 要求返回 Serialize 的错误，序列化为前端可消费结构
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", &self.error_code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

impl AppError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NetworkError(_) => "NETWORK_ERROR",
            Self::AuthError { .. } => "AUTH_ERROR",
            Self::RateLimit { .. } => "RATE_LIMIT",
            Self::QuotaExhausted { .. } => "QUOTA_EXHAUSTED",
            Self::Timeout { .. } => "TIMEOUT",
            Self::AllProvidersFailed { .. } => "ALL_PROVIDERS_FAILED",
            Self::EmptyText => "EMPTY_TEXT",
            Self::NonTextContent => "NON_TEXT_CONTENT",
            Self::SameLanguage { .. } => "SAME_LANGUAGE",
            Self::ClipboardError(_) => "CLIPBOARD_ERROR",
            Self::DatabaseError(_) => "DATABASE_ERROR",
            Self::DatabaseMigration { .. } => "DB_MIGRATION_FAILED",
            Self::ConfigError(_) => "CONFIG_ERROR",
            Self::WindowError(_) => "WINDOW_ERROR",
            Self::CryptoError(_) => "CRYPTO_ERROR",
            Self::SerdeError(_) => "SERDE_ERROR",
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::DatabaseError(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::SerdeError(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            AppError::Timeout { timeout_secs: 5 }
        } else {
            AppError::NetworkError(e.to_string())
        }
    }
}
