// src-tauri/src/commands/translate.rs
// 翻译 command handler（前端手动触发，如设置面板"测试翻译"）
// 注意：快捷键触发的翻译通过 system::translation_flow::execute() 执行，不经过此 command

use tauri::{AppHandle, State};

use crate::domain::cache::{lookup_cached_result, should_fall_back_to_cache};
use crate::error::AppError;
use crate::state::AppState;
use crate::system::translation::request::MAX_TEXT_CHARS;
use crate::types::{ProviderStatus, TranslationRecord, TranslationResult};

/// 手动覆盖敏感过滤：翻译最近一条被拦截的剪贴板内容。
///
/// 唯一的触发源是用户在浮窗/toast 上的显式点击 —— 本命令不主动拉取
/// 剪贴板，也不接受前端传入的文本，只能消费 coordinator 暂存的那一条。
/// 返回 false 表示没有待覆盖的内容（已被新复制取代或已覆盖过一次）。
#[tauri::command]
pub async fn translate_filtered_text(app: AppHandle) -> Result<bool, AppError> {
    Ok(crate::system::translation::translate_filtered_text(&app).await)
}

/// 手动路径的敏感内容过滤判定（评审整改）：与剪贴板路径执行同一套规则，
/// 且同样必须发生在任何网络请求之前。命中且未获用户确认 → Err；
/// 用户显式确认（force）或过滤开关关闭 → 放行。纯逻辑，单测锁定
/// 「先过滤」「确认放行」「开关尊重」三个语义。
fn manual_filter_check(text: &str, enabled: bool, force: bool) -> Result<(), AppError> {
    if force || !enabled {
        return Ok(());
    }
    match crate::domain::clipboard_filter::classify(text) {
        crate::domain::clipboard_filter::FilterDecision::Allow => Ok(()),
        crate::domain::clipboard_filter::FilterDecision::BlockSensitive { rule } => {
            Err(AppError::SensitiveContentBlocked {
                rule: rule.code().to_string(),
                reason: rule.reason().to_string(),
            })
        }
    }
}

