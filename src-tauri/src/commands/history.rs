// src-tauri/src/commands/history.rs
// 历史记录 command handlers

use tauri::State;

use crate::error::AppError;
use crate::state::AppState;
use crate::types::{HistoryQuery, TranslationRecord};

/// 查询历史记录（支持全文搜索 + 分页）
#[tauri::command]
pub async fn query_history(
    state: State<'_, AppState>,
    params: HistoryQuery,
) -> Result<Vec<TranslationRecord>, AppError> {
    state.history.lock().await.query(&params).await
}

/// 获取历史记录总数（用于前端分页计算）
#[tauri::command]
pub async fn count_history(
    state: State<'_, AppState>,
    search: Option<String>,
) -> Result<i64, AppError> {
    state
        .history
        .lock()
        .await
        .count(search.as_deref())
        .await
}

/// 清空所有历史记录
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), AppError> {
    state.history.lock().await.clear_all().await
}
