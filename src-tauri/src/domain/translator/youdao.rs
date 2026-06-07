// src-tauri/src/domain/translator/youdao.rs
// 有道智云翻译 API（备用，通用翻译）
// API 文档：https://ai.youdao.com/DOCSIRMA/html/trans/api/wbfy/index.html

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::TranslationProvider;
use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

const YOUDAO_API: &str = "https://openapi.youdao.com/api";

pub struct YoudaoProvider {
    http_client: Arc<HttpClient>,
    app_key: String,
    app_secret: String,
}

impl YoudaoProvider {
    pub fn new(http_client: Arc<HttpClient>, app_key: String, app_secret: String) -> Self {
        YoudaoProvider {
            http_client,
            app_key,
            app_secret,
        }
    }

    fn to_youdao_lang(lang: &str) -> &str {
        match lang {
            "zh" => "zh-CHS",
            "zh-tw" => "zh-CHT",
            "en" => "en",
            "ja" => "ja",
            "ko" => "ko",
            "fr" => "fr",
            "de" => "de",
            "es" => "es",
            "ru" => "ru",
            "pt" => "pt",
            "it" => "it",
            "ar" => "ar",
            other => other,
        }
    }

    /// 有道 HMAC-SHA256 签名
    fn sign(&self, input: &str, curtime: &str, salt: &str) -> String {
        // 截断规则：q.len() > 20 时用 q[:10] + len + q[-10:]
        let truncated = if input.chars().count() > 20 {
            let chars: Vec<char> = input.chars().collect();
            let len = chars.len();
            let head: String = chars[..10].iter().collect();
            let tail: String = chars[len - 10..].iter().collect();
            format!("{}{}{}", head, len, tail)
        } else {
            input.to_string()
        };

        let sign_str = format!(
            "{}{}{}{}{}",
            self.app_key, truncated, salt, curtime, self.app_secret
        );

        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(sign_str.as_bytes());
        hex::encode(h.finalize())
    }
}

#[derive(Deserialize)]
struct YoudaoResponse {
    #[serde(rename = "errorCode")]
    error_code: String,
    translation: Option<Vec<String>>,
    #[serde(rename = "l")]
    lang_pair: Option<String>,
}

#[async_trait]
impl TranslationProvider for YoudaoProvider {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError> {
        if self.app_key.is_empty() || self.app_secret.is_empty() {
            return Err(AppError::AuthError {
                provider: "youdao".into(),
            });
        }

        let start = Instant::now();
        let target = Self::to_youdao_lang(target_lang);
        let salt = Uuid::new_v4().to_string();
        let curtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();
        let sign = self.sign(text, &curtime, &salt);

        let params = [
            ("q", text),
            ("from", "auto"),
            ("to", target),
            ("appKey", &self.app_key),
            ("salt", &salt),
            ("sign", &sign),
            ("signType", "v3"),
            ("curtime", &curtime),
        ];

        let response = self
            .http_client
            .client()
            .post(YOUDAO_API)
            .form(&params)
            .send()
            .await
            .map_err(AppError::from)?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !response.status().is_success() {
            return Err(AppError::NetworkError(format!(
                "有道翻译 HTTP {}",
                response.status().as_u16()
            )));
        }

        let resp: YoudaoResponse = response
            .json()
            .await
            .map_err(|e| AppError::NetworkError(format!("有道翻译响应解析失败: {}", e)))?;

        if resp.error_code != "0" {
            return match resp.error_code.as_str() {
                "101" | "102" | "103" | "104" => Err(AppError::AuthError {
                    provider: "youdao".into(),
                }),
                "207" => Err(AppError::RateLimit {
                    provider: "youdao".into(),
                }),
                "411" | "412" => Err(AppError::RateLimit {
                    provider: "youdao".into(),
                }),
                _ => Err(AppError::NetworkError(format!(
                    "有道翻译错误 {}",
                    resp.error_code
                ))),
            };
        }

        let items = resp
            .translation
            .ok_or_else(|| AppError::NetworkError("有道翻译返回空结果".into()))?;
        let translated = items.join("\n");

        // 从 lang_pair 提取源语言，格式如 "en2zh-CHS"
        let detected = resp
            .lang_pair
            .as_deref()
            .and_then(|s| s.split('2').next())
            .unwrap_or("auto")
            .to_string();

        let detected_norm = match detected.as_str() {
            "zh-CHS" => "zh",
            "zh-CHT" => "zh-tw",
            other => other,
        }
        .to_string();

        let target_lower = target_lang.to_lowercase();
        if detected_norm == target_lower {
            return Err(AppError::SameLanguage {
                lang: detected_norm,
            });
        }

        Ok(TranslationResult {
            source_text: text.to_string(),
            translated_text: translated,
            detected_source_lang: detected_norm,
            target_lang: target_lang.to_string(),
            provider: "youdao".to_string(),
            duration_ms,
            truncated: false,
        })
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "youdao".to_string(),
            name: "有道翻译".to_string(),
            requires_api_key: true,
            is_available: !self.app_key.is_empty() && !self.app_secret.is_empty(),
        }
    }

    async fn validate_credentials(&self) -> Result<bool, AppError> {
        if self.app_key.is_empty() || self.app_secret.is_empty() {
            return Ok(false);
        }
        match self.translate("hello", "zh", None).await {
            Ok(_) => Ok(true),
            Err(AppError::AuthError { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn update_api_key(&mut self, api_key: String) {
        // 格式 "app_key:app_secret"
        if let Some((k, s)) = api_key.split_once(':') {
            self.app_key = k.to_string();
            self.app_secret = s.to_string();
        }
    }

    fn update_credentials(&mut self, creds: HashMap<String, String>) {
        if let Some(k) = creds.get("youdao_app_key") {
            self.app_key = k.clone();
        }
        if let Some(s) = creds.get("youdao_app_secret") {
            self.app_secret = s.clone();
        }
    }
}
