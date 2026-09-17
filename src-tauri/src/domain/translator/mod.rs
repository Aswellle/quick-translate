// src-tauri/src/domain/translator/mod.rs
// TranslationProvider trait + TranslationEngine 调度器

pub mod baidu;
pub mod deepl;
pub mod error_class;
pub mod google;
pub mod health;
pub mod policy;
pub mod tencent;
pub mod youdao;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::RwLock;

use crate::error::AppError;
use crate::infra::http_client::HttpClient;
use crate::types::{ProviderInfo, TranslationResult};

use baidu::BaiduProvider;
use deepl::DeepLProvider;
use error_class::classify;
use google::GoogleProvider;
use health::{ProviderHealth, ProviderHealthState, ProvidersHealth};
use policy::{policy_for, TranslationBudget, TOTAL_BUDGET};
use tencent::TencentProvider;
use youdao::YoudaoProvider;

/// 翻译源接口 — 所有翻译源必须实现此 trait
///
/// **provider 实例是不可变的。** 这里没有 `update_api_key` / `update_credentials`
/// 之类的 `&mut self` 方法 —— 凭证更新走计划第 12 节的「按新凭证新建实例 +
/// 原子替换」（见 `build_provider` 与 `TranslationEngine::update_provider_credentials`），
/// 而不是对可能正在处理请求的实例就地改写。
///
/// 去掉 `&mut self` 是为了让 provider 能以 `Arc<dyn TranslationProvider>` 的形式
/// 被克隆出注册表 —— 这是「不持锁跨越网络 I/O」的前提（计划第 11 节）。
#[async_trait]
pub trait TranslationProvider: Send + Sync {
    async fn translate(&self, text: &str, target_lang: &str)
        -> Result<TranslationResult, AppError>;

    fn info(&self) -> ProviderInfo;

    async fn validate_credentials(&self) -> Result<bool, AppError>;
}

/// 每个 provider 需要哪些凭证字段（config key 与 credential key 同名）。
///
/// 构造（启动）与更新（设置面板改 Key）两条路径共用这一份定义，
/// 避免出现「加了一个凭证字段却只改了其中一处」。
/// 顺序即注册顺序，也就是 fallback 链的默认优先级。
pub const CREDENTIAL_KEYS: &[(&str, &[&str])] = &[
    ("deepl", &["deepl_api_key"]),
    ("tencent", &["tencent_secret_id", "tencent_secret_key"]),
    ("baidu", &["baidu_app_id", "baidu_secret_key"]),
    ("youdao", &["youdao_app_key", "youdao_app_secret"]),
    ("google", &[]),
];

/// fallback 顺序（计划第 10 节）。active provider 永远排在最前，其后按此表。
const FALLBACK_PRIORITY: [&str; 5] = ["deepl", "tencent", "baidu", "youdao", "google"];

/// 按 id 从**完整**凭证集构造一个全新的 provider 实例。
///
/// 调用方必须给出该 provider 的全部凭证字段（缺失按空串处理）——
/// `commands::config::build_creds_map` 保证了这一点。
pub fn build_provider(
    id: &str,
    creds: &HashMap<String, String>,
    http_client: Arc<HttpClient>,
) -> Result<Box<dyn TranslationProvider>, AppError> {
    let get = |k: &str| creds.get(k).cloned().unwrap_or_default();

    Ok(match id {
        "deepl" => Box::new(DeepLProvider::new(http_client, get("deepl_api_key"))),
        "tencent" => Box::new(TencentProvider::new(
            http_client,
            get("tencent_secret_id"),
            get("tencent_secret_key"),
        )),
        "baidu" => Box::new(BaiduProvider::new(
            http_client,
            get("baidu_app_id"),
            get("baidu_secret_key"),
        )),
        "youdao" => Box::new(YoudaoProvider::new(
            http_client,
            get("youdao_app_key"),
            get("youdao_app_secret"),
        )),
        "google" => Box::new(GoogleProvider::new(http_client)),
        other => {
            return Err(AppError::ConfigError(format!("未知翻译源: {}", other)));
        }
    })
}

/// 注册表里的一条：provider 句柄 + 它的健康状态。
///
/// `Clone` 只复制两个 Arc —— 这就是「从注册表克隆出调用句柄后立刻放锁」的实现。
#[derive(Clone)]
struct ProviderEntry {
    provider: Arc<dyn TranslationProvider>,
    health: Arc<Mutex<ProviderHealth>>,
}

