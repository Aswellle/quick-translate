// src-tauri/src/domain/translator/tencent.rs
// 腾讯云机器翻译（TMT）
// API 文档：https://cloud.tencent.com/document/product/551/15619
// 免费额度：每月 500 万字符

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::TranslationProvider;
use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

const TENCENT_ENDPOINT: &str = "https://tmt.tencentcloudapi.com/";
const SERVICE: &str = "tmt";
const HOST: &str = "tmt.tencentcloudapi.com";
const VERSION: &str = "2018-03-21";
const ACTION: &str = "TextTranslate";

type HmacSha256 = Hmac<Sha256>;

pub struct TencentProvider {
    http_client: Arc<HttpClient>,
    secret_id: String,
    secret_key: String,
}

impl TencentProvider {
    pub fn new(http_client: Arc<HttpClient>, secret_id: String, secret_key: String) -> Self {
        TencentProvider { http_client, secret_id, secret_key }
    }

    fn to_tencent_lang(lang: &str) -> &str {
        match lang {
            "zh" | "zh-tw" => "zh",
            "en" => "en",
            "ja" => "jp",
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

    /// TC3-HMAC-SHA256 签名
    fn sign(&self, payload: &str, timestamp: u64) -> (String, String) {
        let date = {
            let secs = timestamp;
            let d = chrono_from_timestamp(secs);
            format!("{:04}-{:02}-{:02}", d.0, d.1, d.2)
        };

        // Step 1: 构建规范请求串
        let http_method = "POST";
        let canonical_uri = "/";
        let canonical_querystring = "";
        let canonical_headers = format!(
            "content-type:application/json\nhost:{}\nx-tc-action:{}\n",
            HOST,
            ACTION.to_lowercase()
        );
        let signed_headers = "content-type;host;x-tc-action";
        let hashed_payload = hex_sha256(payload.as_bytes());

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            http_method, canonical_uri, canonical_querystring,
            canonical_headers, signed_headers, hashed_payload
        );

        // Step 2: 构建待签字符串
        let credential_scope = format!("{}/{}/tc3_request", date, SERVICE);
        let hashed_request = hex_sha256(canonical_request.as_bytes());
        let string_to_sign = format!(
            "TC3-HMAC-SHA256\n{}\n{}\n{}",
            timestamp, credential_scope, hashed_request
        );

        // Step 3: 计算签名
        let secret_date = hmac_sha256(format!("TC3{}", self.secret_key).as_bytes(), date.as_bytes());
        let secret_service = hmac_sha256(&secret_date, SERVICE.as_bytes());
        let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
        let signature = hex::encode(hmac_sha256(&secret_signing, string_to_sign.as_bytes()));

        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.secret_id, credential_scope, signed_headers, signature
        );

        (authorization, timestamp.to_string())
    }
}

#[derive(Serialize)]
struct TencentRequest {
    #[serde(rename = "SourceText")]
    source_text: String,
    #[serde(rename = "Source")]
    source: String,
    #[serde(rename = "Target")]
    target: String,
    #[serde(rename = "ProjectId")]
    project_id: u32,
}

#[derive(Deserialize)]
struct TencentResponse {
    #[serde(rename = "Response")]
    response: TencentResponseBody,
}

#[derive(Deserialize)]
struct TencentResponseBody {
    #[serde(rename = "TargetText")]
    target_text: Option<String>,
    #[serde(rename = "Source")]
    source: Option<String>,
    #[serde(rename = "Error")]
    error: Option<TencentError>,
}

#[derive(Deserialize)]
struct TencentError {
    #[serde(rename = "Code")]
    code: String,
    #[serde(rename = "Message")]
    message: String,
}

