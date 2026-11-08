// src-tauri/src/domain/translator/deepl.rs
// DeepL Free API 翻译源实现

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

use super::{TranslateOptions, TranslationProvider};
use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

const DEEPL_FREE_API: &str = "https://api-free.deepl.com/v2/translate";
const DEEPL_PRO_API: &str = "https://api.deepl.com/v2/translate";

/// DeepL formality 支持的目标语言（应用内语言代码）。
///
/// 官方文档：formality 仅当目标语言为德/法/意/日/西/荷/波兰/葡/俄时
/// 可用（语义是目标语言的敬语/尊称体系，日语=です・ます体）。本项目
/// 支持的语言与之交集即此清单 —— 英文与中文目标传 formality 会被
/// DeepL 以 400 拒绝，所以必须在请求侧按目标语言裁剪。
pub const FORMALITY_TARGET_LANGS: &[&str] = &["de", "es", "fr", "it", "ja", "pt", "ru"];

/// 风格 → DeepL formality 参数值。返回 None = 不下发：
/// Natural（默认）保持请求形态与引入风格之前一致，目标语言不支持时
/// 下发会被整个请求 400 拒绝（比忽略更糟）。
fn formality_param(style: super::TranslationStyle, target_lang: &str) -> Option<&'static str> {
    let value = match style {
        super::TranslationStyle::Natural => return None,
        super::TranslationStyle::Formal => "more",
        super::TranslationStyle::Informal => "less",
    };
    if FORMALITY_TARGET_LANGS.contains(&target_lang) {
        Some(value)
    } else {
        None
    }
}

pub struct DeepLProvider {
    http_client: Arc<HttpClient>,
    api_key: String,
    #[cfg(test)]
    usage_endpoint_override: Option<String>,
    #[cfg(test)]
    translate_endpoint_override: Option<String>,
}

