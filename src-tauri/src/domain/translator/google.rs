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

/// Google 端点接受的最大 URL 字节数（QT-012）。
///
/// 该端点走 GET query，文本经 form 编码进 `q` 参数：CJK 每字编码后占
/// 9 字节、emoji 占 12 字节 —— 5000 个 CJK 字符（应用的全局上限）编码后
/// 仅 q 一项就 ≈45KB。业界通用的服务器/代理 URL 预算是 ~8KB，取 7500
/// 留出余量：超限请求在客户端快速失败（HTTP 414 语义），而不是把一个
/// 注定过长的请求发给服务器后得到含混的 4xx/413，再走完整个 fallback。
///
/// **阈值是保守估计**：真实端点的精确接受范围需要 opt-in 实测标定
/// （EXTERNAL_VALIDATION_REQUIRED），当前取值只保证不误伤正常长度。
const GOOGLE_MAX_URL_BYTES: usize = 7_500;

/// 轮换 User-Agent 池（降低单 UA 被限流的概率）
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
];

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
        let idx = self
            .ua_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        USER_AGENTS[idx % USER_AGENTS.len()]
    }

    /// 计算本次请求的最终 URL 字节数（QT-012）。
    ///
    /// 与 reqwest `.query(&[...])` 的序列化结果逐字节一致 —— 两者都用
    /// form-urlencoded 编码（serde_urlencoded 与 url crate 的
    /// Serializer 同源），因此这里算出的长度就是线上请求的真实长度。
    fn encoded_url_len(text: &str, tl: &str) -> usize {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("client", "gtx")
            .append_pair("sl", "auto")
            .append_pair("tl", tl)
            .append_pair("dt", "t")
            .append_pair("q", text)
            .finish();
        GOOGLE_TRANSLATE_URL.len() + 1 + query.len() // '?' + query
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
    async fn do_request(&self, text: &str, tl: &str) -> Result<(String, String, u64), AppError> {
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
                let body: Value = response
                    .json()
                    .await
                    .map_err(|e| AppError::NetworkError(format!("Google 响应解析失败: {}", e)))?;

                let (translated, detected) = Self::extract_translation(&body)
                    .ok_or_else(|| AppError::NetworkError("Google 返回空结果".to_string()))?;

                Ok((translated, detected, duration_ms))
            }
            429 => Err(AppError::RateLimit {
                provider: "google".into(),
                retry_after_secs: super::parse_retry_after_secs(response.headers()),
            }),
            code => Err(super::http_status_error("google", code, String::new())),
        }
    }
}

#[async_trait]
impl TranslationProvider for GoogleProvider {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError> {
        let tl = Self::to_google_lang(target_lang);

        // QT-012：GET query 承载全文 —— 编码后超长的请求在客户端快速失败。
        // 分类用 ProviderRejected（Permanent：输入的锅，不熔断健康的
        // Google，fallback 继续尝试 POST 型翻译源）；414 = URI Too Long。
        if Self::encoded_url_len(text, tl) > GOOGLE_MAX_URL_BYTES {
            return Err(AppError::ProviderRejected {
                provider: "google".to_string(),
                status: 414,
            });
        }

        // 这里**不再**自己重试。
        //
        // 此前本函数内置一轮最多 3 次的重试循环，而引擎在 Phase 5 也开始按
        // RequestPolicy 重试 —— 两者相乘会让一次 Google 请求实际打出 6 次
        // HTTP 调用，轻易吃掉整次翻译的时间预算。重试策略现在只有一个归属地：
        // 引擎的 RequestPolicy（best_effort 策略 max_attempts = 2，但
        // `should_retry` 只对 Transient 类生效 —— 429/限流仍然立刻换源，
        // 非公开接口被限流的风险不变）。
        let (translated_text, detected_lang, duration_ms) = self.do_request(text, tl).await?;

        // 检查源语言与目标语言是否相同
        let target_lower = target_lang.to_lowercase();
        let detected_lower = detected_lang.to_lowercase();
        if detected_lower == target_lower
            || (detected_lower.starts_with("zh") && target_lower.starts_with("zh"))
        {
            return Err(AppError::SameLanguage {
                lang: detected_lang,
            });
        }

