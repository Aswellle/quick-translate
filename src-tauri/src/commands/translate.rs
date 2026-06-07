// src-tauri/src/commands/translate.rs
// 翻译 command handler（前端手动触发，如设置面板"测试翻译"）
// 注意：快捷键触发的翻译通过 system::translation_flow::execute() 执行，不经过此 command

use tauri::State;

use crate::error::AppError;
use crate::state::AppState;
use crate::types::{TranslationRecord, TranslationResult};

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

    // 截断超长文本
    let (text_to_translate, truncated) = if text.chars().count() > 5000 {
        (text.chars().take(5000).collect::<String>(), true)
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

    // 异步写入历史记录（不阻塞返回）
    let history = state.history.clone();
    let record = TranslationRecord::from_result(&result, &text, &target);
    let limit = state
        .config
        .read()
        .await
        .get("history_limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(200i64);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = history.insert(&record).await {
            tracing::error!("历史记录写入失败: {}", e);
        }
        if let Err(e) = history.enforce_limit(limit).await {
            tracing::error!("历史清理失败: {}", e);
        }
    });

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
