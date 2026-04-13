// src-tauri/src/commands/system.rs
// 系统操作 command handlers（剪贴板、浮窗控制、Toast、开机自启动）

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;
use crate::system::clipboard;
use crate::types::ToastPayload;

/// 将文本写入剪贴板（前端"复制"按钮触发）
#[tauri::command]
pub async fn copy_to_clipboard(text: String) -> Result<(), AppError> {
    clipboard::write_clipboard_text(&text)
}

/// 隐藏翻译浮窗
#[tauri::command]
pub async fn hide_popup(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("popup") {
        window
            .hide()
            .map_err(|e: tauri::Error| AppError::WindowError(e.to_string()))?;
    }
    Ok(())
}

/// 获取应用版本号
#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 向所有已打开窗口广播 Toast 通知（前端监听 "toast" 事件）
#[tauri::command]
pub async fn notify_toast(app: AppHandle, payload: ToastPayload) -> Result<(), AppError> {
    emit_toast(&app, payload);
    Ok(())
}

/// 内部函数：广播 Toast 事件（供 Rust 其他模块直接调用）
pub fn emit_toast(app: &AppHandle, payload: ToastPayload) {
    let _ = app.emit("toast", &payload);
}

/// 获取开机自启动状态
#[tauri::command]
pub async fn get_autostart(app: AppHandle) -> Result<bool, AppError> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| AppError::ConfigError(format!("获取自启动状态失败: {}", e)))
}

/// 设置开机自启动（Windows 注册表 / macOS LaunchAgent，由插件处理平台差异）
#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|e| AppError::ConfigError(format!("启用自启动失败: {}", e)))?;
    } else {
        autolaunch
            .disable()
            .map_err(|e| AppError::ConfigError(format!("禁用自启动失败: {}", e)))?;
    }

    // 同步到配置数据库
    let state = app.state::<crate::state::AppState>();
    state
        .config
        .write()
        .await
        .set("auto_start", if enabled { "true" } else { "false" })
        .await?;

    Ok(())
}

/// 前端手动触发更新检查（委托给 system::updater 模块）
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<(), AppError> {
    crate::system::updater::check_and_notify(&app).await;
    Ok(())
}

/// 检查是否需要显示首次引导向导
#[tauri::command]
pub async fn check_onboarding(app: AppHandle) -> Result<bool, AppError> {
    let state = app.state::<crate::state::AppState>();
    let completed = state.config.read().await
        .get("onboarding_completed")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(!completed)
}

/// 标记引导向导已完成
#[tauri::command]
pub async fn complete_onboarding(app: AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::state::AppState>();
    let result = state.config.write().await.set("onboarding_completed", "true").await;
    result
}