#[async_trait]
impl TranslationProvider for TencentProvider {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
        _source_lang: Option<&str>,
    ) -> Result<TranslationResult, AppError> {
        if self.secret_id.is_empty() || self.secret_key.is_empty() {
            return Err(AppError::AuthError { provider: "tencent".into() });
        }

        let start = Instant::now();
        let target = Self::to_tencent_lang(target_lang);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        let body = TencentRequest {
            source_text: text.to_string(),
            source: "auto".to_string(),
            target: target.to_string(),
            project_id: 0,
        };
        let payload = serde_json::to_string(&body)
            .map_err(|e| AppError::NetworkError(e.to_string()))?;

        let (authorization, ts_str) = self.sign(&payload, timestamp);

        let response = self
            .http_client
            .client()
            .post(TENCENT_ENDPOINT)
            .header("Content-Type", "application/json")
            .header("Host", HOST)
            .header("X-TC-Action", ACTION)
            .header("X-TC-Timestamp", &ts_str)
            .header("X-TC-Version", VERSION)
            .header("Authorization", &authorization)
            .body(payload)
            .send()
            .await
            .map_err(AppError::from)?;

        let duration_ms = start.elapsed().as_millis() as u64;

        if !response.status().is_success() {
            return Err(AppError::NetworkError(format!("腾讯翻译 HTTP {}", response.status().as_u16())));
        }

        let resp: TencentResponse = response.json().await
            .map_err(|e| AppError::NetworkError(format!("腾讯翻译响应解析失败: {}", e)))?;

        if let Some(err) = resp.response.error {
            return match err.code.as_str() {
                "AuthFailure" | "AuthFailure.SignatureFailure" =>
                    Err(AppError::AuthError { provider: "tencent".into() }),
                "RequestLimitExceeded" =>
                    Err(AppError::RateLimit { provider: "tencent".into() }),
                _ => Err(AppError::NetworkError(format!("腾讯翻译错误: {} {}", err.code, err.message))),
            };
        }

        let translated = resp.response.target_text
            .ok_or_else(|| AppError::NetworkError("腾讯翻译返回空结果".into()))?;
        let detected = resp.response.source
            .unwrap_or_else(|| "auto".to_string());

        let detected_lower = detected.to_lowercase();
        let target_lower = target_lang.to_lowercase();
        if detected_lower == target_lower || (detected_lower.starts_with("zh") && target_lower.starts_with("zh")) {
            return Err(AppError::SameLanguage { lang: detected });
        }

        Ok(TranslationResult {
            translated_text: translated,
            detected_source_lang: detected,
            target_lang: target_lang.to_string(),
            provider: "tencent".to_string(),
            duration_ms,
            truncated: false,
        })
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "tencent".to_string(),
            name: "腾讯翻译".to_string(),
            requires_api_key: true,
            is_available: !self.secret_id.is_empty() && !self.secret_key.is_empty(),
        }
    }

    async fn validate_credentials(&self) -> Result<bool, AppError> {
        if self.secret_id.is_empty() || self.secret_key.is_empty() { return Ok(false); }
        match self.translate("hello", "zh", None).await {
            Ok(_) => Ok(true),
            Err(AppError::AuthError { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn update_api_key(&mut self, api_key: String) {
        // api_key 格式："secret_id:secret_key"
        if let Some((id, key)) = api_key.split_once(':') {
            self.secret_id = id.to_string();
            self.secret_key = key.to_string();
        }
    }

    fn update_credentials(&mut self, creds: HashMap<String, String>) {
        if let Some(id) = creds.get("tencent_secret_id") { self.secret_id = id.clone(); }
        if let Some(key) = creds.get("tencent_secret_key") { self.secret_key = key.clone(); }
    }
}

// ── 加密工具函数 ──

fn hex_sha256(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC init failed");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// 从 Unix 时间戳提取 (year, month, day)
fn chrono_from_timestamp(ts: u64) -> (u32, u32, u32) {
    // 简单日期计算（无需 chrono crate）
    let days = ts / 86400;
    let mut y = 1970u32;
    let mut d = days as u32;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if d < dy { break; }
        d -= dy;
        y += 1;
    }
    let months = [31u32, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0u32;
    for (i, &dm) in months.iter().enumerate() {
        if d < dm { m = i as u32 + 1; break; }
        d -= dm;
    }
    (y, m, d + 1)
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
