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
}

impl DeepLProvider {
    pub fn new(http_client: Arc<HttpClient>, api_key: String) -> Self {
        DeepLProvider {
            http_client,
            api_key,
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

    /// 将 ISO 639-1 语言代码转换为 DeepL 格式（大写，zh → ZH）
    fn to_deepl_lang(lang: &str) -> String {
        match lang {
            "zh" => "ZH".to_string(),
            "zh-tw" => "ZH".to_string(),
            _ => lang.to_uppercase(),
        }
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
        _source_lang: Option<&str>,
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

                let detected_lang = translation
                    .detected_source_language
                    .to_lowercase();

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
            }),
            456 => Err(AppError::QuotaExhausted {
                provider: "deepl".to_string(),
            }),
            code => {
                let body = response.text().await.unwrap_or_default();
                Err(AppError::NetworkError(format!(
                    "DeepL API 错误 {}: {}",
                    code, body
                )))
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

    async fn validate_credentials(&self) -> Result<bool, AppError> {
        if self.api_key.is_empty() {
            return Ok(false);
        }

        // 发送一个极短文本的测试翻译
        match self.translate("hi", "zh", None).await {
            Ok(_) => Ok(true),
            Err(AppError::AuthError { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn update_api_key(&mut self, api_key: String) {
        self.api_key = api_key;
    }
}
