// src-tauri/src/infra/http_client.rs
// reqwest 客户端单例，统一超时、UA 配置

use std::time::Duration;

/// 封装 reqwest::Client，统一超时、UA 配置
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            // 5s 请求超时（含连接 + 读取）
            .timeout(Duration::from_secs(5))
            // 连接超时 3s
            .connect_timeout(Duration::from_secs(3))
            // 设置 User-Agent，避免被 Google API 过滤
            .user_agent("Mozilla/5.0 (compatible; QuickTranslate/0.1)")
            // 使用 rustls（无需系统 OpenSSL）
            .use_rustls_tls()
            .build()
            .expect("HTTP 客户端初始化失败");

        HttpClient { inner: client }
    }

    /// 获取内部 reqwest Client 引用
    pub fn client(&self) -> &reqwest::Client {
        &self.inner
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
