// src-tauri/src/system/updater.rs
// 自动更新检查：启动后延迟 5s 后台静默检查，有新版本时通过 Toast 通知

use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::commands::system::emit_toast;
use crate::types::ToastPayload;

/// 后台静默检查更新，有新版本时推送 Toast，无新版本或出错均静默退出
pub async fn check_and_notify(app: &AppHandle) {
    // pubkey 为占位符时 updater() 会返回 Err，直接静默退出
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            tracing::debug!("Updater 未配置，跳过更新检查: {}", e);
            return;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            tracing::info!("发现新版本: v{}", version);

            emit_toast(
                app,
                ToastPayload {
                    message: format!("发现新版本 v{}，正在后台下载…", version),
                    kind: "info".into(),
                    duration: Some(5000),
                },
            );

            let app_clone = app.clone();
            // 后台下载安装：使用 tauri::async_runtime::spawn 避免 reactor 问题
            tauri::async_runtime::spawn(async move {
                match update
                    .download_and_install(|_dl, _total| {}, || {})
                    .await
                {
                    Ok(_) => {
                        emit_toast(
                            &app_clone,
                            ToastPayload::success("新版本已就绪，重启应用后生效".to_string()),
                        );
                    }
                    Err(e) => {
                        tracing::warn!("更新下载/安装失败: {}", e);
                        emit_toast(
                            &app_clone,
                            ToastPayload::warning(format!("更新下载失败，请手动更新: {}", e)),
                        );
                    }
                }
            });
        }
        Ok(None) => {
            tracing::debug!("当前已是最新版本");
        }
        Err(e) => {
            // 网络不可用 / endpoint 未配置时静默失败，不打扰用户，不打 ERROR 日志
            tracing::debug!("更新检查跳过（endpoint 未配置或网络不可用）: {}", e);
        }
    }
}
