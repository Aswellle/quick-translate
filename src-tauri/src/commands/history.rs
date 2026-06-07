// src-tauri/src/commands/history.rs
// 历史记录 command handlers
// history 字段现为 Arc<HistoryRepository>（无外层 Mutex），直接调用即可

use tauri::State;

use crate::error::AppError;
use crate::state::AppState;
use crate::types::{HistoryQuery, StatsResult, TranslationRecord};

/// 查询历史记录（支持全文搜索 + 分页 + starred_only 过滤）
#[tauri::command]
pub async fn query_history(
    state: State<'_, AppState>,
    params: HistoryQuery,
) -> Result<Vec<TranslationRecord>, AppError> {
    state.history.query(&params).await
}

/// 获取历史记录总数（用于前端分页计算）
#[tauri::command]
pub async fn count_history(
    state: State<'_, AppState>,
    search: Option<String>,
    starred_only: Option<bool>,
) -> Result<i64, AppError> {
    state
        .history
        .count(search.as_deref(), starred_only.unwrap_or(false))
        .await
}

/// 清空所有历史记录
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), AppError> {
    state.history.clear_all().await
}

/// 删除单条历史记录
#[tauri::command]
pub async fn delete_history_record(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    state.history.delete_by_id(&id).await
}

/// 切换收藏状态，返回新的收藏状态（true = 已收藏）
#[tauri::command]
pub async fn toggle_star_record(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    state.history.toggle_star(&id).await
}

/// 导出全部历史记录为 JSON 字符串
#[tauri::command]
pub async fn export_history(state: State<'_, AppState>) -> Result<String, AppError> {
    let records = state.history.export_all().await?;
    serde_json::to_string_pretty(&records).map_err(|e| AppError::SerdeError(e.to_string()))
}

/// 获取使用统计数据
#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>) -> Result<StatsResult, AppError> {
    state.history.get_stats().await
}