impl ProviderEntry {
    /// 锁中毒时取回内部数据继续用，而不是 unwrap panic ——
    /// 一次 panic 不该让某个 provider 永久不可用。
    fn lock_health(&self) -> std::sync::MutexGuard<'_, ProviderHealth> {
        self.health.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 依据 HTTP 状态码构造错误（计划第 14 节）。
///
/// 4xx（401/403/408/429 除外）意味着「这个请求被拒绝了」：换一家也一样，
/// 所以归入 `ProviderRejected` —— 不重试、不 fallback、也不计入熔断。
/// 408 与 5xx 是服务端暂态问题，仍走 `NetworkError`（Transient）。
///
/// 此前所有非 200 响应一律折进 `NetworkError`，于是「语种不支持」这类请求
/// 本身的问题会被当成 provider 故障 —— 撞够三次就把一个完全正常的源
/// 熔断十分钟。这是本次拆分要修的东西。
pub fn http_status_error(provider: &str, status: u16, body: String) -> AppError {
    match status {
        401 | 403 => AppError::AuthError {
            provider: provider.to_string(),
        },
        429 => AppError::RateLimit {
            provider: provider.to_string(),
        },
        // 408 是服务端在等请求超时，属于暂态
        408 => AppError::NetworkError(format!("{} HTTP 408: {}", provider, body)),
        400..=499 => AppError::ProviderRejected {
            provider: provider.to_string(),
            status,
        },
        _ => AppError::NetworkError(format!("{} HTTP {}: {}", provider, status, body)),
    }
}

/// 单个 provider 的尝试循环结局。
enum AttemptOutcome {
    Success(TranslationResult),
    /// 该源已用尽（失败/超时/熔断中被跳过），换下一个
    Exhausted,
    /// 总预算耗尽 —— 整条链必须停下来
    BudgetExhausted,
    /// 同语言：不是失败，但也无需换源，直接终止整条链
    SameLanguage(String),
    /// Permanent：换一家也一样，把真实原因交给用户
    Fatal(AppError),
}

/// 对**一个** provider 执行「尝试 → 判定 → 有限重试」的循环（计划第 8/13/30 节）。
///
/// Retry 与 Fallback 的分界线在这里：本函数只决定「同一个源要不要再试一次」，
/// 换源是调用方的事。
async fn try_provider(
    entry: &ProviderEntry,
    text: &str,
    target_lang: &str,
    budget: &TranslationBudget,
    errors: &mut Vec<(String, String)>,
) -> AttemptOutcome {
    let provider_id = entry.provider.info().id;
    let policy = policy_for(&provider_id);

    for attempt in 1..=policy.max_attempts {
        // ── 预算闸门：任何一次尝试之前都先看还剩多少 ──
        if budget.is_exhausted() {
            tracing::warn!(
                event = "translation_budget_exhausted",
                provider = %provider_id,
                "[provider] 总预算已耗尽，停止 fallback 链"
            );
            errors.push((provider_id, "总预算已耗尽".to_string()));
            return AttemptOutcome::BudgetExhausted;
        }

        // ── 熔断闸门：同步、短暂持锁，且不跨越下面的网络 await ──
        //
        // 每次尝试前都要过：上一次尝试可能刚把它推到熔断阈值。
        {
            let mut h = entry.lock_health();
            if !h.try_acquire(Instant::now()) {
                tracing::debug!(
                    event = "provider_skipped_by_circuit",
                    provider = %provider_id,
                    "[provider] {} 处于熔断中，跳过本次请求",
                    provider_id
                );
                errors.push((provider_id, "熔断中，已跳过".to_string()));
                return AttemptOutcome::Exhausted;
            }
        }

        // 单次尝试超时 = 策略值与剩余预算取小 —— 总预算真正生效的地方
        let attempt_timeout = budget.attempt_timeout(policy);
        let outcome =
            tokio::time::timeout(attempt_timeout, entry.provider.translate(text, target_lang))
                .await;

        // 引擎施加的超时与 provider 自报的超时走同一条处理路径，
        // 因此同样计入熔断、同样允许重试（计划第 8 节：Request timeout 重试 0~1 次）
        let result = match outcome {
            Ok(r) => r,
            Err(_elapsed) => Err(AppError::Timeout {
                timeout_secs: attempt_timeout.as_secs().max(1),
            }),
        };

        match result {
            Ok(result) => {
                entry.lock_health().record_success(Instant::now());
                tracing::info!(
                    "翻译成功: provider={}, {}ms",
                    provider_id,
                    result.duration_ms
                );
                return AttemptOutcome::Success(result);
            }
            Err(AppError::SameLanguage { lang }) => {
                // provider 正常回应了，只是这次源语言与目标语言相同。
                // 这是一次成功的调用（它证明 provider 可达），因此记成功 ——
                // 顺带正确归还半开探测名额。
                entry.lock_health().record_success(Instant::now());
                tracing::info!("检测到相同语言 ({})，跳过 fallback 链", lang);
                return AttemptOutcome::SameLanguage(lang);
            }
            Err(e) => {
                let class = classify(&e);
                {
                    let mut h = entry.lock_health();
                    let mut rng = rand::thread_rng();
                    h.record_failure(class, Instant::now(), &mut rng);
                }

                tracing::warn!(
                    event = "provider_attempt_failed",
                    provider = %provider_id,
                    class = ?class,
                    attempt,
                    "[provider] {} 第 {} 次尝试失败: {}",
                    provider_id,
                    attempt,
                    e
                );

                if !class.allows_fallback() {
                    return AttemptOutcome::Fatal(e);
                }

                if attempt < policy.max_attempts && policy.should_retry(class) {
                    let backoff = policy.backoff_after(attempt);
                    if backoff >= budget.remaining() {
                        // 连退避都睡不起，重试没有意义 —— 省下预算给别的源
                        errors.push((provider_id, e.to_string()));
                        return AttemptOutcome::Exhausted;
                    }
                    tracing::info!(
                        event = "provider_retry_scheduled",
                        provider = %provider_id,
                        attempt,
                        backoff_ms = backoff.as_millis() as u64,
                        "[provider] {} 将在 {:?} 后重试",
                        provider_id,
                        backoff
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                errors.push((provider_id, e.to_string()));
                return AttemptOutcome::Exhausted;
            }
        }
    }

    // 只在 max_attempts == 0 时可达；保住类型上的完整性
    AttemptOutcome::Exhausted
}

pub struct TranslationEngine {
    providers: RwLock<Vec<ProviderEntry>>,
    active_provider_id: RwLock<String>,
    fallback_enabled: RwLock<bool>,
    /// 供凭证更新时重建 provider 实例
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
        self.providers.write().await.push(ProviderEntry {
            provider: Arc::from(provider),
            health: Arc::new(Mutex::new(ProviderHealth::new())),
        });
    }

    /// 翻译。使用计划第 13 节规定的 8 秒总预算。
    pub async fn translate(
        &self,
        text: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, AppError> {
        self.translate_with_budget(text, target_lang, TranslationBudget::new(TOTAL_BUDGET))
            .await
    }

    /// 带显式预算的翻译。
    ///
    /// 生产路径固定用 8 秒总预算；测试用这个入口构造「已耗尽」与「只剩一点」
    /// 的情形 —— 否则验证预算生效就得让测试真的等满 8 秒。
    pub async fn translate_with_budget(
        &self,
        text: &str,
        target_lang: &str,
        budget: TranslationBudget,
    ) -> Result<TranslationResult, AppError> {
        let active_id = self.active_provider_id.read().await.clone();
        let fallback = *self.fallback_enabled.read().await;

        let entries = self.call_order(&active_id, fallback).await;
        if entries.is_empty() {
            return Err(AppError::AllProvidersFailed {
                errors: vec![("none".to_string(), "没有可用的翻译源".to_string())],
            });
        }

        let mut errors: Vec<(String, String)> = Vec::new();

        // 到这里注册表锁已经释放：下面的网络 I/O 不再阻塞 list_providers
        // （托盘菜单/设置页）。重构前读锁会一直活到 translate().await 之后。
        for entry in entries {
            let info = entry.provider.info();

            // 跳过未配置凭证的需要 API Key 的 provider
            if info.requires_api_key && !info.is_available {
                tracing::debug!("跳过未配置的翻译源: {}", info.id);
                continue;
            }

            match try_provider(&entry, text, target_lang, &budget, &mut errors).await {
                AttemptOutcome::Success(result) => return Ok(result),
                AttemptOutcome::SameLanguage(lang) => return Err(AppError::SameLanguage { lang }),
                // Permanent：换一家也是同样结果。直接把原因告诉用户，
                // 比统一报「所有翻译源均不可用」有用得多。
                AttemptOutcome::Fatal(e) => return Err(e),
                // 预算耗尽：整条链必须停 —— 这正是「不让用户干等几十秒」的落点
                AttemptOutcome::BudgetExhausted => break,
                AttemptOutcome::Exhausted => continue,
            }
        }

        Err(AppError::AllProvidersFailed { errors })
    }

    /// 计算本次请求的尝试顺序，并把需要的句柄克隆出来（计划第 10 节）。
    ///
    /// 排序依据是「静态优先级」与「健康度」两件事：
    ///
    ///   tier 0  active —— 用户明确选定的源，永远先试
    ///   tier 1  Healthy —— 按 FALLBACK_PRIORITY 静态顺序
    ///   tier 2  Degraded —— 有失败但还能用，排到健康源之后
    ///   tier 3  Open / HalfOpen —— 排最后
    ///
    /// 把 Degraded 排在 Healthy 之后，是为了不再拿一个正在失败的源去浪费
    /// 用户的预算。Open 的**不**在这里剔除：熔断闸门（`try_acquire`）才是
    /// 唯一有权决定放不放行的地方，而冷却到期时正是它在发放半开探测名额 ——
    /// 提前过滤掉就等于永远不探测，那个 provider 再也回不来。
    ///
    /// 读锁在这里拿、在这里放。返回之后调用方做网络 I/O 时，注册表锁已经
    /// 释放 —— 这是本模块要修的那个问题的核心（计划第 11 节）。
    async fn call_order(&self, active_id: &str, fallback: bool) -> Vec<ProviderEntry> {
        let entries = self.providers.read().await;

        if entries.is_empty() {
            return Vec::new();
        }

        // active 未注册时退回注册表里的第一个（与重构前 unwrap_or(0) 一致）
        let active = entries
            .iter()
            .find(|e| e.provider.info().id == active_id)
            .map(|e| e.provider.info().id)
            .unwrap_or_else(|| entries[0].provider.info().id);

        if !fallback {
            return entries
                .iter()
                .find(|e| e.provider.info().id == active)
                .cloned()
                .into_iter()
                .collect();
        }

        let mut ranked: Vec<(u8, usize, ProviderEntry)> = entries
            .iter()
            .map(|e| {
                let id = e.provider.info().id;
                let tier = if id == active {
                    0
                } else {
                    match e.lock_health().state() {
                        ProviderHealthState::Healthy => 1,
                        ProviderHealthState::Degraded => 2,
                        ProviderHealthState::Open | ProviderHealthState::HalfOpen => 3,
                    }
                };
                // 不在 FALLBACK_PRIORITY 表里的（理论上没有）排在同 tier 最后
                let rank = FALLBACK_PRIORITY
                    .iter()
                    .position(|p| *p == id)
                    .unwrap_or(usize::MAX);
                (tier, rank, e.clone())
            })
            .collect();

        ranked.sort_by_key(|(tier, rank, _)| (*tier, *rank));
        ranked.into_iter().map(|(_, _, e)| e).collect()
    }

    pub async fn set_active_provider(&self, provider_id: &str) -> Result<(), AppError> {
        let providers = self.providers.read().await;
        let exists = providers
            .iter()
            .any(|e| e.provider.info().id == provider_id);
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
            .map(|e| e.provider.info())
            .collect()
    }

    /// 同步版本：供 tray::init 等同步上下文使用（setup 阶段安全）
    pub fn list_providers_sync(&self) -> Vec<ProviderInfo> {
        self.providers
            .blocking_read()
            .iter()
            .map(|e| e.provider.info())
            .collect()
    }

    /// 全部翻译源的整体可用性结论（计划第 4 节）。
    ///
    /// 只统计**可用**的源：需要凭证却没配的那些既不是健康也不是故障，
    /// 它们根本没参与调度，把它们算进来会让「配了 DeepL 但没配百度」
    /// 显示成「有问题」。
    pub async fn providers_health(&self) -> ProvidersHealth {
        let entries = self.providers.read().await;

        let mut total = 0usize;
        let mut healthy = 0usize;
        let mut usable = 0usize;

        for e in entries.iter() {
            let info = e.provider.info();
            if info.requires_api_key && !info.is_available {
                continue;
            }
            total += 1;
            match e.lock_health().state() {
                ProviderHealthState::Healthy => {
                    healthy += 1;
                    usable += 1;
                }
                ProviderHealthState::Degraded => usable += 1,
                ProviderHealthState::Open | ProviderHealthState::HalfOpen => {}
            }
        }

        if total == 0 {
            ProvidersHealth::Unconfigured
        } else if usable == 0 {
            ProvidersHealth::Unavailable
        } else if healthy == total {
            ProvidersHealth::Healthy
        } else {
            ProvidersHealth::Degraded
        }
    }

    /// 读取某个翻译源的健康状态（供诊断与后续的 Tray/设置页状态展示）
    pub async fn provider_health(&self, provider_id: &str) -> Option<ProviderHealthState> {
        self.providers
            .read()
            .await
            .iter()
            .find(|e| e.provider.info().id == provider_id)
            .map(|e| e.lock_health().state())
    }

    /// 批量替换某翻译源的凭证（腾讯/百度/有道/DeepL 通用路径）。
    ///
    /// 计划第 12 节：不复用旧实例就地 mutate，而是按新凭证**新建**实例后原子
    /// 替换。正在处理请求的旧实例持有的一份凭证快照始终是自洽的，不会读到
    /// 「改了一半」的状态。
    ///
    /// 同时重置该 provider 的健康状态：用户刚把 Key 改对，预期是「现在就能用」，
    /// 不该让他在上一次失败留下的冷却里干等十分钟。
    ///
    /// `creds` 必须是该 provider 的**完整**凭证集（`build_creds_map` 保证）。
    pub async fn update_provider_credentials(
        &self,
        provider_id: &str,
        creds: HashMap<String, String>,
    ) -> Result<(), AppError> {
        let replacement: Arc<dyn TranslationProvider> = Arc::from(build_provider(
            provider_id,
            &creds,
            self.http_client.clone(),
        )?);

        let mut entries = self.providers.write().await;
        match entries
            .iter_mut()
            .find(|e| e.provider.info().id == provider_id)
        {
            Some(entry) => {
                entry.provider = replacement;
                entry.lock_health().reset();
                tracing::info!("已替换翻译源实例: {}", provider_id);
                Ok(())
            }
            None => Err(AppError::ConfigError(format!(
                "未找到翻译源: {}",
                provider_id
            ))),
        }
    }

    pub async fn set_fallback_enabled(&self, enabled: bool) {
        *self.fallback_enabled.write().await = enabled;
    }

    /// 验证指定翻译源的凭证（调用 validate_credentials，不消耗翻译配额）。
    ///
    /// 刻意**不**触碰熔断器（计划第 29 节）：手动验证与运行时翻译是两套独立
    /// 状态。让一次「点测试按钮」触发的验证去污染熔断器，会把一个明明正常的
    /// provider 停掉十分钟。
    pub async fn validate_provider_credentials(&self, provider_id: &str) -> Result<bool, AppError> {
        // 先把句柄克隆出来再放锁 —— validate_credentials 对 DeepL 会发真实
        // 网络请求，不能持着注册表锁等它。
        let entry = self
            .providers
            .read()
            .await
            .iter()
            .find(|e| e.provider.info().id == provider_id)
            .cloned();

        match entry {
            Some(e) => e.provider.validate_credentials().await,
            None => Err(AppError::ConfigError(format!(
                "未知翻译源: {}",
                provider_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::error_class::ErrorClass;
    use super::*;

    /// 脚本化的翻译源（计划第 35 节要求）。
    /// 每次 `translate` 消费一个预设结局；脚本耗尽后重复最后一个。
    #[derive(Debug, Clone, Copy)]
    enum Outcome {
        Ok(&'static str),
        Timeout,
        Auth,
        RateLimit,
        Quota,
        ServerError,
        SameLanguage,
        UnsupportedInput,
        /// 服务端以 4xx 拒绝（语种不支持等）
        Rejected,
        /// 慢响应：用于验证引擎施加的单次尝试超时
        Slow(Duration),
    }

    struct MockProvider {
        id: &'static str,
        requires_api_key: bool,
        available: bool,
        script: Mutex<VecDeque<Outcome>>,
        calls: AtomicUsize,
    }

    impl MockProvider {
        fn new(id: &'static str, script: Vec<Outcome>) -> Arc<Self> {
            Arc::new(Self {
                id,
                requires_api_key: true,
                available: true,
                script: Mutex::new(script.into()),
                calls: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl TranslationProvider for MockProvider {
        async fn translate(
            &self,
            text: &str,
            target_lang: &str,
        ) -> Result<TranslationResult, AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);

            // 用块作用域而不是 drop()：下面 Slow 分支里有 await，
            // 而 MutexGuard 不是 Send —— 必须让编译器明确看到它在 await 前
            // 就已经出作用域。
            let outcome = {
                let mut script = self.script.lock().unwrap();
                if script.len() > 1 {
                    script.pop_front().unwrap()
                } else {
                    script.front().copied().unwrap_or(Outcome::Ok("ok"))
                }
            };

            match outcome {
                Outcome::Ok(translated) => Ok(TranslationResult {
                    source_text: text.to_string(),
                    translated_text: translated.to_string(),
                    detected_source_lang: "en".to_string(),
                    target_lang: target_lang.to_string(),
                    provider: self.id.to_string(),
                    duration_ms: 1,
                    truncated: false,
                }),
                Outcome::Timeout => Err(AppError::Timeout { timeout_secs: 5 }),
                Outcome::Auth => Err(AppError::AuthError {
                    provider: self.id.to_string(),
                }),
                Outcome::RateLimit => Err(AppError::RateLimit {
                    provider: self.id.to_string(),
                }),
                Outcome::Quota => Err(AppError::QuotaExhausted {
                    provider: self.id.to_string(),
                }),
                Outcome::ServerError => Err(AppError::NetworkError("HTTP 503".to_string())),
                Outcome::SameLanguage => Err(AppError::SameLanguage {
                    lang: "zh".to_string(),
                }),
                Outcome::UnsupportedInput => Err(AppError::NonTextContent),
                Outcome::Rejected => Err(http_status_error(self.id, 400, String::new())),
                Outcome::Slow(d) => {
                    tokio::time::sleep(d).await;
                    Ok(TranslationResult {
                        source_text: text.to_string(),
                        translated_text: "late".to_string(),
                        detected_source_lang: "en".to_string(),
                        target_lang: target_lang.to_string(),
                        provider: self.id.to_string(),
                        duration_ms: d.as_millis() as u64,
                        truncated: false,
                    })
                }
            }
        }

        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: self.id.to_string(),
                name: self.id.to_string(),
                requires_api_key: self.requires_api_key,
                is_available: self.available,
            }
        }

        async fn validate_credentials(&self) -> Result<bool, AppError> {
            Ok(true)
        }
    }

    /// 造一个装了 mock provider 的引擎。返回引擎与各 mock 的句柄（用于断言调用次数）。
    async fn engine_with(
        mocks: &[Arc<MockProvider>],
        active: &str,
        fallback: bool,
    ) -> TranslationEngine {
        let engine = TranslationEngine::new(Arc::new(HttpClient::new()));
        for m in mocks {
            // MockProvider 需要被引擎与测试共享，这里用一层转发把 Arc 交出去
            engine
                .register_provider(Box::new(SharedMock(m.clone())))
                .await;
        }
        engine.set_active_provider(active).await.unwrap();
        engine.set_fallback_enabled(fallback).await;
        engine
    }

    /// 把 `Arc<MockProvider>` 包装成 `Box<dyn TranslationProvider>`，
    /// 使测试在交给引擎之后仍能读到调用次数。
    struct SharedMock(Arc<MockProvider>);

    #[async_trait]
    impl TranslationProvider for SharedMock {
        async fn translate(
            &self,
            text: &str,
            target_lang: &str,
        ) -> Result<TranslationResult, AppError> {
            self.0.translate(text, target_lang).await
        }
        fn info(&self) -> ProviderInfo {
            self.0.info()
        }
        async fn validate_credentials(&self) -> Result<bool, AppError> {
            self.0.validate_credentials().await
        }
    }

    // ── 计划第 35 节测试 1：A 超时 → B 成功 ──────────────────────────────

    #[tokio::test]
    async fn timeout_falls_back_to_the_next_provider() {
        let a = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let b = MockProvider::new("google", vec![Outcome::Ok("你好")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        let r = engine.translate("hello", "zh").await.unwrap();

        assert_eq!(r.provider, "google");
        assert_eq!(r.translated_text, "你好");
        // Phase 5 起，暂态失败会先按 RequestPolicy 原地重试一次再换源
        // （计划第 30 节：per-provider attempts <= 2）
        assert_eq!(
            a.calls(),
            policy_for("deepl").max_attempts as usize,
            "A 应被尝试 max_attempts 次"
        );
        assert_eq!(b.calls(), 1, "B 应被尝试");
    }

    // ── 计划第 35 节测试 2 + 3：熔断打开后不再发起网络调用 ───────────────

    #[tokio::test]
    async fn circuit_opens_after_threshold_and_stops_calling_that_provider() {
        let a = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        // 撞满阈值：A 每次都失败并落到 B
        for _ in 0..health::FAILURE_THRESHOLD {
            assert_eq!(
                engine.translate("x", "zh").await.unwrap().provider,
                "google"
            );
        }
        assert_eq!(a.calls(), health::FAILURE_THRESHOLD as usize);
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Open),
            "连续失败达阈值后必须熔断"
        );

        // 再请求若干次：A 绝不能再被调用
        let before = a.calls();
        for _ in 0..5 {
            assert_eq!(
                engine.translate("x", "zh").await.unwrap().provider,
                "google"
            );
        }
        assert_eq!(
            a.calls(),
            before,
            "熔断期间不得再向该 provider 发起任何网络调用（计划第 35 节测试 3）"
        );
    }

    #[tokio::test]
    async fn healthy_provider_stays_healthy_across_successes() {
        let a = MockProvider::new("deepl", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone()], "deepl", true).await;

        for _ in 0..10 {
            engine.translate("x", "zh").await.unwrap();
        }
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Healthy)
        );
    }

    // ── 计划第 35 节测试 5：Auth 失败不盲目重试，但仍换源 ───────────────

    #[tokio::test]
    async fn auth_failure_falls_back_but_never_retries_the_same_provider() {
        let a = MockProvider::new("deepl", vec![Outcome::Auth]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        let r = engine.translate("x", "zh").await.unwrap();

        assert_eq!(r.provider, "google", "认证失败应换源");
        assert_eq!(a.calls(), 1, "认证失败后不能原地重试 —— 重试一万次也不会好");
    }

    // ── 429 与额度耗尽：都换源，但后果不同 ───────────────────────────────

    #[tokio::test]
    async fn rate_limit_falls_back_to_the_next_provider() {
        let a = MockProvider::new("deepl", vec![Outcome::RateLimit]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        assert_eq!(
            engine.translate("x", "zh").await.unwrap().provider,
            "google"
        );
        assert_eq!(a.calls(), 1, "429 之后应换源，不该原地重试");
    }

    /// 额度耗尽与认证失败一样计入熔断（只是冷却更长）——
    /// 在用户充值之前，每次请求都去撞它纯属白等。
    #[tokio::test]
    async fn quota_failures_open_the_circuit_like_other_counted_classes() {
        let a = MockProvider::new("deepl", vec![Outcome::Quota]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        for _ in 0..health::FAILURE_THRESHOLD {
            assert_eq!(
                engine.translate("x", "zh").await.unwrap().provider,
                "google"
            );
        }
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Open)
        );

        let before = a.calls();
        engine.translate("x", "zh").await.unwrap();
        assert_eq!(a.calls(), before, "熔断后不该再撞额度已尽的源");
    }

    #[tokio::test]
    async fn all_providers_failing_yields_the_aggregate_error() {
        let a = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let b = MockProvider::new("google", vec![Outcome::ServerError]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        let err = engine.translate("x", "zh").await.unwrap_err();
        assert_eq!(err.error_code(), "ALL_PROVIDERS_FAILED");
        match err {
            AppError::AllProvidersFailed { errors } => {
                assert_eq!(errors.len(), 2, "每个失败源都该被记进诊断信息");
            }
            other => panic!("期望聚合错误，实际 {:?}", other),
        }
    }

    // ── 计划第 35 节测试 6：SameLanguage 不 fallback ─────────────────────

    #[tokio::test]
    async fn same_language_does_not_fall_back() {
        let a = MockProvider::new("deepl", vec![Outcome::SameLanguage]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        let err = engine.translate("你好", "zh").await.unwrap_err();

        assert_eq!(err.error_code(), "SAME_LANGUAGE");
        assert_eq!(b.calls(), 0, "同语言不是故障，不该换源");
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Healthy),
            "同语言不该影响 provider 健康"
        );
    }

    // ── Permanent 错误：不换源，且不熔断 ─────────────────────────────────

    #[tokio::test]
    async fn permanent_error_is_returned_as_is_and_does_not_open_the_circuit() {
        let a = MockProvider::new("deepl", vec![Outcome::UnsupportedInput]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        let err = engine.translate("x", "zh").await.unwrap_err();

        assert_eq!(
            err.error_code(),
            "NON_TEXT_CONTENT",
            "输入问题应把原因直接告诉用户，而不是报「所有翻译源均不可用」"
        );
        assert_eq!(b.calls(), 0, "换一家也是同样结果，不该白白多等一个超时");
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Healthy),
            "输入问题不是 provider 的健康问题"
        );
    }

    // ── fallback 关闭时的行为 ────────────────────────────────────────────

    #[tokio::test]
    async fn fallback_disabled_reports_the_aggregate_error() {
        let a = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", false).await;

        let err = engine.translate("x", "zh").await.unwrap_err();
        assert_eq!(err.error_code(), "ALL_PROVIDERS_FAILED");
        assert_eq!(b.calls(), 0, "fallback 关闭时不该尝试其他源");
    }

    // ── 凭证更新：实例替换 + 健康重置 ────────────────────────────────────

    #[tokio::test]
    async fn updating_credentials_resets_a_stuck_circuit() {
        let a = MockProvider::new("deepl", vec![Outcome::Auth]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        for _ in 0..health::FAILURE_THRESHOLD {
            let _ = engine.translate("x", "zh").await;
        }
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Open)
        );

        // 用户改对了 Key
        engine
            .update_provider_credentials("deepl", HashMap::new())
            .await
            .unwrap();

        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Healthy),
            "改完凭证应立刻可用，而不是干等冷却"
        );
    }

    #[tokio::test]
    async fn updating_unknown_provider_is_an_error() {
        let engine = TranslationEngine::new(Arc::new(HttpClient::new()));
        let err = engine
            .update_provider_credentials("nonexistent", HashMap::new())
            .await
            .unwrap_err();
        assert_eq!(err.error_code(), "CONFIG_ERROR");
    }

    // ── 凭证校验不污染熔断器（计划第 29 节）─────────────────────────────

    #[tokio::test]
    async fn credential_validation_does_not_touch_health() {
        let a = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let engine = engine_with(&[a.clone()], "deepl", true).await;

        for _ in 0..(health::FAILURE_THRESHOLD * 2) {
            engine.validate_provider_credentials("deepl").await.unwrap();
        }

        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Healthy),
            "手动验证不该影响运行时熔断状态"
        );
    }

    // ── 未配置凭证的源被静默跳过 ─────────────────────────────────────────

    #[tokio::test]
    async fn unconfigured_provider_is_skipped_without_a_call() {
        let unconfigured = Arc::new(MockProvider {
            id: "deepl",
            requires_api_key: true,
            available: false,
            script: Mutex::new(VecDeque::new()),
            calls: AtomicUsize::new(0),
        });
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[unconfigured.clone(), b.clone()], "deepl", true).await;

        let r = engine.translate("x", "zh").await.unwrap();

        assert_eq!(r.provider, "google");
        assert_eq!(unconfigured.calls(), 0, "未配置凭证的源不该被调用");
    }

