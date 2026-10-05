// src-tauri/src/commands/translate.rs
// 翻译 command handler（前端手动触发，如设置面板"测试翻译"）
// 注意：快捷键触发的翻译通过 system::translation_flow::execute() 执行，不经过此 command

use tauri::State;

use crate::error::AppError;
use crate::state::AppState;
use crate::system::translation::request::MAX_TEXT_CHARS;
use crate::types::{ProviderStatus, TranslationRecord, TranslationResult};

/// 前端手动触发翻译
/// 用于：设置面板的"测试翻译"按钮
/// 注意：返回值直接给前端，不通过 event
#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    text: String,
    target_lang: Option<String>,
) -> Result<TranslationResult, AppError> {
    // 参数验证
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::EmptyText);
    }

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

    // 调用翻译引擎
    let mut result = state
        .translator
        .translate(&text_to_translate, &target)
        .await?;
    result.truncated = truncated;

    // 与剪贴板路径一致：翻译源健康结论交给运行时层（计划第 4 节）
    state
        .runtime
        .report_providers_health(state.translator.providers_health().await);

    // 落盘走有界队列 + 单 worker（计划第 24/41 节），与剪贴板路径同一条管道。
    // 此前这里是「每请求 spawn 一个游离历史写任务」—— 计划第 41 节明确禁止的
    // 唯一残留形态。入队失败只丢一次历史/缓存写入，不影响返回给前端的结果。
    let record = TranslationRecord::from_result(&result, &text, &target);
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
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "unknown".to_string());
        let last_error_class = state
            .translator
            .provider_last_error_class(&info.id)
            .await
            .map(|c| format!("{:?}", c).to_lowercase());
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