/// 前端手动触发翻译
/// 用于：设置面板的"测试翻译"按钮
/// 注意：返回值直接给前端，不通过 event
#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    text: String,
    target_lang: Option<String>,
    source_lang: Option<String>,
    force: Option<bool>,
) -> Result<TranslationResult, AppError> {
    // 参数验证
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::EmptyText);
    }

    // 敏感内容过滤：手动路径与剪贴板路径同一套规则，且都在任何网络请求
    // （包括术语表同步）之前完成。命中时返回结构化错误，前端确认后以
    // force=true 重试 —— 过滤已完成、用户显式知情，与浮窗「仍然翻译」
    // 同一语义。被拦截的文本不记为「上一次翻译」。
    let filter_enabled = state
        .config
        .read()
        .await
        .get("sensitive_filter_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);
    let force = force.unwrap_or(false);
    if let Err(e) = manual_filter_check(&text, filter_enabled, force) {
        tracing::info!(
            event = "manual_translate_filtered",
            rule = match &e {
                AppError::SensitiveContentBlocked { rule, .. } => rule.as_str(),
                _ => "unknown",
            },
            len = text.len(),
            "[translate_text] 手动翻译命中敏感内容过滤，等待用户确认"
        );
        return Err(e);
    }

    // 显式源语言（可空 = 自动检测）。术语表只在这条路径上可能生效 ——
    // DeepL 不支持「术语表 + 源语言自动检测」。
    let source_lang = match source_lang {
        Some(s) if !s.trim().is_empty() && s.trim() != "auto" => {
            let s = s.trim().to_string();
            if !crate::types::SUPPORTED_LANGS.contains(&s.as_str()) {
                return Err(AppError::ConfigError(format!("源语言不受支持: {}", s)));
            }
            Some(s)
        }
        _ => None,
    };

    // 截断超长文本（与剪贴板路径共用同一上限，避免两处漂移）
    let (text_to_translate, truncated) = if text.chars().count() > MAX_TEXT_CHARS {
        (text.chars().take(MAX_TEXT_CHARS).collect::<String>(), true)
    } else {
        (text.clone(), false)
    };

    // 获取目标语言（参数 > 配置 > 默认值）
    // 必须用 .read().await，不能用 blocking_read()：此函数是 async fn，
    // 在 Tokio worker 线程上运行，blocking_read() 会 panic + abort 整进程
    let target = match target_lang {
        Some(t) => t,
        None => state
            .config
            .read()
            .await
            .get("target_lang")
            .unwrap_or_else(|| "zh".to_string()),
    };

    // 记为「上一次翻译」，供快捷键重试（存实际发送的截断后文本）
    state.coordinator.remember_text(&text_to_translate);

    // 术语表：仅在「开启 + 当前翻译源是 DeepL + 显式源语言 + 该语言对有
    // 条目」时同步并应用。同步失败返回 None（翻译继续，不假装生效）。
    let applied_glossary = crate::system::glossary_sync::prepare_deepl_glossary(
        &state,
        source_lang.as_deref(),
        &target,
    )
    .await;
    let glossary_fingerprint = applied_glossary
        .as_ref()
        .map(|g| g.fingerprint.clone())
        .unwrap_or_default();
    // 控制台术语库引用（百度/有道）与翻译风格：随请求读取，改配置立即生效。
    let (baidu_use_term_base, youdao_vocab_id, tencent_term_repo_ids, style) = {
        let cfg = state.config.read().await;
        (
            cfg.get("baidu_use_term_base")
                .map(|v| v == "true")
                .unwrap_or(false),
            cfg.get("youdao_vocab_id").filter(|v| !v.is_empty()),
            crate::domain::translator::tencent::TencentProvider::parse_term_repo_ids(
                &cfg.get("tencent_term_repo_ids").unwrap_or_default(),
            ),
            crate::domain::translator::TranslationStyle::from_config(
                &cfg.get("translation_style").unwrap_or_default(),
            ),
        )
    };
    // 缓存作用域令牌：显式源语言/术语指纹/控制台术语库/风格上下文进键。
    // 查询与写入用同一把钥匙 —— 用户切换语言、术语或风格配置后，旧上下文
    // 的缓存译文不会再被端出来（domain/cache.rs::scope_token）。
    let scope_token = crate::domain::cache::scope_token(
        source_lang.as_deref(),
        &glossary_fingerprint,
        baidu_use_term_base,
        youdao_vocab_id.as_deref(),
        &tencent_term_repo_ids,
        style.as_config_value(),
    );
    let options = crate::domain::translator::TranslateOptions {
        source_lang: source_lang.clone(),
        glossary_id: applied_glossary.as_ref().map(|g| g.glossary_id.clone()),
        glossary_fingerprint: applied_glossary.as_ref().map(|g| g.fingerprint.clone()),
        baidu_use_term_base,
        youdao_vocab_id,
        tencent_term_repo_ids,
        style,
    };

    // 调用翻译引擎（带上下文：显式源语言 + 已同步的 glossary）
    let engine_outcome = state
        .translator
        .translate_with_options(&text_to_translate, &target, &options)
        .await;

    // 与剪贴板路径一致：翻译源健康结论交给运行时层（计划第 4 节），
    // 无论成功失败都上报 —— coordinator 在 outcome 分支之前上报。
    state
        .runtime
        .report_providers_health(state.translator.providers_health().await);

    // 全部翻译源失败 → 回退本地缓存（与 coordinator 的剪贴板路径同一语义，
    // 判定与查询逻辑见 domain/cache.rs）。缓存结果不再落盘：条目本来就来自
    // 上一次翻译，重复入队只会多出一条重复的历史记录。
    // 缓存按「实际应用的术语指纹」查：术语变化后不会端出旧译法。
    let mut result = match engine_outcome {
        Ok(r) => r,
        Err(e) => {
            if should_fall_back_to_cache(&e) {
                if let Some(cached) = lookup_cached_result(
                    &state.cache,
                    &text_to_translate,
                    &target,
                    &scope_token,
                    truncated,
                )
                .await
                {
                    tracing::info!(
                        event = "translation_served_from_cache",
                        "[translate_text] 翻译源均不可用，改用本地缓存"
                    );
                    return Ok(cached);
                }
            }
            return Err(e);
        }
    };
    result.truncated = truncated;

    // 落盘走有界队列 + 单 worker（计划第 24/41 节），与剪贴板路径同一条管道。
    // 此前这里是「每请求 spawn 一个游离历史写任务」—— 计划第 41 节明确禁止的
    // 唯一残留形态。入队失败只丢一次历史/缓存写入，不影响返回给前端的结果。
    //
    // 历史记录存截断后的原文：与实际翻译内容一致（超长输入是先截断再送翻译
    // 的），HISTORY_LIMIT_MAX 对入库字节数也因此真正有效 —— 存截断前全文
    // 的话，一次粘贴几 MB 文本就绕过了保留上限。与缓存条目同源。
    let record = TranslationRecord::from_result(
        &result,
        &text_to_translate,
        &target,
        crate::types::RECORD_SOURCE_MANUAL,
    );
    let job = crate::system::persistence::PersistJob {
        history_limit: state
            .config
            .read()
            .await
            .get("history_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(200i64),
        record,
        cache_entry: crate::system::persistence::OwnedCacheEntry {
            source_text: text_to_translate.clone(),
            target_lang: target.clone(),
            translated_text: result.translated_text.clone(),
            source_lang: result.detected_source_lang.clone(),
            provider: result.provider.clone(),
            scope_token,
        },
    };
    if !state.persistence.enqueue(job) {
        tracing::warn!(
            event = "persistence_queue_full",
            "[translate_text] 落盘队列已满，本次历史与缓存写入被丢弃（翻译结果不受影响）"
        );
    }

    Ok(result)
}

/// 获取所有已注册翻译源列表
#[tauri::command]
pub async fn list_providers(
    state: State<'_, AppState>,
) -> Result<Vec<crate::types::ProviderInfo>, AppError> {
    Ok(state.translator.list_providers().await)
}

/// 验证指定翻译源的 API Key（调用 validate_credentials，不消耗翻译配额）
#[tauri::command]
pub async fn validate_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, AppError> {
    state
        .translator
        .validate_provider_credentials(&provider_id)
        .await
}

/// 枚举调试名 → 前端契约的 snake_case。
///
/// `ProviderStatus.health_state` / `last_error_class` 的前端契约是小写
/// snake_case（`"half_open"`、`"rate_limit"`，见 types.rs 与 commands.ts
/// 注释）。此前 `format!("{:?}", ...)` 直接下发 PascalCase（"HalfOpen"），
/// 设置页的状态词表 switch 全部落进 default，健康状态从未显示过；
/// `RateLimit` 小写后是 "ratelimit"，也命不中前端的 "rate_limit" 分支。
/// 与 `RuntimeHealth`/`ComponentState` 的 `#[serde(rename_all =
/// "snake_case")]` 保持同一命名约定。
fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// 获取所有翻译源的运行时健康状态（用于托盘菜单与诊断）
#[tauri::command]
pub async fn get_provider_status(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderStatus>, AppError> {
    let providers = state.translator.list_providers().await;
    let mut result = Vec::new();
    for info in providers {
        let health_state = state
            .translator
            .provider_health(&info.id)
            .await
            .map(|s| to_snake_case(&format!("{:?}", s)))
            .unwrap_or_else(|| "unknown".to_string());
        let last_error_class = state
            .translator
            .provider_last_error_class(&info.id)
            .await
            .map(|c| to_snake_case(&format!("{:?}", c)));
        result.push(ProviderStatus {
            id: info.id,
            name: info.name,
            requires_api_key: info.requires_api_key,
            is_available: info.is_available,
            health_state,
            last_error_class,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::manual_filter_check;
    use super::to_snake_case;
    use crate::error::AppError;

    /// 手动路径与剪贴板路径共用同一套规则：命中即拦，且解释文案
    /// 不回显原文（错误里只有规则代码与静态解释）。
    #[test]
    fn manual_filter_blocks_sensitive_content_before_any_network() {
        let sample = "Authorization: Bearer sk-test-1234567890";
        let err = manual_filter_check(sample, true, false).expect_err("必须被拦截");
        match &err {
            AppError::SensitiveContentBlocked { rule, reason } => {
                assert_eq!(rule, "SENSITIVE_AUTH_HEADER");
                assert!(!reason.contains(sample), "解释文案不得回显原文");
                assert!(!reason.contains("sk-test"), "解释文案不得回显原文片段");
            }
            other => panic!("应为 SensitiveContentBlocked，实际: {:?}", other),
        }
        assert_eq!(err.error_code(), "SENSITIVE_CONTENT_BLOCKED");
    }

    /// 用户显式确认（force）后放行 —— 过滤已执行、知情由前端确认承担，
    /// 与浮窗「仍然翻译」同一语义。
    #[test]
    fn manual_filter_allows_after_explicit_confirmation() {
        let sample = "Authorization: Bearer sk-test-1234567890";
        assert!(manual_filter_check(sample, true, true).is_ok());
    }

    /// 过滤开关关闭时手动路径同样不拦（与剪贴板门控开关语义一致）。
    #[test]
    fn manual_filter_respects_disabled_switch() {
        let sample = "Authorization: Bearer sk-test-1234567890";
        assert!(manual_filter_check(sample, false, false).is_ok());
    }

    /// 正常文本不受影响。
    #[test]
    fn manual_filter_allows_plain_text() {
        assert!(manual_filter_check("Hello, world!", true, false).is_ok());
    }

    /// 前端契约（SettingsWindow::providerStateLabel 的 switch 值）：
    /// 这组值一旦漂移，设置页的健康状态词表会整体静默消失。
    #[test]
    fn snake_case_matches_frontend_contract() {
        use crate::domain::translator::error_class::ErrorClass;
        use crate::domain::translator::health::ProviderHealthState;

        assert_eq!(
            to_snake_case(&format!("{:?}", ProviderHealthState::Healthy)),
            "healthy"
        );
        assert_eq!(
            to_snake_case(&format!("{:?}", ProviderHealthState::Degraded)),
            "degraded"
        );
        assert_eq!(
            to_snake_case(&format!("{:?}", ProviderHealthState::Open)),
            "open"
        );
        assert_eq!(
            to_snake_case(&format!("{:?}", ProviderHealthState::HalfOpen)),
            "half_open"
        );

        assert_eq!(
            to_snake_case(&format!("{:?}", ErrorClass::Transient)),
            "transient"
        );
        assert_eq!(
            to_snake_case(&format!("{:?}", ErrorClass::RateLimit)),
            "rate_limit"
        );
        assert_eq!(to_snake_case(&format!("{:?}", ErrorClass::Auth)), "auth");
        assert_eq!(to_snake_case(&format!("{:?}", ErrorClass::Quota)), "quota");
        assert_eq!(
            to_snake_case(&format!("{:?}", ErrorClass::Permanent)),
            "permanent"
        );
        assert_eq!(
            to_snake_case(&format!("{:?}", ErrorClass::SameLanguage)),
            "same_language"
        );
    }
}