        Ok(TranslationResult {
            source_text: text.to_string(),
            translated_text,
            detected_source_lang: detected_lang,
            target_lang: target_lang.to_string(),
            provider: "google".to_string(),
            duration_ms,
            truncated: false,
        })
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 审计 QT-012 验收：测量各长度档位的最终 URL 字节长度。
    ///
    /// 编码事实（form-urlencoded，UTF-8）：ASCII 1:1、CJK 9 字节/字、
    /// emoji 12 字节/字 —— 5000 个 CJK 字符的 q 参数一项就 ≈45KB。
    #[test]
    fn encoded_url_length_grows_with_multibyte_content() {
        let base = GoogleProvider::encoded_url_len("", "zh-CN");

        let l1 = GoogleProvider::encoded_url_len("a", "zh-CN");
        assert_eq!(l1, base + 1, "1 个 ASCII 字符占 1 字节");

        let l_ascii_200 = GoogleProvider::encoded_url_len(&"a".repeat(200), "zh-CN");
        assert_eq!(l_ascii_200, base + 200);

        let l_cjk_200 = GoogleProvider::encoded_url_len(&"中".repeat(200), "zh-CN");
        assert_eq!(l_cjk_200, base + 200 * 9, "每个 CJK 字符编码后占 9 字节");

        let l_emoji_200 = GoogleProvider::encoded_url_len(&"😀".repeat(200), "zh-CN");
        assert_eq!(
            l_emoji_200,
            base + 200 * 12,
            "每个 emoji（4 字节 UTF-8）编码后占 12 字节"
        );
    }

    /// 审计验收的核心测量：全局上限 5000 字符在不同字符集下的最终 URL 长度。
    /// ASCII 档安全（≈6KB），CJK 档（≈45KB）与 emoji 档（≈60KB）远超任何
    /// 服务器/代理的合理 URL 预算 —— 客户端护栏必须在请求发出前拦截。
    #[test]
    fn max_length_inputs_measure_as_documented() {
        let ascii = GoogleProvider::encoded_url_len(&"a".repeat(5000), "zh-CN");
        assert_eq!(ascii, GoogleProvider::encoded_url_len("", "zh-CN") + 5000);

        let cjk = GoogleProvider::encoded_url_len(&"中".repeat(5000), "zh-CN");
        assert!(
            cjk > 40_000,
            "5000 CJK 编码后应远超 8KB 常见预算，实测 {}",
            cjk
        );

        let emoji = GoogleProvider::encoded_url_len(&"😀".repeat(5000), "zh-CN");
        assert!(emoji > cjk, "emoji 编码膨胀更甚于 CJK");
    }

    /// 护栏：超限文本在进入网络层之前快速失败，分类为 Permanent
    /// （414 = URI Too Long）—— 不熔断健康的 Google，fallback 继续尝试
    /// POST 型翻译源。
    #[tokio::test]
    async fn over_limit_text_fails_fast_without_network() {
        let provider = GoogleProvider::new(Arc::new(HttpClient::new()));
        let cjk_5000 = "中".repeat(crate::system::translation::request::MAX_TEXT_CHARS);

        let err = provider
            .translate(&cjk_5000, "zh")
            .await
            .expect_err("超限文本必须被拦截");

        match err {
            AppError::ProviderRejected { provider, status } => {
                assert_eq!(provider, "google");
                assert_eq!(status, 414, "语义必须是 URI Too Long");
            }
            other => panic!("应分类为 ProviderRejected(414)，实际: {:?}", other),
        }
    }

    /// 边界：护栏只拦「编码后超限」的请求 —— 短文本（哪怕 5000 个
    /// ASCII 字符，≈6KB）必须放行到网络层。这里不真发请求：护栏判定
    /// 是纯函数，直接断言阈值两侧的判定结果。
    #[test]
    fn guard_boundary_matches_encoded_length() {
        let under = "a".repeat(5000); // ≈6.1KB < 7500
        assert!(
            GoogleProvider::encoded_url_len(&under, "zh-CN") <= GOOGLE_MAX_URL_BYTES,
            "5000 ASCII 应在预算内"
        );
        let over = "中".repeat(1000); // 9KB > 7500
        assert!(
            GoogleProvider::encoded_url_len(&over, "zh-CN") > GOOGLE_MAX_URL_BYTES,
            "1000 CJK（9KB 编码）应超预算"
        );
    }
}
