// src-tauri/src/infra/http_client.rs
// reqwest 客户端单例，统一超时、UA 配置
//
// 计划第 13/14 节：不同 Provider 的网络策略不应该完全一样。这里预建两档
// 客户端——官方 API（3s 连接超时）与 best_effort 兜底源（2s 连接超时）。
// 单次尝试的请求级超时仍由引擎按 RequestPolicy 用 tokio::time::timeout 施加；
// 客户端上的 5s timeout 只是永不越过安全上限。
// 连接超时是 reqwest 的客户端级配置（无法按请求覆盖），因此用两档预建客户端。

use std::time::Duration;

/// 封装 reqwest::Client，统一超时、UA 配置
pub struct HttpClient {
    /// 官方 API 档：3s 连接超时
    inner: reqwest::Client,
    /// best_effort 档：2s 连接超时（慢连接的兜底源不值得占用预算）
    fast_inner: reqwest::Client,
}

impl HttpClient {
    fn build(connect_timeout: Duration) -> reqwest::Client {
        reqwest::Client::builder()
            // 5s 请求超时（含连接 + 读取）——安全上限
            .timeout(Duration::from_secs(5))
            .connect_timeout(connect_timeout)
            // 设置 User-Agent，避免被 Google API 过滤
            .user_agent("Mozilla/5.0 (compatible; QuickTranslate/0.1)")
            // 使用 rustls（无需系统 OpenSSL）
            .use_rustls_tls()
            .build()
            .expect("HTTP 客户端初始化失败")
    }

    pub fn new() -> Self {
        HttpClient {
            inner: Self::build(Duration::from_secs(3)),
            fast_inner: Self::build(Duration::from_secs(2)),
        }
    }

    /// 获取内部 reqwest Client 引用（官方 API 档）
    pub fn client(&self) -> &reqwest::Client {
        &self.inner
    }

    /// best_effort 源（计划第 10 节标记）用这一档：连接更急促，
    /// 不让一个慢连接的非官方端点吃掉 8s 总预算。
    pub fn fast_client(&self) -> &reqwest::Client {
        &self.fast_inner
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}
