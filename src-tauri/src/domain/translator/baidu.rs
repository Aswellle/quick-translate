// src-tauri/src/domain/translator/baidu.rs
// 百度翻译 API（中文优化，通用版免费 5万字符/月，高级版 100万/月）
// API 文档：https://fanyi-api.baidu.com/doc/21

use async_trait::async_trait;
use md5::{Digest, Md5};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::TranslationProvider;
use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

const BAIDU_API: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";

pub struct BaiduProvider {
    http_client: Arc<HttpClient>,
    app_id: String,
    secret_key: String,
}

impl BaiduProvider {
    pub fn new(http_client: Arc<HttpClient>, app_id: String, secret_key: String) -> Self {
        BaiduProvider {
            http_client,
            app_id,
            secret_key,
        }
    }

    fn to_baidu_lang(lang: &str) -> &str {
        match lang {
            "zh" => "zh",
            "zh-tw" => "cht",
            "en" => "en",
            "ja" => "jp",
            "ko" => "kor",
            "fr" => "fra",
            "de" => "de",
            "es" => "spa",
            "ru" => "ru",
            "pt" => "pt",
            "it" => "it",
            "ar" => "ara",
            other => other,
        }
    }

    fn md5_sign(s: &str) -> String {
        let mut h = Md5::new();
        h.update(s.as_bytes());
        format!("{:x}", h.finalize())
    }
}

#[derive(Deserialize)]
struct BaiduResponse {
    from: Option<String>,
    trans_result: Option<Vec<BaiduTransItem>>,
    error_code: Option<String>,
    error_msg: Option<String>,
}

#[derive(Deserialize)]
struct BaiduTransItem {
    dst: String,
}

#[async_trait]
impl TranslationProvider for BaiduProvider {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError> {
        if self.app_id.is_empty() || self.secret_key.is_empty() {
            return Err(AppError::AuthError {
                provider: "baidu".into(),
            });
        }

        let start = Instant::now();
        let target = Self::to_baidu_lang(target_lang);

        // 百度签名：MD5(appid + q + salt + secret_key)
        let salt = format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let sign_str = format!("{}{}{}{}", self.app_id, text, salt, self.secret_key);
        let sign = Self::md5_sign(&sign_str);

        let params = [
            ("q", text),
            ("from", "auto"),
            ("to", target),
            ("appid", &self.app_id),
            ("salt", &salt),
            ("sign", &sign),
        ];

        let response = self
            .http_client
            .client()
            .get(BAIDU_API)
            .query(&params)
            .send()
            .await
            .map_err(AppError::from)?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !response.status().is_success() {
            return Err(AppError::NetworkError(format!(
                "百度翻译 HTTP {}",
                response.status().as_u16()
            )));
        }

        let resp: BaiduResponse = response
            .json()
            .await
            .map_err(|e| AppError::NetworkError(format!("百度翻译响应解析失败: {}", e)))?;

        if let Some(code) = resp.error_code {
            return match code.as_str() {
                "52001" => Err(AppError::Timeout { timeout_secs: 5 }),
                "52002" => Err(AppError::NetworkError("百度翻译系统错误".into())),
                "52003" => Err(AppError::AuthError {
                    provider: "baidu".into(),
                }),
                "54003" | "54004" => Err(AppError::RateLimit {
                    provider: "baidu".into(),
                }),
                "54005" => Err(AppError::RateLimit {
                    provider: "baidu".into(),
                }),
                _ => Err(AppError::NetworkError(format!(
                    "百度翻译错误 {}: {}",
                    code,
                    resp.error_msg.unwrap_or_default()
                ))),
            };
        }

        let items = resp
            .trans_result
            .ok_or_else(|| AppError::NetworkError("百度翻译返回空结果".into()))?;

        let translated: String = items
            .iter()
            .map(|i| i.dst.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let detected = resp.from.unwrap_or_else(|| "auto".to_string());

        // 百度的语言代码和我们的不同，做反向映射
        let detected_norm = match detected.as_str() {
            "zh" => "zh",
            "cht" => "zh-tw",
            "jp" => "ja",
            "kor" => "ko",
            "fra" => "fr",
            "spa" => "es",
            "ara" => "ar",
            other => other,
        }
        .to_string();

        let target_lower = target_lang.to_lowercase();
        if detected_norm == target_lower
            || (detected_norm.starts_with("zh") && target_lower.starts_with("zh"))
        {
            return Err(AppError::SameLanguage {
                lang: detected_norm,
            });
        }

        Ok(TranslationResult {
            source_text: text.to_string(),
            translated_text: translated,
            detected_source_lang: detected_norm,
            target_lang: target_lang.to_string(),
            provider: "baidu".to_string(),
            duration_ms,
            truncated: false,
        })
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "baidu".to_string(),
            name: "百度翻译".to_string(),
            requires_api_key: true,
            is_available: !self.app_id.is_empty() && !self.secret_key.is_empty(),
        }
    }

    async fn validate_credentials(&self) -> Result<bool, AppError> {
        if self.app_id.is_empty() || self.secret_key.is_empty() {
            return Ok(false);
        }
        match self.translate("hello", "zh", None).await {
            Ok(_) => Ok(true),
            Err(AppError::AuthError { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn update_api_key(&mut self, api_key: String) {
        // 格式 "app_id:secret_key"
        if let Some((id, key)) = api_key.split_once(':') {
            self.app_id = id.to_string();
            self.secret_key = key.to_string();
        }
    }

    fn update_credentials(&mut self, creds: HashMap<String, String>) {
        if let Some(id) = creds.get("baidu_app_id") {
            self.app_id = id.clone();
        }
        if let Some(key) = creds.get("baidu_secret_key") {
            self.secret_key = key.clone();
        }
    }
}
