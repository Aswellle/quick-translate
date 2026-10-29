// src-tauri/src/infra/http_client.rs
// reqwest 客户端单例，统一超时、UA 配置
//
// 计划第 13/14 节：单次尝试的请求级超时由引擎按 RequestPolicy 用
// `tokio::time::timeout` 施加；客户端上的 5s timeout 只是永不越过安全上限。
//
// 此前这里预建「官方 3s / best_effort 2s」两档连接超时客户端。2026-10 的
// 实测日志证明 2s 连接超时是误报的根因：走代理的网络到 Google 冷启动
// TLS 握手普遍超过 2s，reqwest 的 connect 超时被 `From<reqwest::Error>`
// 归类为 Timeout 后计入熔断，三次就把可用的源锁进长冷却。两档自此合并
// 为一档（3s 连接超时），档位差异只保留在 RequestPolicy 的重试额度上。
//
// 连接池保温：翻译是典型的「间歇突发」负载，默认 90s 的空闲连接回收
// 意味着每次间隔稍久的翻译都要重新付一次 TLS 握手（恰恰是最容易超时
// 的那一次）。tcp_keepalive + 更长的 idle 保持让后续请求复用热连接。

use std::time::Duration;

/// 封装 reqwest::Client，统一超时、UA 配置
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    fn build() -> reqwest::Client {
        reqwest::Client::builder()
            // 5s 请求超时（含连接 + 读取）——安全上限
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .tcp_keepalive(Duration::from_secs(30))
            // 默认 90s：对翻译这种「几分钟来一发」的负载太短，
            // 每次都冷启动 TLS 正是超时误报的主要来源
            .pool_idle_timeout(Duration::from_secs(300))
            // 设置 User-Agent，避免被 Google API 过滤
            .user_agent("Mozilla/5.0 (compatible; QuickTranslate/0.1)")
            // 使用 rustls（无需系统 OpenSSL）
            .use_rustls_tls()
            .build()
            .expect("HTTP 客户端初始化失败")
    }

    pub fn new() -> Self {
        HttpClient {
            inner: Self::build(),
        }
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
