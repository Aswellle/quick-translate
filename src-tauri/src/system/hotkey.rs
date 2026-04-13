// src-tauri/src/system/hotkey.rs
// 全局快捷键注册 / 注销 / 更新，含冲突检测 Toast 反馈

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::commands::system::emit_toast;
use crate::error::AppError;
use crate::types::ToastPayload;

/// 在 lib.rs setup 中调用，注册初始快捷键
pub fn register_initial(app: &AppHandle, hotkey: &str) -> Result<(), AppError> {
    let app_clone = app.clone();
    let hotkey_str = hotkey.to_string();

    app.global_shortcut()
        .on_shortcut(hotkey, move |_app, shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                tracing::debug!("快捷键触发: {:?}", shortcut);
                let handle = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    crate::system::translation_flow::execute(&handle).await;
                });
            }
        })
        .map_err(|_| AppError::HotkeyConflict {
            hotkey: hotkey_str,
        })?;

    tracing::info!("全局快捷键已注册: {}", hotkey);
    Ok(())
}

/// 快捷键变更时调用：注销旧快捷键，注册新快捷键
/// 注册失败时广播 Toast 错误提示，并返回 Err（前端可据此保持旧快捷键显示）
pub fn re_register(app: &AppHandle, new_hotkey: &str) -> Result<(), AppError> {
    // 先注销所有已注册快捷键
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| AppError::HotkeyConflict {
            hotkey: format!("注销失败: {}", e),
        })?;

    // 尝试注册新快捷键
    match register_initial(app, new_hotkey) {
        Ok(()) => Ok(()),
        Err(e @ AppError::HotkeyConflict { .. }) => {
            // 快捷键冲突：广播 Toast 提示用户
            emit_toast(
                app,
                ToastPayload::error(format!(
                    "快捷键 {} 已被占用，请在设置中选择其他快捷键",
                    new_hotkey
                )),
            );
            Err(e)
        }
        Err(e) => Err(e),
    }
}
