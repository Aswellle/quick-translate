// src-tauri/src/commands/history.rs
// 历史记录 command handlers
// history 字段现为 Arc<HistoryRepository>（无外层 Mutex），直接调用即可

use tauri::{Emitter, State};

use crate::error::AppError;
use crate::state::AppState;
use crate::types::{HistoryQuery, StatsResult, TranslationRecord};

/// 历史数据发生变化后通知前端窗口刷新。
///
/// 落盘 worker 的写入路径由 system::persistence 广播同一条事件；这里
/// 覆盖用户直接驱动的变更（清空/删除/收藏切换）——它们不经过落盘
/// worker，不广播的话设置仪表盘的统计会停留在旧值。
fn notify_history_changed(app: &tauri::AppHandle) {
    let _ = app.emit("history-changed", ());
}

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
pub async fn clear_history(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    state.history.clear_all().await?;
    notify_history_changed(&app);
    Ok(())
}

/// 删除单条历史记录
#[tauri::command]
pub async fn delete_history_record(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    state.history.delete_by_id(&id).await?;
    notify_history_changed(&app);
    Ok(())
}

/// 切换收藏状态，返回新的收藏状态（true = 已收藏）
#[tauri::command]
pub async fn toggle_star_record(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, AppError> {
    let starred = state.history.toggle_star(&id).await?;
    notify_history_changed(&app);
    Ok(starred)
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
    ensure_plain_file_name(&file_name)?;

    let export_dir = state
        .config
        .read()
        .await
        .get("export_dir")
        .unwrap_or_default();

    let target = if export_dir.trim().is_empty() {
        // 每次询问：原生保存对话框。blocking API 会 park 调用它的线程直到
        // 用户关闭对话框——包进 spawn_blocking 让这次 park 落在阻塞线程池，
        // 不占住异步执行器的 worker
        let dialog = app
            .dialog()
            .file()
            .add_filter("导出文件", &["json", "md", "html"])
            .set_file_name(&file_name);
        let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
            .await
            .map_err(|e| AppError::ConfigError(format!("保存对话框任务失败: {}", e)))?;
        match picked {
            Some(path) => path
                .into_path()
                .map_err(|e| AppError::ConfigError(format!("无法解析所选路径: {}", e)))?,
            None => return Ok(None),
        }
    } else {
        let target = std::path::Path::new(&export_dir).join(&file_name);
        // join 的语义会被 file_name 翻转：绝对路径整体替换基路径、`..`
        // 可以上跳。file_name 已经由 ensure_plain_file_name 卡死为纯文件名，
        // 这里再断言一次落点确实在导出目录之内 —— 写入面的边界必须由
        // 接收方自己兜住，而不是信任调用方。
        if !target.starts_with(&export_dir) {
            return Err(AppError::ConfigError("导出路径越出保存目录".to_string()));
        }
        dedupe_path(target)
    };

    std::fs::write(&target, contents.as_bytes())
        .map_err(|e| AppError::ConfigError(format!("写入导出文件失败: {}", e)))?;

    let written = target.to_string_lossy().to_string();
    tracing::info!(event = "history_exported", "[history] 已导出到 {}", written);
    Ok(Some(written))
}

/// 校验 file_name 是纯文件名（不含任何路径分量）。
///
/// `Path::join` 对绝对路径参数会整体替换基路径、对 `..\` 分量可目录穿越、
/// Windows 下 `:` 还可写 ADS——contents 虽来自自家前端，但写入面一旦
/// 暴露成 IPC 命令，边界就必须卡死在这里。
pub(crate) fn ensure_plain_file_name(file_name: &str) -> Result<(), AppError> {
    let path = std::path::Path::new(file_name);
    let plain = path.file_name().is_some()
        && path.file_name() == Some(std::ffi::OsStr::new(file_name))
        && !file_name.contains(['/', '\\', ':']);
    if !plain {
        return Err(AppError::ConfigError(format!(
            "导出文件名必须是纯文件名: {}",
            file_name
        )));
    }
    Ok(())
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
/// 精确到毫秒：秒级粒度在同秒内连续两次导出时会生成同名文件、静默
/// 覆盖第一份——与 dedupe_path「绝不覆盖」的承诺相悖。
fn local_timestamp() -> String {
    let now = crate::types::now_unix_ms();
    let t = time::OffsetDateTime::from_unix_timestamp(now.div_euclid(1000))
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let local =
        t.to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}{:03}",
        local.year(),
        local.month() as u8,
        local.day(),
        local.hour(),
        local.minute(),
        local.second(),
        local.nanosecond() / 1_000_000
    )
}

/// 弹原生目录选择器，返回用户选择的目录（None = 取消）。
/// 供设置页设置/修改导出默认保存位置。
#[tauri::command]
pub async fn choose_export_dir(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file();
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder())
        .await
        .map_err(|e| AppError::ConfigError(format!("目录选择器任务失败: {}", e)))?;
    match picked {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── ensure_plain_file_name：写入面的路径分量防线 ────────────────────

    #[test]
    fn plain_file_names_pass_the_gate() {
        for name in [
            "report.json",
            "quicktranslate-history-2026-10-30.md",
            "x.html",
        ] {
            ensure_plain_file_name(name).expect("纯文件名应放行");
        }
    }

    #[test]
    fn path_components_are_rejected() {
        // 绝对路径：Path::join 会整体替换基路径
        ensure_plain_file_name(r"C:\Temp\evil.json").unwrap_err();
        ensure_plain_file_name("/etc/evil.json").unwrap_err();
        // 目录穿越与分隔符
        ensure_plain_file_name(r"..\evil.json").unwrap_err();
        ensure_plain_file_name("a/b.json").unwrap_err();
        ensure_plain_file_name(r"a\b.json").unwrap_err();
        // Windows ADS 与设备名形态
        ensure_plain_file_name("f.json:ads").unwrap_err();
        // 「.」与「..」本身没有 file_name 分量
        ensure_plain_file_name(".").unwrap_err();
        ensure_plain_file_name("..").unwrap_err();
    }

    // ── dedupe_path：已存在的导出不静默覆盖 ─────────────────────────────

    #[test]
    fn missing_target_keeps_its_name() {
        let dir = std::env::temp_dir().join(format!("qt-dedupe-{}", uuid::Uuid::new_v4()));
        let path = dir.join("report.json");
        assert_eq!(dedupe_path(path.clone()), path, "不存在的目标不改名");
    }

    #[test]
    fn existing_target_gets_a_timestamped_sibling() {
        let dir = std::env::temp_dir().join(format!("qt-dedupe-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("report.md");
        std::fs::write(&path, b"old").unwrap();

        let deduped = dedupe_path(path.clone());
        assert_ne!(deduped, path, "已存在的目标必须改名");
        let name = deduped.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("report-") && name.ends_with(".md"),
            "应保留词干与扩展名并插入时间戳: {}",
            name
        );
        assert_eq!(deduped.parent(), path.parent(), "仍在同一目录内");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