    // ── HTTP 状态映射（计划第 14 节）─────────────────────────────────────

    #[test]
    fn http_status_maps_4xx_to_rejection_and_5xx_to_transient() {
        // 4xx（401/403/408/429 除外）→ 请求被拒绝，不是 provider 故障
        for status in [400, 404, 413, 422] {
            let e = http_status_error("deepl", status, String::new());
            assert_eq!(e.error_code(), "PROVIDER_REJECTED", "HTTP {}", status);
            assert_eq!(classify(&e), ErrorClass::Permanent);
        }

        assert_eq!(
            http_status_error("x", 401, String::new()).error_code(),
            "AUTH_ERROR"
        );
        assert_eq!(
            http_status_error("x", 403, String::new()).error_code(),
            "AUTH_ERROR"
        );
        assert_eq!(
            http_status_error("x", 429, String::new()).error_code(),
            "RATE_LIMIT"
        );

        // 408 与 5xx 是服务端暂态问题
        assert_eq!(
            http_status_error("x", 408, String::new()).error_code(),
            "NETWORK_ERROR"
        );
        for status in [500, 502, 503, 504] {
            let e = http_status_error("x", status, String::new());
            assert_eq!(e.error_code(), "NETWORK_ERROR");
            assert_eq!(classify(&e), ErrorClass::Transient);
        }
    }

