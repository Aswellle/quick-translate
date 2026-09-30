// src-tauri/src/error.rs
// 统一错误类型，覆盖所有模块的错误场景

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // ---- 翻译错误 ----
    #[error("网络连接失败：{0}")]
    NetworkError(String),

    /// 无法建立 TCP/TLS 连接（含 DNS 解析失败）。reqwest 的 is_connect 统一归这里：
    /// 离线的典型信号，与「请求发出去但出问题」在诊断上要分开（计划第 14 节）。
    #[error("网络连接失败（无法连接服务器）：{0}")]
    ConnectError(String),

    /// 响应体/协议层问题（reqwest is_body / is_decode）。
    #[error("服务响应异常：{0}")]
    ProtocolError(String),

    #[error("API 认证失败：{provider}")]
    AuthError { provider: String },

    #[error("请求频率超限：{provider}")]
    RateLimit {
        provider: String,
        /// 服务端 Retry-After 指示的等待秒数（HTTP 429 响应头）。
        /// 有值时熔断冷却取 max(常规曲线, retry_after)，计划第 8 节。
        retry_after_secs: Option<u64>,
    },

    #[error("翻译额度已用尽：{provider}")]
    QuotaExhausted { provider: String },

    #[error("翻译请求超时（{timeout_secs}s）")]
    Timeout { timeout_secs: u64 },

    /// 翻译源以 4xx 拒绝了这次请求（401/403/408/429 已归入更具体的变体）。
    ///
    /// 与 NetworkError 分开是必要的：此前 4xx 也被折进 NetworkError，于是
    /// 「语种不支持」这类请求本身的问题会被当成 provider 故障 ——
    /// 撞够三次就把一个完全正常的翻译源熔断十分钟。
    #[error("翻译源拒绝了该请求：{provider}（HTTP {status}）")]
    ProviderRejected { provider: String, status: u16 },

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

    /// 本地存储（SQLite）在启动时初始化失败 —— 进程继续运行（计划第 26 节），
    /// 历史与缓存降级：读空、写拒绝。翻译主链路不受影响。
    #[error("本地存储暂不可用，历史记录功能已降级")]
    StorageUnavailable,

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
            Self::ConnectError(_) => "CONNECT_ERROR",
            Self::ProtocolError(_) => "PROTOCOL_ERROR",
            Self::AuthError { .. } => "AUTH_ERROR",
            Self::RateLimit { .. } => "RATE_LIMIT",
            Self::QuotaExhausted { .. } => "QUOTA_EXHAUSTED",
            Self::Timeout { .. } => "TIMEOUT",
            Self::ProviderRejected { .. } => "PROVIDER_REJECTED",
            Self::AllProvidersFailed { .. } => "ALL_PROVIDERS_FAILED",
            Self::EmptyText => "EMPTY_TEXT",
            Self::NonTextContent => "NON_TEXT_CONTENT",
            Self::SameLanguage { .. } => "SAME_LANGUAGE",
            Self::ClipboardError(_) => "CLIPBOARD_ERROR",
            Self::DatabaseError(_) => "DATABASE_ERROR",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
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
        // 计划第 14 节：reqwest::Error 必须按性质细分，而不是全部折进
        // 一个 NetworkError —— 离线（连不上）与「服务器回了个怪东西」
        // 在日志、诊断与健康统计上应当可辨。
        if e.is_timeout() {
            AppError::Timeout { timeout_secs: 5 }
        } else if e.is_connect() {
            // 连接建立失败：TCP/DNS/TLS 握手都在这一层
            AppError::ConnectError(e.to_string())
        } else if e.is_body() || e.is_decode() {
            AppError::ProtocolError(e.to_string())
        } else {
            AppError::NetworkError(e.to_string())
        }
    }
}
