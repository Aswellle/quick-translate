// src-tauri/src/commands/config.rs
// 配置读写 command handlers

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::state::AppState;
use crate::system::{hotkey, tray};
use crate::types::AppConfig;

/// 获取完整配置（API Key 已脱敏）
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, AppError> {
    Ok(state.config.read().await.get_all())
}

/// 更新单个配置项，并触发必要的副作用
#[tauri::command]
pub async fn set_config(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
    value: String,
) -> Result<(), AppError> {
    state.config.write().await.set(&key, &value).await?;

    handle_side_effect(&app, &state, &key, &value).await?;
    Ok(())
}

/// 批量更新配置（设置面板保存时调用）
#[tauri::command]
pub async fn set_config_batch(
    state: State<'_, AppState>,
    app: AppHandle,
    updates: Vec<(String, String)>,
) -> Result<(), AppError> {
    // 批量写入 DB（auto_start 单独处理避免双写）
    let db_updates: Vec<(String, String)> = updates
        .iter()
        .filter(|(k, _)| k != "auto_start")
        .cloned()
        .collect();

    if !db_updates.is_empty() {
        state.config.write().await.set_batch(db_updates).await?;
    }

    let mut need_menu_refresh = false;

    for (key, value) in &updates {
        match key.as_str() {
            "hotkey" => {
                hotkey::re_register(&app, value)?;
            }
            "provider" => {
                state.translator.set_active_provider(value).await?;
                need_menu_refresh = true;
            }
            "target_lang" => {
                need_menu_refresh = true;
            }
            "deepl_api_key" => {
                state.translator.update_provider_config("deepl", Some(value.clone())).await?;
            }
            "fallback_enabled" => {
                state.translator.set_fallback_enabled(value == "true").await;
            }
            "auto_start" => {
                crate::commands::system::set_autostart(app.clone(), value == "true").await?;
            }
            "theme" => {
                // 广播给所有窗口，实现无刷新实时主题切换
                emit_theme_changed(&app, value);
            }
            _ => {}
        }
    }

    if need_menu_refresh {
        tray::refresh_menu(&app);
    }

    Ok(())
}

// ──────────── 副作用处理（set_config 单项用） ────────────

async fn handle_side_effect(
    app: &AppHandle,
    state: &State<'_, AppState>,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    match key {
        "hotkey" => {
            hotkey::re_register(app, value)?;
        }
        "provider" => {
            state.translator.set_active_provider(value).await?;
            tray::refresh_menu(app);
        }
        "target_lang" => {
            tray::refresh_menu(app);
        }
        "deepl_api_key" => {
            state.translator.update_provider_config("deepl", Some(value.to_string())).await?;
        }
        "tencent_secret_id" | "tencent_secret_key" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &*cfg,
                &[("tencent_secret_id", "tencent_secret_id"),
                  ("tencent_secret_key", "tencent_secret_key")],
                key, value,
            );
            drop(cfg);
            state.translator.update_provider_credentials("tencent", creds).await?;
        }
        "baidu_app_id" | "baidu_secret_key" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &*cfg,
                &[("baidu_app_id", "baidu_app_id"),
                  ("baidu_secret_key", "baidu_secret_key")],
                key, value,
            );
            drop(cfg);
            state.translator.update_provider_credentials("baidu", creds).await?;
        }
        "youdao_app_key" | "youdao_app_secret" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &*cfg,
                &[("youdao_app_key", "youdao_app_key"),
                  ("youdao_app_secret", "youdao_app_secret")],
                key, value,
            );
            drop(cfg);
            state.translator.update_provider_credentials("youdao", creds).await?;
        }
        "fallback_enabled" => {
            state.translator.set_fallback_enabled(value == "true").await;
        }
        "auto_start" => {
            crate::commands::system::set_autostart(app.clone(), value == "true").await?;
        }
        "theme" => {
            emit_theme_changed(app, value);
        }
        _ => {}
    }
    Ok(())
}

/// 广播主题变更事件给所有已打开的 WebviewWindow
fn emit_theme_changed(app: &AppHandle, theme: &str) {
    #[derive(serde::Serialize, Clone)]
    struct ThemePayload { theme: String }

    let _ = app.emit("theme-changed", ThemePayload { theme: theme.to_string() });
    tracing::info!("主题已切换: {}", theme);
}


// ── 辅助函数 ──

/// 构建凭证 HashMap，将当前 config 中的值与刚更新的单个字段合并
fn build_creds_map(
    config: &crate::domain::config::ConfigService,
    keys: &[(&str, &str)],
    updated_key: &str,
    updated_value: &str,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for (config_key, cred_key) in keys {
        let val = if *config_key == updated_key {
            updated_value.to_string()
        } else {
            config.get(config_key).unwrap_or_default()
        };
        map.insert(cred_key.to_string(), val);
    }
    map
}
