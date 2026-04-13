// src-tauri/src/domain/translator/google.rs
// Google Translate 非官方 Web API 翻译源实现（作为 DeepL 的 fallback）
// 包含：UA 轮换、响应格式兜底、超时自动重试（最多 2 次）

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

use super::TranslationProvider;
use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

const GOOGLE_TRANSLATE_URL: &str = "https://translate.googleapis.com/translate_a/single";

/// 轮换 User-Agent 池（降低单 UA 被限流的概率）
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
];

/// 最大重试次数（超时或 429 时）
const MAX_RETRIES: usize = 2;

pub struct GoogleProvider {
    http_client: Arc<HttpClient>,
    /// 简单计数器，用于 UA 轮换（无需原子，每次请求都通过 &self 调用）
    ua_index: std::sync::atomic::AtomicUsize,
}

impl GoogleProvider {
    pub fn new(http_client: Arc<HttpClient>) -> Self {
        GoogleProvider {
            http_client,
            ua_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 获取下一个轮换 UA
    fn next_ua(&self) -> &'static str {
        let idx = self.ua_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        USER_AGENTS[idx % USER_AGENTS.len()]
    }

    /// ISO 639-1 → Google 语言代码
    fn to_google_lang(lang: &str) -> &str {
        match lang {
            "zh" => "zh-CN",
            "zh-tw" => "zh-TW",
            other => other,
        }
    }

    /// 从 Google 嵌套 JSON 数组提取翻译文本
    /// 格式: [[[translated, original, ...], ...], null, detected_lang, ...]
    /// 兜底：尝试多种路径，任一成功即返回
    fn extract_translation(data: &Value) -> Option<(String, String)> {
        // 路径 1：标准格式 data[0][n][0]
        let translated = data
            .get(0)
            .and_then(|v| v.as_array())
            .map(|segments| {
                segments
                    .iter()
                    .filter_map(|seg| seg.get(0)?.as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .filter(|s| !s.is_empty());

        // 路径 2：data 本身是字符串（极少数情况）
        let translated = translated.or_else(|| data.as_str().map(|s| s.to_string()));

        let translated = translated?;

        // 检测语言位于 data[2]（可能为 null 或缺失，兜底为 "auto"）
        let detected_lang = data
            .get(2)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("auto")
            .to_string();

        Some((translated, detected_lang))
    }

    /// 实际执行单次 HTTP 请求
    async fn do_request(
        &self,
        text: &str,
        tl: &str,
    ) -> Result<(String, String, u64), AppError> {
        let ua = self.next_ua();
        let start = Instant::now();

        let response = self
            .http_client
            .client()
            .get(GOOGLE_TRANSLATE_URL)
            .header("User-Agent", ua)
            .query(&[
                ("client", "gtx"),
                ("sl", "auto"),
                ("tl", tl),
                ("dt", "t"),
                ("q", text),
            ])
            .send()
            .await
            .map_err(AppError::from)?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let status = response.status();

        match status.as_u16() {
            200 => {
                // 尝试解析 JSON，兜底处理非标准响应
                let body: Value = response.json().await.map_err(|e| {
                    AppError::NetworkError(format!("Google 响应解析失败: {}", e))
                })?;

                let (translated, detected) = Self::extract_translation(&body)
                    .ok_or_else(|| AppError::NetworkError("Google 返回空结果".to_string()))?;

                Ok((translated, detected, duration_ms))
            }
            429 => Err(AppError::RateLimit {
                provider: "google".into(),
            }),
            code => Err(AppError::NetworkError(format!(
                "Google Translate HTTP {}",
                code
            ))),
        }
    }
}

#[async_trait]
impl TranslationProvider for GoogleProvider {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
        _source_lang: Option<&str>,
    ) -> Result<TranslationResult, AppError> {
        let tl = Self::to_google_lang(target_lang);
        let mut last_err = AppError::NetworkError("未知错误".into());

        // 最多重试 MAX_RETRIES 次（超时或 429 触发重试）
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                // 重试前等待，时间随重试次数线性增长（500ms, 1000ms）
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
                tracing::warn!("Google Translate 第 {} 次重试", attempt);
            }

            match self.do_request(text, tl).await {
                Ok((translated_text, detected_lang, duration_ms)) => {
                    // 检查源语言与目标语言是否相同
                    let target_lower = target_lang.to_lowercase();
                    let detected_lower = detected_lang.to_lowercase();
                    if detected_lower == target_lower
                        || (detected_lower.starts_with("zh") && target_lower.starts_with("zh"))
                    {
                        return Err(AppError::SameLanguage { lang: detected_lang });
                    }

                    return Ok(TranslationResult {
                        translated_text,
                        detected_source_lang: detected_lang,
                        target_lang: target_lang.to_string(),
                        provider: "google".to_string(),
                        duration_ms,
                        truncated: false,
                    });
                }
                // 超时和限流可重试
                Err(e @ AppError::Timeout { .. }) | Err(e @ AppError::RateLimit { .. }) => {
                    last_err = e;
                    continue;
                }
                // 其他错误不重试
                Err(e) => return Err(e),
            }
        }

        Err(last_err)
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "google".to_string(),
            name: "Google Translate".to_string(),
            requires_api_key: false,
            is_available: true,
        }
    }

    async fn validate_credentials(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    fn update_api_key(&mut self, _api_key: String) {}
}
