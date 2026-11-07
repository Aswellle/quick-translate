// src-tauri/src/commands/glossary.rs
// 本地术语表的 IPC 面（计划 Task 10）。
//
// 只做数据管理：CRUD 与 CSV/JSON 导入导出。术语是否真正影响翻译由
// Provider 原生 glossary 能力决定（当前 DeepL），不在这里做任何字符串
// 替换或伪装生效。

use tauri::State;

use crate::domain::glossary::{self, GlossaryEntry, GlossaryEntryInput};
use crate::error::AppError;
use crate::state::AppState;

#[tauri::command]
pub async fn list_glossary_entries(
    state: State<'_, AppState>,
    source_lang: String,
    target_lang: String,
) -> Result<Vec<GlossaryEntry>, AppError> {
    state.glossary.list(&source_lang, &target_lang).await
}

#[tauri::command]
pub async fn create_glossary_entry(
    state: State<'_, AppState>,
    input: GlossaryEntryInput,
) -> Result<GlossaryEntry, AppError> {
    state.glossary.create(input).await
}

#[tauri::command]
pub async fn update_glossary_entry(
    state: State<'_, AppState>,
    id: i64,
    input: GlossaryEntryInput,
) -> Result<GlossaryEntry, AppError> {
    state.glossary.update(id, input).await
}

#[tauri::command]
pub async fn delete_glossary_entry(state: State<'_, AppState>, id: i64) -> Result<bool, AppError> {
    state.glossary.delete(id).await
}

#[tauri::command]
pub async fn clear_glossary(
    state: State<'_, AppState>,
    source_lang: String,
    target_lang: String,
) -> Result<i64, AppError> {
    state.glossary.clear(&source_lang, &target_lang).await
}

/// 导入 JSON：替换当前语言对的条目（整批校验通过才落库），返回写入条数。
#[tauri::command]
pub async fn import_glossary_json(
    state: State<'_, AppState>,
    source_lang: String,
    target_lang: String,
    content: String,
) -> Result<usize, AppError> {
    let entries = glossary::import_json(&content, &source_lang, &target_lang)
        .map_err(AppError::ConfigError)?;
    state
        .glossary
        .replace_pair(&source_lang, &target_lang, entries)
        .await
}

/// 导入 CSV：替换当前语言对的条目（整批校验通过才落库），返回写入条数。
#[tauri::command]
pub async fn import_glossary_csv(
    state: State<'_, AppState>,
    source_lang: String,
    target_lang: String,
    content: String,
) -> Result<usize, AppError> {
    let entries = glossary::import_csv(&content, &source_lang, &target_lang)
        .map_err(AppError::ConfigError)?;
    state
        .glossary
        .replace_pair(&source_lang, &target_lang, entries)
        .await
}

#[tauri::command]
pub async fn export_glossary_json(
    state: State<'_, AppState>,
    source_lang: String,
    target_lang: String,
) -> Result<String, AppError> {
    let entries = state.glossary.list(&source_lang, &target_lang).await?;
    glossary::export_json(&entries)
}

#[tauri::command]
pub async fn export_glossary_csv(
    state: State<'_, AppState>,
    source_lang: String,
    target_lang: String,
) -> Result<String, AppError> {
    let entries = state.glossary.list(&source_lang, &target_lang).await?;
    Ok(glossary::export_csv(&entries))
}