impl DeepLProvider {
    pub fn new(http_client: Arc<HttpClient>, api_key: String) -> Self {
        DeepLProvider {
            http_client,
            api_key,
            #[cfg(test)]
            usage_endpoint_override: None,
            #[cfg(test)]
            translate_endpoint_override: None,
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

    /// 按 API Key 判定 API 主机（Free 与 Pro 是不同域名）。
    /// 术语表同步（不在 provider 实例内）也用它，避免两处各判一次。
    pub(crate) fn api_base(api_key: &str) -> &'static str {
        if api_key.ends_with(":fx") {
            "https://api-free.deepl.com"
        } else {
            "https://api.deepl.com"
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

    /// 将应用内语言代码转换为 DeepL 目标语言代码（QT-006）。
    ///
    /// DeepL 官方代码表（developers.deepl.com）：ZH 为简体（历史代码）、
    /// **ZH-HANT 为繁体**。此前 zh-tw 被映射成 "ZH"，繁体目标被静默
    /// 替换成简体 —— 现按官方代码表显式区分。
    pub(crate) fn to_deepl_lang(lang: &str) -> String {
        match lang {
            "zh" => "ZH".to_string(),
            "zh-tw" => "ZH-HANT".to_string(),
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

    #[cfg(test)]
    fn with_translate_endpoint(mut self, url: String) -> Self {
        self.translate_endpoint_override = Some(url);
        self
    }

    fn translate_endpoint(&self) -> String {
        #[cfg(test)]
        if let Some(url) = &self.translate_endpoint_override {
            return url.clone();
        }
        self.endpoint().to_string()
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
        self.translate_with_options(text, target_lang, &TranslateOptions::default())
            .await
    }

    /// DeepL 是唯一读取请求上下文的 provider：显式源语言与 glossary_id
    /// 都通过 form 参数下发。其余 provider 用 trait 默认实现忽略上下文。
    async fn translate_with_options(
        &self,
        text: &str,
        target_lang: &str,
        options: &TranslateOptions,
    ) -> Result<TranslationResult, AppError> {
        if self.api_key.is_empty() {
            return Err(AppError::AuthError {
                provider: "deepl".to_string(),
            });
        }

        let start = Instant::now();
        let target = Self::to_deepl_lang(target_lang);
        let source = options.source_lang.as_deref().map(Self::to_deepl_lang);

        let mut params: Vec<(&str, &str)> = vec![
            ("auth_key", self.api_key.as_str()),
            ("text", text),
            ("target_lang", &target),
        ];
        if let Some(source) = source.as_deref() {
            params.push(("source_lang", source));
        }
        if let Some(glossary_id) = options.glossary_id.as_deref() {
            params.push(("glossary_id", glossary_id));
        }
        // 风格（计划 Task 10）：formality 只对部分目标语言有意义，目标
        // 不支持时不下发 —— 传了 DeepL 会以 400 拒绝整个请求。
        // Natural（默认）不下发，请求形态与引入风格之前完全一致。
        if let Some(formality) = formality_param(options.style, target_lang) {
            params.push(("formality", formality));
        }

        let response = self
            .http_client
            .client()
            .post(self.translate_endpoint())
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

// ── 术语表 API（计划 Task 10）──────────────────────────────────────────
//
// 独立于 provider 实例：同步发生在翻译请求之前，由 system::glossary_sync
// 调用。认证方式与 v3 文档一致（Authorization: DeepL-Auth-Key），请求体
// 为 JSON。v2 glossary 没有编辑接口 —— 条目变化 = 删旧建新。

#[derive(Serialize)]
struct CreateGlossaryRequest<'a> {
    name: &'a str,
    source_lang: String,
    target_lang: String,
    entries: &'a str,
    entries_format: &'a str,
}

#[derive(Deserialize)]
struct GlossaryResponse {
    glossary_id: String,
    #[serde(default)]
    ready: bool,
}

fn glossary_url(api_key: &str) -> String {
    format!("{}/v2/glossaries", DeepLProvider::api_base(api_key))
}

fn auth_header(api_key: &str) -> String {
    format!("DeepL-Auth-Key {}", api_key)
}

fn map_glossary_status(response: reqwest::Response, status: u16) -> AppError {
    match status {
        403 => AppError::AuthError {
            provider: "deepl".to_string(),
        },
        429 => AppError::RateLimit {
            provider: "deepl".to_string(),
            retry_after_secs: super::parse_retry_after_secs(response.headers()),
        },
        456 => AppError::QuotaExhausted {
            provider: "deepl".to_string(),
        },
        code => {
            // 与 translate 路径相同的脱敏规则：状态码进错误分类，
            // 响应体不进入常规日志（QT-011）
            AppError::ProviderRejected {
                provider: "deepl".to_string(),
                status: code,
            }
        }
    }
}

/// DeepL glossary 同步动作（创建/删除，含就绪轮询）的硬超时。
///
/// 同步内联在翻译关键路径上（`prepare_deepl_glossary` 在翻译发起前完成），
/// 此前只有单次 HTTP 的 5s 客户端超时，创建 + 轮询叠加最坏可拖住翻译
/// 半分钟以上。现在每个同步动作整体受此时限约束，超时返回错误，由调用方
/// 降级为「本次不应用术语表」—— 同步永远不会把翻译开头拖过可感知时限。
pub const GLOSSARY_SYNC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 创建 glossary，返回 glossary_id（已就绪）。
///
/// `entries_tsv` 每行 `原文\t译文`。创建后若 `ready=false`，最多轮询 5 次
/// （200ms 间隔，合计约 1s）；仍未就绪返回错误，由调用方降级为「本次不
/// 应用术语表」—— 不阻塞翻译主链路。整个创建动作受
/// [`GLOSSARY_SYNC_TIMEOUT`] 硬超时约束，超时同样按失败降级。
pub async fn create_glossary(
    client: &HttpClient,
    api_key: &str,
    name: &str,
    source_lang: &str,
    target_lang: &str,
    entries_tsv: &str,
) -> Result<String, AppError> {
    if api_key.is_empty() {
        return Err(AppError::AuthError {
            provider: "deepl".to_string(),
        });
    }
    create_glossary_at(
        &glossary_url(api_key),
        client,
        api_key,
        name,
        source_lang,
        target_lang,
        entries_tsv,
        GLOSSARY_SYNC_TIMEOUT,
    )
    .await
}

/// 可指定 glossary 端点根 URL（测试用 mock server；生产传官方地址）。
/// 整个创建动作（请求 + 就绪轮询）受 `sync_timeout` 硬超时约束。
// 请求参数 + 超时上限：形状由 DeepL v2 接口与测试 mock 双方固定，不值得为
// 过 clippy 再抽一层参数结构。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_glossary_at(
    url: &str,
    client: &HttpClient,
    api_key: &str,
    name: &str,
    source_lang: &str,
    target_lang: &str,
    entries_tsv: &str,
    sync_timeout: std::time::Duration,
) -> Result<String, AppError> {
    tokio::time::timeout(
        sync_timeout,
        create_glossary_request(
            url,
            client,
            api_key,
            name,
            source_lang,
            target_lang,
            entries_tsv,
        ),
    )
    .await
    .map_err(|_| AppError::Timeout {
        timeout_secs: sync_timeout.as_secs(),
    })?
}

async fn create_glossary_request(
    url: &str,
    client: &HttpClient,
    api_key: &str,
    name: &str,
    source_lang: &str,
    target_lang: &str,
    entries_tsv: &str,
) -> Result<String, AppError> {
    let body = CreateGlossaryRequest {
        name,
        source_lang: DeepLProvider::to_deepl_lang(source_lang),
        target_lang: DeepLProvider::to_deepl_lang(target_lang),
        entries: entries_tsv,
        entries_format: "tsv",
    };

    let response = client
        .client()
        .post(url)
        .header("Authorization", auth_header(api_key))
        .json(&body)
        .send()
        .await
        .map_err(AppError::from)?;

    let status = response.status().as_u16();
    if status != 200 && status != 201 {
        return Err(map_glossary_status(response, status));
    }
    let created: GlossaryResponse = response
        .json()
        .await
        .map_err(|e| AppError::NetworkError(format!("DeepL 术语表响应解析失败: {}", e)))?;

    let mut ready = created.ready;
    let mut glossary_id = created.glossary_id;
    for _ in 0..5 {
        if ready {
            return Ok(glossary_id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let response = client
            .client()
            .get(format!("{}/{}", url, glossary_id))
            .header("Authorization", auth_header(api_key))
            .send()
            .await
            .map_err(AppError::from)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(map_glossary_status(response, status));
        }
        let info: GlossaryResponse = response
            .json()
            .await
            .map_err(|e| AppError::NetworkError(format!("DeepL 术语表响应解析失败: {}", e)))?;
        ready = info.ready;
        glossary_id = info.glossary_id;
    }

    Err(AppError::NetworkError("DeepL 术语表尚未就绪".to_string()))
}

/// 删除 glossary。404 视为已删除（幂等）；其他失败交给调用方记日志。
/// 整个删除动作同样受 [`GLOSSARY_SYNC_TIMEOUT`] 硬超时约束。
pub async fn delete_glossary(
    client: &HttpClient,
    api_key: &str,
    glossary_id: &str,
) -> Result<(), AppError> {
    if api_key.is_empty() {
        return Err(AppError::AuthError {
            provider: "deepl".to_string(),
        });
    }
    delete_glossary_at(
        &glossary_url(api_key),
        client,
        api_key,
        glossary_id,
        GLOSSARY_SYNC_TIMEOUT,
    )
    .await
}

pub(crate) async fn delete_glossary_at(
    url: &str,
    client: &HttpClient,
    api_key: &str,
    glossary_id: &str,
    sync_timeout: std::time::Duration,
) -> Result<(), AppError> {
    tokio::time::timeout(
        sync_timeout,
        delete_glossary_request(url, client, api_key, glossary_id),
    )
    .await
    .map_err(|_| AppError::Timeout {
        timeout_secs: sync_timeout.as_secs(),
    })?
}

async fn delete_glossary_request(
    url: &str,
    client: &HttpClient,
    api_key: &str,
    glossary_id: &str,
) -> Result<(), AppError> {
    let response = client
        .client()
        .delete(format!("{}/{}", url, glossary_id))
        .header("Authorization", auth_header(api_key))
        .send()
        .await
        .map_err(AppError::from)?;
    let status = response.status().as_u16();
    match status {
        200 | 204 | 404 => Ok(()),
        _ => Err(map_glossary_status(response, status)),
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

    /// QT-006：目标语言映射必须对齐 DeepL 官方代码表
    /// （developers.deepl.com/docs/getting-started/supported-languages）：
    /// ZH 简体（历史代码）、ZH-HANT 繁体。此前 zh-tw 被静默换成简体 ZH。
    #[test]
    fn target_lang_mapping_matches_official_codes() {
        let cases: &[(&str, &str)] = &[
            ("zh", "ZH"),
            ("zh-tw", "ZH-HANT"), // 繁体：官方专用代码
            ("en", "EN"),
            ("ja", "JA"),
            ("ko", "KO"),
            ("fr", "FR"),
            ("de", "DE"),
            ("es", "ES"),
            ("ru", "RU"),
            ("pt", "PT"),
            ("it", "IT"),
            ("ar", "AR"),
        ];
        for &(input, expected) in cases {
            assert_eq!(
                DeepLProvider::to_deepl_lang(input),
                expected,
                "语言 {} 映射错误",
                input
            );
        }
    }

    /// 未知/未来代码按大写透传。
    #[test]
    fn unknown_lang_codes_pass_through_uppercased() {
        assert_eq!(DeepLProvider::to_deepl_lang("th"), "TH");
    }

    // ── 术语表上下文（计划 Task 10）────────────────────────────────────

    /// 显式源语言与 glossary_id 必须进入翻译请求的 form；默认调用不带它们。
    #[test]
    fn translate_options_send_source_lang_and_glossary_id() {
        let (url, server) = spawn_mock(
            200,
            r#"{"translations":[{"detected_source_language":"EN","text":"你好"}]}"#,
        );
        let provider = DeepLProvider::new(Arc::new(HttpClient::new()), "testkey:fx".to_string())
            .with_translate_endpoint(format!("{}/v2/translate", url));

        let options = TranslateOptions {
            source_lang: Some("en".to_string()),
            glossary_id: Some("glossary-123".to_string()),
            glossary_fingerprint: Some("fp".to_string()),
            ..Default::default()
        };
        let result = tauri::async_runtime::block_on(
            provider.translate_with_options("hello", "zh", &options),
        );
        assert!(result.is_ok(), "{:?}", result);

        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /v2/translate"),
            "实际请求: {}",
            request.lines().next().unwrap_or("")
        );
        let lower = request.to_lowercase();
        assert!(lower.contains("source_lang=en"), "必须显式下发源语言");
        assert!(
            lower.contains("glossary_id=glossary-123"),
            "必须下发 glossary_id"
        );
        assert!(lower.contains("auth_key=testkey%3afx"), "认证走 form");
    }

    // ── 翻译风格（计划 Task 10 后半）────────────────────────────────────

    /// formality 仅对支持的目标语言下发：正式 → more；请求形态其余部分
    /// 与无风格时一致。
    #[test]
    fn formal_style_sends_formality_for_supported_target() {
        let (url, server) = spawn_mock(
            200,
            r#"{"translations":[{"detected_source_language":"EN","text":"Guten Tag"}]}"#,
        );
        let provider = DeepLProvider::new(Arc::new(HttpClient::new()), "testkey:fx".to_string())
            .with_translate_endpoint(format!("{}/v2/translate", url));

        let options = TranslateOptions {
            style: super::super::TranslationStyle::Formal,
            ..Default::default()
        };
        let result = tauri::async_runtime::block_on(
            provider.translate_with_options("hello", "de", &options),
        );
        assert!(result.is_ok(), "{:?}", result);

        let lower = server.join().unwrap().to_lowercase();
        assert!(lower.contains("formality=more"), "正式风格必须下发 more");
    }

    /// 非正式 → less；目标语言不支持（如中文）时不下发 —— 传了会被
    /// DeepL 400 拒绝整个请求。
    #[test]
    fn formality_is_omitted_for_unsupported_target() {
        let cases = [
            (
                super::super::TranslationStyle::Informal,
                "zh",
                false,
                "less",
            ),
            (super::super::TranslationStyle::Formal, "zh", false, "more"),
            (super::super::TranslationStyle::Formal, "en", false, "more"),
            (super::super::TranslationStyle::Natural, "de", false, "more"),
        ];
        for (style, target, expect_present, needle) in cases {
            // detected 语言统一用 FR：与四个目标语言都不同，避免触发
            // SameLanguage 短路（那是另一条路径，与风格参数无关）
            let (url, server) = spawn_mock(
                200,
                r#"{"translations":[{"detected_source_language":"FR","text":"x"}]}"#,
            );
            let provider =
                DeepLProvider::new(Arc::new(HttpClient::new()), "testkey:fx".to_string())
                    .with_translate_endpoint(format!("{}/v2/translate", url));
            let options = TranslateOptions {
                style,
                ..Default::default()
            };
            let result = tauri::async_runtime::block_on(
                provider.translate_with_options("hello", target, &options),
            );
            assert!(result.is_ok(), "{:?} {}", result, target);

            let lower = server.join().unwrap().to_lowercase();
            assert_eq!(
                lower.contains(&format!("formality={}", needle)),
                expect_present,
                "style={:?} target={} 的 formality 下发判定错误",
                style,
                target
            );
        }
    }

    /// TranslationStyle::from_config 对脏值兜底为 Natural（与配置校验
    /// 双保险，旧库残留值不应让请求侧报错）。
    #[test]
    fn style_from_config_falls_back_to_natural() {
        use super::super::TranslationStyle;
        assert_eq!(
            TranslationStyle::from_config("formal"),
            TranslationStyle::Formal
        );
        assert_eq!(
            TranslationStyle::from_config("informal"),
            TranslationStyle::Informal
        );
        assert_eq!(
            TranslationStyle::from_config("natural"),
            TranslationStyle::Natural
        );
        assert_eq!(
            TranslationStyle::from_config("casual"),
            TranslationStyle::Natural
        );
        assert_eq!(TranslationStyle::from_config(""), TranslationStyle::Natural);
    }

    /// 创建 glossary：请求体是 JSON（auth 走请求头），解析 glossary_id；
    /// ready=true 时不再轮询。
    #[test]
    fn create_glossary_posts_entries_and_returns_id() {
        let (url, server) = spawn_mock(201, r#"{"glossary_id":"g-1","ready":true,"name":"n"}"#);
        let client = HttpClient::new();

        let result = tauri::async_runtime::block_on(create_glossary_at(
            &format!("{}/v2/glossaries", url),
            &client,
            "testkey:fx",
            "qt-en-zh",
            "en",
            "zh",
            "Apple\t苹果",
            GLOSSARY_SYNC_TIMEOUT,
        ));
        assert_eq!(result.unwrap(), "g-1");

        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v2/glossaries"));
        let lower = request.to_lowercase();
        assert!(lower.contains("authorization: deepl-auth-key testkey:fx"));
        assert!(lower.contains("\"entries_format\":\"tsv\""), "{}", request);
        assert!(lower.contains("\"source_lang\":\"en\""), "{}", request);
        assert!(lower.contains("\"target_lang\":\"zh\""), "{}", request);
        assert!(lower.contains("apple\\t"), "条目必须进请求体");
    }

    /// 同步硬超时（评审整改）：服务端拖延应答时，创建动作必须在期限之内
    /// 以 Timeout 失败返回，而不是叠加轮询把翻译关键路径挂住。
    #[test]
    fn create_glossary_times_out_when_server_is_slow() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test server accept");
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            thread::sleep(std::time::Duration::from_secs(1));
            let body = r#"{"glossary_id":"g-slow","ready":true,"name":"n"}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let started = std::time::Instant::now();
        let result = tauri::async_runtime::block_on(create_glossary_at(
            &format!("http://{}/v2/glossaries", addr),
            &HttpClient::new(),
            "testkey:fx",
            "qt-en-zh",
            "en",
            "zh",
            "Apple\t苹果",
            std::time::Duration::from_millis(150),
        ));
        let elapsed = started.elapsed();

        assert!(
            matches!(result, Err(AppError::Timeout { .. })),
            "慢速服务必须在期限内超时，实际: {:?}",
            result
        );
        assert!(
            elapsed < std::time::Duration::from_millis(900),
            "超时必须及时返回（服务端拖延 1s，期限 150ms），实际耗时 {:?}",
            elapsed
        );
        server.join().unwrap();
    }
}
