// src-tauri/src/domain/translator/deepl.rs
// DeepL Free API 翻译源实现

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

use super::TranslationProvider;
use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

const DEEPL_FREE_API: &str = "https://api-free.deepl.com/v2/translate";
const DEEPL_PRO_API: &str = "https://api.deepl.com/v2/translate";

pub struct DeepLProvider {
    http_client: Arc<HttpClient>,
    api_key: String,
    #[cfg(test)]
    usage_endpoint_override: Option<String>,
}

impl DeepLProvider {
    pub fn new(http_client: Arc<HttpClient>, api_key: String) -> Self {
        DeepLProvider {
            http_client,
            api_key,
            #[cfg(test)]
            usage_endpoint_override: None,
        }
    }

    /// 根据 API Key 格式判断使用 Free 还是 Pro 端点
    fn endpoint(&self) -> &str {
        if self.api_key.ends_with(":fx") {
            DEEPL_FREE_API
        } else {
            DEEPL_PRO_API
        }
    }

    /// 凭证校验端点：与翻译同主机的 `/v2/usage` 用量查询。
    ///
    /// DeepL 的用量接口只做认证与额度统计，不执行翻译 —— 这是官方
    /// 支持的零配额校验途径（QT-005）。认证方式与官方文档一致：
    /// `Authorization: DeepL-Auth-Key <key>` 请求头，密钥不进 URL。
    fn usage_endpoint(&self) -> String {
        self.endpoint().replace("/translate", "/usage")
    }

    /// 将 ISO 639-1 语言代码转换为 DeepL 格式（大写，zh → ZH）
    fn to_deepl_lang(lang: &str) -> String {
        match lang {
            "zh" => "ZH".to_string(),
            "zh-tw" => "ZH".to_string(),
            _ => lang.to_uppercase(),
        }
    }

    /// 测试专用：覆盖端点主机（指向本地 mock server）。
    /// 生产构造路径不变 —— 只有测试需要把请求指到 127.0.0.1。
    #[cfg(test)]
    fn with_usage_endpoint(mut self, url: String) -> Self {
        self.usage_endpoint_override = Some(url);
        self
    }
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    detected_source_language: String,
    text: String,
}

#[async_trait]
impl TranslationProvider for DeepLProvider {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError> {
        if self.api_key.is_empty() {
            return Err(AppError::AuthError {
                provider: "deepl".to_string(),
            });
        }

        let start = Instant::now();
        let target = Self::to_deepl_lang(target_lang);

        let params = [
            ("auth_key", self.api_key.as_str()),
            ("text", text),
            ("target_lang", &target),
        ];

        let response = self
            .http_client
            .client()
            .post(self.endpoint())
            .form(&params)
            .send()
            .await
            .map_err(AppError::from)?;

        let status = response.status();
        let duration_ms = start.elapsed().as_millis() as u64;

