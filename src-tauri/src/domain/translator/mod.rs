// src-tauri/src/domain/translator/mod.rs
// TranslationProvider trait + TranslationEngine 调度器

pub mod baidu;
pub mod deepl;
pub mod google;
pub mod tencent;
pub mod youdao;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

/// 翻译源接口 — 所有翻译源必须实现此 trait
#[async_trait]
pub trait TranslationProvider: Send + Sync {
    async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError>;

    fn info(&self) -> ProviderInfo;

    async fn validate_credentials(&self) -> Result<bool, AppError>;

    /// 更新单个主凭证（向后兼容 DeepL/Google）
    fn update_api_key(&mut self, api_key: String);

    /// 更新多个凭证（腾讯/百度/有道等多字段凭证提供者）
    /// 默认实现：若 map 中有 "api_key" 则调用 update_api_key
    fn update_credentials(&mut self, creds: HashMap<String, String>) {
        if let Some(key) = creds.get("api_key") {
            self.update_api_key(key.clone());
        }
    }
}

pub struct TranslationEngine {
    providers: RwLock<Vec<Box<dyn TranslationProvider>>>,
    active_provider_id: RwLock<String>,
    fallback_enabled: RwLock<bool>,
    #[allow(dead_code)]
    http_client: Arc<HttpClient>,
}

impl TranslationEngine {
    pub fn new(http_client: Arc<HttpClient>) -> Self {
        TranslationEngine {
            providers: RwLock::new(Vec::new()),
            active_provider_id: RwLock::new("google".to_string()),
            fallback_enabled: RwLock::new(true),
            http_client,
        }
    }

    pub async fn register_provider(&self, provider: Box<dyn TranslationProvider>) {
        self.providers.write().await.push(provider);
    }

    pub async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError> {
        let active_id = self.active_provider_id.read().await.clone();
        let fallback = *self.fallback_enabled.read().await;

        let providers = self.providers.read().await;
        if providers.is_empty() {
            return Err(AppError::AllProvidersFailed {
                errors: vec![("none".to_string(), "没有可用的翻译源".to_string())],
            });
        }

        let active_idx = providers
            .iter()
            .position(|p| p.info().id == active_id)
            .unwrap_or(0);

        let mut order: Vec<usize> = vec![active_idx];
        if fallback {
            // fallback 顺序：按优先级排列（跳过已选的 active）
            let priority = ["deepl", "tencent", "baidu", "youdao", "google"];
            for pid in &priority {
                if let Some(idx) = providers.iter().position(|p| p.info().id == *pid) {
                    if idx != active_idx {
                        order.push(idx);
                    }
                }
            }
            // 兜底：其余未在 priority 中的 provider
            for i in 0..providers.len() {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
        }

        let mut errors: Vec<(String, String)> = Vec::new();

        for idx in order {
            let provider = &providers[idx];
            let info = provider.info();

            // 跳过未配置凭证的需要 API Key 的 provider
            if info.requires_api_key && !info.is_available {
                tracing::debug!("跳过未配置的翻译源: {}", info.id);
                continue;
            }

            let provider_id = info.id.clone();

            match provider.translate(text, target_lang).await {
                Ok(result) => {
                    tracing::info!(
                        "翻译成功: provider={}, {}ms",
                        provider_id,
                        result.duration_ms
                    );
                    return Ok(result);
                }
                Err(AppError::SameLanguage { lang }) => {
                    // 源语言与目标语言相同，无需继续尝试其他 provider
                    tracing::info!("检测到相同语言 ({})，跳过 fallback 链", lang);
                    return Err(AppError::SameLanguage { lang });
                }
                Err(e) => {
                    tracing::warn!("翻译源 {} 失败: {}", provider_id, e);
                    errors.push((provider_id, e.to_string()));
                    if !fallback {
                        return Err(AppError::AllProvidersFailed { errors });
                    }
                }
            }
        }

        Err(AppError::AllProvidersFailed { errors })
    }

    pub async fn set_active_provider(&self, provider_id: &str) -> Result<(), AppError> {
        let providers = self.providers.read().await;
        let exists = providers.iter().any(|p| p.info().id == provider_id);
        drop(providers);
        if !exists {
            return Err(AppError::ConfigError(format!(
                "未知翻译源: {}",
                provider_id
            )));
        }
        *self.active_provider_id.write().await = provider_id.to_string();
        Ok(())
    }

    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .read()
            .await
            .iter()
            .map(|p| p.info())
            .collect()
    }

    /// 同步版本：供 tray::init 等同步上下文使用（setup 阶段安全）
    pub fn list_providers_sync(&self) -> Vec<ProviderInfo> {
        self.providers.blocking_read().iter().map(|p| p.info()).collect()
    }

    pub async fn update_provider_config(
        &self,
        provider_id: &str,
        api_key: Option<String>,
    ) -> Result<(), AppError> {
        let mut providers = self.providers.write().await;
        for provider in providers.iter_mut() {
            if provider.info().id == provider_id {
                if let Some(key) = api_key {
                    provider.update_api_key(key);
                }
                return Ok(());
            }
        }
        Err(AppError::ConfigError(format!(
            "未找到翻译源: {}",
            provider_id
        )))
    }

    /// 批量更新多字段凭证（腾讯/百度/有道专用）
    pub async fn update_provider_credentials(
        &self,
        provider_id: &str,
        creds: HashMap<String, String>,
    ) -> Result<(), AppError> {
        let mut providers = self.providers.write().await;
        for provider in providers.iter_mut() {
            if provider.info().id == provider_id {
                provider.update_credentials(creds);
                return Ok(());
            }
        }
        Err(AppError::ConfigError(format!(
            "未找到翻译源: {}",
            provider_id
        )))
    }

    pub async fn set_fallback_enabled(&self, enabled: bool) {
        *self.fallback_enabled.write().await = enabled;
    }

    /// 验证指定翻译源的凭证（调用 validate_credentials，不消耗翻译配额）
    pub async fn validate_provider_credentials(&self, provider_id: &str) -> Result<bool, AppError> {
        let providers = self.providers.read().await;
        for provider in providers.iter() {
            if provider.info().id == provider_id {
                return provider.validate_credentials().await;
            }
        }
        Err(AppError::ConfigError(format!(
            "未知翻译源: {}",
            provider_id
        )))
    }
}
