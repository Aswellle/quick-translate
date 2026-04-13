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
    let target = target_lang.unwrap_or_else(|| {
        state
            .config
            .blocking_read()
            .get("target_lang")
            .unwrap_or_else(|| "zh".to_string())
    });

    // 调用翻译引擎
    let mut result = state.translator.translate(&text_to_translate, &target).await?;
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
        let h = history.lock().await;
        if let Err(e) = h.insert(&record).await {
            tracing::error!("历史记录写入失败: {}", e);
        }
        if let Err(e) = h.enforce_limit(limit).await {
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

/// 验证指定翻译源的 API Key
#[tauri::command]
pub async fn validate_provider(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, AppError> {
    let providers = state.translator.list_providers().await;
    let exists = providers.iter().any(|p| p.id == provider_id);
    if !exists {
        return Err(AppError::ConfigError(format!(
            "未知翻译源: {}",
            provider_id
        )));
    }
    // 通过临时触发一次翻译来验证
    state.translator.translate("hello", "zh").await.map(|_| true).or_else(|e| match e {
        AppError::AuthError { .. } => Ok(false),
        other => Err(other),
    })
}