    /// 回归：语种不支持（4xx）此前被折进 NetworkError，撞三次就把一个
    /// 完全正常的翻译源熔断十分钟。
    #[tokio::test]
    async fn rejected_request_never_opens_the_circuit() {
        let a = MockProvider::new("deepl", vec![Outcome::Rejected]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        for _ in 0..(health::FAILURE_THRESHOLD * 2) {
            let err = engine.translate("x", "zh").await.unwrap_err();
            assert_eq!(err.error_code(), "PROVIDER_REJECTED");
        }

        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Healthy),
            "语种不支持不是 DeepL 的健康问题"
        );
        assert_eq!(b.calls(), 0, "换一家也是同样结果，不该白白多等一个超时");
    }

    // ── 重试预算（计划第 30 节）──────────────────────────────────────────

    #[tokio::test]
    async fn transient_failure_retries_the_same_provider_up_to_the_limit() {
        let a = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let engine = engine_with(&[a.clone()], "deepl", true).await;

        let err = engine.translate("x", "zh").await.unwrap_err();

        assert_eq!(err.error_code(), "ALL_PROVIDERS_FAILED");
        assert_eq!(
            a.calls(),
            policy_for("deepl").max_attempts as usize,
            "暂态失败应重试同一个源，但严格受 max_attempts 限制"
        );
    }

    /// 非官方兜底源不重试（best_effort 策略）
    #[tokio::test]
    async fn best_effort_provider_is_never_retried() {
        let g = MockProvider::new("google", vec![Outcome::Timeout]);
        let engine = engine_with(&[g.clone()], "google", true).await;

        let _ = engine.translate("x", "zh").await;

        assert_eq!(
            g.calls(),
            1,
            "Google 的 best_effort 策略是 max_attempts = 1"
        );
    }

    /// 认证失败不重试（重试一万次也不会好），但仍换源
    #[tokio::test]
    async fn non_retryable_class_moves_on_immediately() {
        let a = MockProvider::new("deepl", vec![Outcome::Auth]);
        let b = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone(), b.clone()], "deepl", true).await;

        assert_eq!(
            engine.translate("x", "zh").await.unwrap().provider,
            "google"
        );
        assert_eq!(a.calls(), 1, "Auth 不该在原地重试");
    }

    // ── 总预算（计划第 13 节）────────────────────────────────────────────

    #[tokio::test]
    async fn exhausted_budget_stops_before_calling_any_provider() {
        let a = MockProvider::new("deepl", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[a.clone()], "deepl", true).await;

        let expired = TranslationBudget::starting_at(
            Instant::now() - TOTAL_BUDGET - Duration::from_secs(1),
            TOTAL_BUDGET,
        );
        let err = engine
            .translate_with_budget("x", "zh", expired)
            .await
            .unwrap_err();

        assert_eq!(err.error_code(), "ALL_PROVIDERS_FAILED");
        assert_eq!(a.calls(), 0, "预算已耗尽时不该再发起任何网络请求");
    }

    /// 计划第 13 节的核心承诺：五个源都坏掉也不能让用户等几十秒
    #[tokio::test]
    async fn slow_provider_is_cut_off_by_the_remaining_budget() {
        let a = MockProvider::new("deepl", vec![Outcome::Slow(Duration::from_secs(5))]);
        let engine = engine_with(&[a.clone()], "deepl", true).await;

        // 预算只有 80ms，单次尝试超时会被压到 80ms
        let start = Instant::now();
        let err = engine
            .translate_with_budget("x", "zh", TranslationBudget::new(Duration::from_millis(80)))
            .await
            .unwrap_err();

        assert!(
            start.elapsed() < Duration::from_secs(2),
            "被 5 秒的慢响应拖住了，实际 {:?}",
            start.elapsed()
        );
        assert_eq!(err.error_code(), "ALL_PROVIDERS_FAILED");
    }

    // ── fallback 排序：健康优先（计划第 10 节）───────────────────────────

    /// Degraded 的源必须排到 Healthy 之后 —— 不该拿一个正在失败的源去浪费预算。
    ///
    /// 反证：若排序只看静态优先级，deepl（FALLBACK_PRIORITY 首位）会排在
    /// google 之前并被选中，本测试就会失败。
    #[tokio::test]
    async fn degraded_providers_are_tried_after_healthy_ones() {
        let deepl = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let google = MockProvider::new("google", vec![Outcome::Ok("google-ok")]);
        let youdao = MockProvider::new("youdao", vec![Outcome::Ok("youdao-ok")]);
        let engine = engine_with(
            &[deepl.clone(), google.clone(), youdao.clone()],
            "deepl",
            true,
        )
        .await;

        // 让 deepl 降到 Degraded：以它为 active 失败一次（内部会按策略重试 2 次）
        let _ = engine.translate("x", "zh").await;
        assert_eq!(
            engine.provider_health("deepl").await,
            Some(ProviderHealthState::Degraded),
            "两次失败应停在 Degraded（阈值 3）"
        );

        // 换成 youdao 做 active，并让它失败；deepl 改为成功脚本
        engine.set_active_provider("youdao").await.unwrap();
        *youdao.script.lock().unwrap() = VecDeque::from(vec![Outcome::Timeout]);
        *deepl.script.lock().unwrap() = VecDeque::from(vec![Outcome::Ok("deepl-ok")]);

        let r = engine.translate("x", "zh").await.unwrap();

        assert_eq!(
            r.provider, "google",
            "健康源应优先于 Degraded 源：期望 google，若排序忽略健康度会得到 deepl"
        );
    }

    /// active 永远第一，即使它已经 Degraded —— 用户明确选了它
    #[tokio::test]
    async fn active_provider_is_tried_first_even_when_degraded() {
        let deepl = MockProvider::new("deepl", vec![Outcome::Timeout]);
        let google = MockProvider::new("google", vec![Outcome::Ok("ok")]);
        let engine = engine_with(&[deepl.clone(), google.clone()], "deepl", true).await;

        let _ = engine.translate("x", "zh").await; // deepl → Degraded

        let before = deepl.calls();
        let _ = engine.translate("x", "zh").await;

        assert!(
            deepl.calls() > before,
            "active 是用户选定的源，即使 Degraded 也要先试它"
        );
    }

    // ── build_provider ──────────────────────────────────────────────────

    #[test]
    fn build_provider_knows_every_registered_id() {
        let http = Arc::new(HttpClient::new());
        for (id, _) in CREDENTIAL_KEYS {
            let p = build_provider(id, &HashMap::new(), http.clone())
                .unwrap_or_else(|e| panic!("构造 {} 失败: {}", id, e));
            assert_eq!(p.info().id, *id, "构造出的实例 id 应与请求的一致");
        }
    }

    #[test]
    fn build_provider_rejects_unknown_id() {
        let http = Arc::new(HttpClient::new());
        // 不能用 unwrap_err：Box<dyn TranslationProvider> 没实现 Debug
        let err = build_provider("nope", &HashMap::new(), http)
            .err()
            .expect("未知 id 应当报错");
        assert_eq!(err.error_code(), "CONFIG_ERROR");
    }

    /// 凭证齐备时 `is_available` 必须为真 —— 否则启动时会把配好的源当成没配
    #[test]
    fn credential_keys_cover_every_provider_requirement() {
        let http = Arc::new(HttpClient::new());
        for (id, keys) in CREDENTIAL_KEYS {
            let creds: HashMap<String, String> = keys
                .iter()
                .map(|k| (k.to_string(), "dummy-value".to_string()))
                .collect();
            let p = build_provider(id, &creds, http.clone()).unwrap();
            let info = p.info();
            if info.requires_api_key {
                assert!(
                    info.is_available,
                    "{} 声明需要凭证，且 CREDENTIAL_KEYS 已给出 {} 个字段，\
                     构造后却仍不可用 —— 说明需要的字段名与表里写的不一致",
                    id,
                    keys.len()
                );
            }
        }
    }
}