        match status.as_u16() {
            200 => {
                let body: DeepLResponse = response
                    .json()
                    .await
                    .map_err(|e| AppError::NetworkError(format!("DeepL 响应解析失败: {}", e)))?;

                let translation = body
                    .translations
                    .into_iter()
                    .next()
                    .ok_or_else(|| AppError::NetworkError("DeepL 返回空结果".to_string()))?;

                let detected_lang = translation.detected_source_language.to_lowercase();

                // 检查源语言与目标语言是否相同
                let target_lower = target_lang.to_lowercase();
                let detected_lower = detected_lang.as_str();
                if detected_lower == target_lower
                    || (detected_lower == "zh" && target_lower.starts_with("zh"))
                {
                    return Err(AppError::SameLanguage {
                        lang: detected_lang,
                    });
                }

                Ok(TranslationResult {
                    source_text: text.to_string(),
                    translated_text: translation.text,
                    detected_source_lang: detected_lang,
                    target_lang: target_lang.to_string(),
                    provider: "deepl".to_string(),
                    duration_ms,
                    truncated: false,
                })
            }
            403 => Err(AppError::AuthError {
                provider: "deepl".to_string(),
            }),
            429 => Err(AppError::RateLimit {
                provider: "deepl".to_string(),
                retry_after_secs: super::parse_retry_after_secs(response.headers()),
            }),
            456 => Err(AppError::QuotaExhausted {
                provider: "deepl".to_string(),
            }),
            code => {
                let body = response.text().await.unwrap_or_default();
                Err(super::http_status_error("deepl", code, body))
            }
        }
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "deepl".to_string(),
            name: "DeepL".to_string(),
            requires_api_key: true,
            is_available: !self.api_key.is_empty(),
        }
    }

    /// 校验凭证：查询用量接口而非执行翻译（QT-005）。
    ///
    /// DeepL 官方提供 `/v2/usage` 认证/用量端点 —— 凭证有效返回 200，
    /// 无效返回 401/403，全程不产生任何翻译字符消耗。旧实现用
    /// `translate("hi", "zh")` 校验，与「验证不消耗配额」的承诺直接矛盾。
    async fn validate_credentials(&self) -> Result<bool, AppError> {
        if self.api_key.is_empty() {
            return Ok(false);
        }

        #[cfg(test)]
        let url = self
            .usage_endpoint_override
            .clone()
            .unwrap_or_else(|| self.usage_endpoint());
        #[cfg(not(test))]
        let url = self.usage_endpoint();

        let response = self
            .http_client
            .client()
            .get(&url)
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .send()
            .await
            .map_err(AppError::from)?;

        match response.status().as_u16() {
            // 凭证有效（用量信息本身不需要展示给用户）
            200 => Ok(true),
            // 服务商明确拒绝：密钥无效/无权限 —— 与「无法判定」区分开
            401 | 403 => Ok(false),
            429 => Err(AppError::RateLimit {
                provider: "deepl".to_string(),
                retry_after_secs: super::parse_retry_after_secs(response.headers()),
            }),
            456 => Err(AppError::QuotaExhausted {
                provider: "deepl".to_string(),
            }),
            code => {
                // 与 translate 路径同样的脱敏规则：状态码进错误分类，
                // 响应体不进入常规日志（QT-011 统一处理）
                let body = response.text().await.unwrap_or_default();
                Err(super::http_status_error("deepl", code, body))
            }
        }
    }

    fn validation_uses_translation(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::thread;

    use super::*;

    /// 单请求 mock HTTP server：返回固定状态码与响应体，
    /// 并把收到的原始请求头交回测试断言（请求路径 + 认证头）。
    /// 手写 TcpListener 而非引入 wiremock —— 只需要一个请求的 canned 响应。
    fn spawn_mock(status: u16, body: &'static str) -> (String, thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test server accept");
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                if status == 200 { "OK" } else { "ERR" },
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            request
        });
        (format!("http://{}", addr), handle)
    }

    fn provider_at(mock_base: String) -> DeepLProvider {
        DeepLProvider::new(Arc::new(HttpClient::new()), "testkey:fx".to_string())
            .with_usage_endpoint(format!("{}/v2/usage", mock_base))
    }

    /// 核心回归（QT-005）：凭证校验必须打 /v2/usage 用量接口而不是
    /// /v2/translate —— 旧实现用 translate("hi") 校验，会真实消耗配额。
    #[test]
    fn validate_hits_usage_endpoint_with_auth_header() {
        let (url, server) = spawn_mock(200, r#"{"character_count":0,"character_limit":500000}"#);
        let provider = provider_at(url);

        let result = tauri::async_runtime::block_on(provider.validate_credentials());

        let request = server.join().unwrap();
        assert!(
            matches!(result, Ok(true)),
            "200 应判定为凭证有效，实际: {:?}",
            result
        );
        assert!(
            request.starts_with("GET /v2/usage"),
            "校验必须走用量接口，实际请求: {}",
            request.lines().next().unwrap_or("")
        );
        assert!(
            request.contains("authorization: deepl-auth-key testkey:fx")
                || request.to_lowercase().contains("deepl-auth-key testkey:fx"),
            "认证必须走请求头（密钥不进 URL）"
        );
        assert!(
            !request.contains("POST /v2/translate"),
            "校验路径绝不能出现翻译请求"
        );
    }

    /// 401/403 是服务商的明确拒绝 → 凭证无效（Ok(false)），不是网络错误。
    #[test]
    fn rejected_key_maps_to_invalid() {
        for status in [401u16, 403] {
            let (url, server) = spawn_mock(status, r#"{"message":"Wrong key"}"#);
            let provider = provider_at(url);
            let result = tauri::async_runtime::block_on(provider.validate_credentials());
            server.join().unwrap();
            assert!(
                matches!(result, Ok(false)),
                "{} 应判定为凭证无效，实际: {:?}",
                status,
                result
            );
        }
    }

    /// 429（限流）与 456（配额用尽）是「无法判定」而非「无效」：
    /// 必须返回结构化错误，界面才能区分「密钥错了」和「稍后再试」。
    #[test]
    fn rate_limit_and_quota_are_distinguishable_errors() {
        let (url, server) = spawn_mock(429, "{}");
        let provider = provider_at(url);
        let result = tauri::async_runtime::block_on(provider.validate_credentials());
        server.join().unwrap();
        assert!(
            matches!(result, Err(AppError::RateLimit { .. })),
            "429 必须映射为限流错误，实际: {:?}",
            result
        );

        let (url, server) = spawn_mock(456, "{}");
        let provider = provider_at(url);
        let result = tauri::async_runtime::block_on(provider.validate_credentials());
        server.join().unwrap();
        assert!(
            matches!(result, Err(AppError::QuotaExhausted { .. })),
            "456 必须映射为配额错误，实际: {:?}",
            result
        );
    }

    /// 契约声明：DeepL 的校验不走翻译（用量接口零配额），
    /// 其余 provider 默认声明走翻译。
    #[test]
    fn validation_method_declaration_is_honest() {
        let provider = DeepLProvider::new(Arc::new(HttpClient::new()), "k:fx".to_string());
        assert!(
            !provider.validation_uses_translation(),
            "DeepL 走用量接口，不得声明为消耗配额"
        );
    }
}
