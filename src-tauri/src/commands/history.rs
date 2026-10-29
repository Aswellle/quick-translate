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

/// 导出文件允许的最大体积（防御性上限：内容来自自家前端，但仍不该
/// 让一次异常调用把磁盘写满）
const EXPORT_MAX_BYTES: usize = 64 * 1024 * 1024;

/// 把历史导出内容写入文件。
///
/// 路径解析（`export_dir` 配置，问题反馈 2026-10-29）：
/// - 已设置 → 直接写 `export_dir/<file_name>`，不再弹框；同名文件已存在时
///   在扩展名前插入时间戳，绝不静默覆盖已有导出；
/// - 未设置（默认）→ 弹原生保存对话框，预填 `file_name`，用户取消返回 None。
///
/// 返回 `Some(最终写入路径)` 供前端 toast 展示；`None` = 用户取消了对话框。
#[tauri::command]
pub async fn export_history_to_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    file_name: String,
    contents: String,
) -> Result<Option<String>, AppError> {
    use tauri_plugin_dialog::DialogExt;

    if contents.len() > EXPORT_MAX_BYTES {
        return Err(AppError::ConfigError("导出内容超出大小上限".to_string()));
    }
    let lower = file_name.to_lowercase();
    if !lower.ends_with(".json") && !lower.ends_with(".md") && !lower.ends_with(".html") {
        return Err(AppError::ConfigError(format!(
            "不支持的导出文件名: {}",
            file_name
        )));
    }

    let export_dir = state
        .config
        .read()
        .await
        .get("export_dir")
        .unwrap_or_default();

    let target = if export_dir.trim().is_empty() {
        // 每次询问：原生保存对话框（blocking API 内部切换线程等待，
        // 不会堵住调用它的异步执行器）
        let file = app
            .dialog()
            .file()
            .add_filter("导出文件", &["json", "md", "html"])
            .set_file_name(&file_name);
        match file.blocking_save_file() {
            Some(path) => path
                .into_path()
                .map_err(|e| AppError::ConfigError(format!("无法解析所选路径: {}", e)))?,
            None => return Ok(None),
        }
    } else {
        dedupe_path(std::path::Path::new(&export_dir).join(&file_name))
    };

    std::fs::write(&target, contents.as_bytes())
        .map_err(|e| AppError::ConfigError(format!("写入导出文件失败: {}", e)))?;

    let written = target.to_string_lossy().to_string();
    tracing::info!(event = "history_exported", "[history] 已导出到 {}", written);
    Ok(Some(written))
}

/// 目标文件已存在时在扩展名前插入时间戳（`report.md` → `report-20261029-183000.md`），
/// 避免静默覆盖用户此前的导出。
fn dedupe_path(path: std::path::PathBuf) -> std::path::PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = format!("{}-{}.{}", stem, local_timestamp(), ext);
    path.with_file_name(name)
}

/// 本地时间戳（复用已在依赖树中的 `time` crate，不为此新增依赖）。
fn local_timestamp() -> String {
    let now = crate::types::now_unix_ms();
    let t = time::OffsetDateTime::from_unix_timestamp(now.div_euclid(1000))
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let local =
        t.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        local.year(),
        local.month() as u8,
        local.day(),
        local.hour(),
        local.minute(),
        local.second()
    )
}

/// 弹原生目录选择器，返回用户选择的目录（None = 取消）。
/// 供设置页设置/修改导出默认保存位置。
#[tauri::command]
pub async fn choose_export_dir(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    use tauri_plugin_dialog::DialogExt;
    match app.dialog().file().blocking_pick_folder() {
        Some(path) => path
            .into_path()
            .map(|p| Some(p.to_string_lossy().to_string()))
            .map_err(|e| AppError::ConfigError(format!("无法解析所选目录: {}", e))),
        None => Ok(None),
    }
}

/// 获取使用统计数据（`days`：逐日趋势覆盖的天数，1..=90，缺省 7）
#[tauri::command]
pub async fn get_stats(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> Result<StatsResult, AppError> {
    state.history.get_stats(days).await
}

/// 获取自定义区间的使用统计（两端均含其所在本地日；区间跨度 ≤90 天）
#[tauri::command]
pub async fn get_stats_range(
    state: State<'_, AppState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<StatsResult, AppError> {
    state.history.get_stats_range(start_ms, end_ms).await
}
