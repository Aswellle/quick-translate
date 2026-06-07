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

/// 隐藏翻译浮窗，并重置剪贴板监控的 last_text
/// 重置确保下次复制相同文本时仍能触发翻译（否则监控线程认为内容未变化）
#[tauri::command]
pub async fn hide_popup(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("popup") {
        window
            .hide()
            .map_err(|e: tauri::Error| AppError::WindowError(e.to_string()))?;
    }
    app.state::<crate::state::AppState>()
        .clipboard_monitor
        .reset_last_text();
    Ok(())
}

/// 动态调整翻译浮窗尺寸（根据内容自适应）
#[tauri::command]
pub async fn resize_popup(app: AppHandle, width: f64, height: f64) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("popup") {
        // 限制尺寸范围，避免过大或过小
        let w = width.clamp(280.0, 520.0);
        let h = height.clamp(60.0, 480.0);
        window
            .set_size(tauri::LogicalSize::new(w, h))
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
        // 仅在当前确实已启用时才执行 disable，
        // 避免注册表项不存在时 disable() 返回 os error 2
        let is_enabled = autolaunch.is_enabled().unwrap_or(false);
        if is_enabled {
            autolaunch
                .disable()
                .map_err(|e| AppError::ConfigError(format!("禁用自启动失败: {}", e)))?;
        }
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
    let completed = state
        .config
        .read()
        .await
        .get("onboarding_completed")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(!completed)
}

/// 使用系统默认浏览器打开指定 URL
/// 仅允许 http/https，使用 explorer 而非 cmd shell 避免 & | 等字符被解析为 shell 指令
#[tauri::command]
pub fn open_url(url: String) -> Result<(), AppError> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(AppError::WindowError("仅支持 http/https 链接".to_string()));
    }
    std::process::Command::new("explorer")
        .arg(&url)
        .spawn()
        .map_err(|e| AppError::WindowError(format!("打开链接失败: {}", e)))?;
    Ok(())
}

/// 设置剪贴板监控开关状态
#[tauri::command]
pub async fn set_clipboard_monitor_enabled(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    let state = app.state::<crate::state::AppState>();
    tracing::info!("[set_clipboard_monitor_enabled] enabled={}", enabled);

    // 更新配置数据库
    state
        .config
        .write()
        .await
        .set("clipboard_monitor_enabled", if enabled { "true" } else { "false" })
        .await?;

    // 更新监控线程状态
    if enabled {
        state.clipboard_monitor.resume();
        tracing::info!("[set_clipboard_monitor_enabled] 已调用 resume()，监控已启用");
    } else {
        state.clipboard_monitor.suspend();
        tracing::info!("[set_clipboard_monitor_enabled] 已调用 suspend()，监控已禁用");
    }

    Ok(())
}

/// 标记引导向导已完成
#[tauri::command]
pub async fn complete_onboarding(app: AppHandle) -> Result<(), AppError> {
    tracing::info!("[complete_onboarding] 开始设置 onboarding_completed=true");
    let state = app.state::<crate::state::AppState>();
    let result = state
        .config
        .write()
        .await
        .set("onboarding_completed", "true")
        .await;
    tracing::info!("[complete_onboarding] 设置完成: {:?}", result.is_ok());
    result
}

/// 打开居中的引导向导窗口（独立窗口，不复用 popup webview）
#[tauri::command]
pub async fn open_onboarding_window(app: AppHandle) -> Result<(), AppError> {
    // 若向导窗口已存在，则聚焦显示
    if let Some(window) = app.get_webview_window("onboarding") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    // 隐藏 popup 窗口（它被创建在光标位置，不应用于向导）
    if let Some(popup) = app.get_webview_window("popup") {
        tracing::info!("[open_onboarding] 隐藏现有 popup");
        let _ = popup.hide();
    } else {
        tracing::info!("[open_onboarding] popup 不存在，无需隐藏");
    }

    // 创建居中的向导窗口
    match tauri::WebviewWindowBuilder::new(
        &app,
        "onboarding",
        tauri::WebviewUrl::App("index.html#onboarding".into()),
    )
    .title("QuickTranslate 设置向导")
    .inner_size(480.0, 600.0)
    .min_inner_size(400.0, 500.0)
    .resizable(true)
    .center()
    .build()
    {
        Ok(window) => {
            let _ = window.show();
            tracing::info!("引导向导窗口已打开（居中）");
            Ok(())
        }
        Err(e) => {
            tracing::error!("创建引导向导窗口失败: {}", e);
            Err(AppError::WindowError(format!("创建向导窗口失败: {}", e)))
        }
    }
}
