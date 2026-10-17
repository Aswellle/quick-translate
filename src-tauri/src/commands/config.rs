// src-tauri/src/commands/config.rs
// 配置读写 command handlers

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::state::AppState;
use crate::system::tray;
use crate::types::AppConfig;

/// 获取完整配置（API Key 已脱敏）
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, AppError> {
    Ok(state.config.read().await.get_all())
}

/// 更新单个配置项，并触发必要的副作用
///
/// 统一委托批量管线（校验 → 持久化 → 副作用）：此前单项与批量是两套
/// 顺序相反的实现，每新增配置键的作者都要猜该模仿哪套。单项更新就是
/// 长度为 1 的批量。
#[tauri::command]
pub async fn set_config(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
    value: String,
) -> Result<(), AppError> {
    set_config_batch_inner(&state, &app, vec![(key, value)]).await
}

/// 批量更新配置（设置面板保存时调用）
#[tauri::command]
pub async fn set_config_batch(
    state: State<'_, AppState>,
    app: AppHandle,
    updates: Vec<(String, String)>,
) -> Result<(), AppError> {
    set_config_batch_inner(&state, &app, updates).await
}

/// 配置写入的单一管线：校验 → 持久化 → 幂等副作用。
///
/// 单项与批量共用同一条路径 —— 顺序不一致曾是本仓库最年长的结构性
/// 隐患（副作用先行会在持久化失败后留下引擎/数据库各执一词的状态）。
async fn set_config_batch_inner(
    state: &State<'_, AppState>,
    app: &AppHandle,
    updates: Vec<(String, String)>,
) -> Result<(), AppError> {
    // 逐键预校验 + 掩码凭证过滤：无效值跳过并记录（宽松模式 = 拒之门外，
    // 而不是写进数据库让引擎与配置持久化各执一词）
    let mut validated: Vec<(String, String)> = Vec::with_capacity(updates.len());
    for (key, value) in &updates {
        // 掩码凭证在进入管线前丢弃：持久化与副作用两条路径都无需再各自设防
        if crate::domain::config::is_masked_credential(key, value) {
            tracing::debug!("[set_config] {} 传入掩码值，已跳过", key);
            continue;
        }
        if let Err(e) = validate_update(state, key, value).await {
            tracing::warn!("[set_config] {} 未通过校验，已跳过: {}", key, e);
            continue;
        }
        validated.push((key.clone(), value.clone()));
    }

    // 批量写入 DB（auto_start 单独处理避免双写）
    let db_updates: Vec<(String, String)> = validated
        .iter()
        .filter(|(k, _)| k != "auto_start")
        .cloned()
        .collect();

    if !db_updates.is_empty() {
        state.config.write().await.set_batch(db_updates).await?;
    }

    // 逐键应用副作用：宽松模式，单键失败仅记录警告，不中止整批次
    for (key, value) in &validated {
        if let Err(e) = handle_side_effect(app, state, key, value).await {
            tracing::warn!("[set_config] side_effect[{}] 失败（已忽略）: {}", key, e);
        }
    }

    Ok(())
}

/// 持久化前的值校验：只覆盖无需副作用的纯校验（副作用自身的构建失败
/// 已经由 handle_side_effect 把关）。
async fn validate_update(
    state: &State<'_, AppState>,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    if key == "provider" {
        let exists = state
            .translator
            .list_providers()
            .await
            .iter()
            .any(|p| p.id == value);
        if !exists {
            return Err(AppError::ConfigError(format!("未知翻译源: {}", value)));
        }
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
    // 掩码凭证已在管线入口（set_config_batch_inner）统一过滤，
    // 这里只处理确认为真实值的键。
    match key {
        "provider" => {
            state.translator.set_active_provider(value).await?;
            tray::refresh_menu(app).await;
        }
        "target_lang" => {
            tray::refresh_menu(app).await;
        }
        "deepl_api_key" => {
            // 与下面三家走同一条「完整凭证集 → 重建实例」路径。
            // 此前 DeepL 走的是单独的就地 mutate 分支，是唯一一个特例。
            let cfg = state.config.read().await;
            let creds = build_creds_map(&cfg, &[("deepl_api_key", "deepl_api_key")], key, value);
            drop(cfg);
            state
                .translator
                .update_provider_credentials("deepl", creds)
                .await?;
        }
        "tencent_secret_id" | "tencent_secret_key" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &cfg,
                &[
                    ("tencent_secret_id", "tencent_secret_id"),
                    ("tencent_secret_key", "tencent_secret_key"),
                ],
                key,
                value,
            );
            drop(cfg);
            state
                .translator
                .update_provider_credentials("tencent", creds)
                .await?;
        }
        "baidu_app_id" | "baidu_secret_key" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &cfg,
                &[
                    ("baidu_app_id", "baidu_app_id"),
                    ("baidu_secret_key", "baidu_secret_key"),
                ],
                key,
                value,
            );
            drop(cfg);
            state
                .translator
                .update_provider_credentials("baidu", creds)
                .await?;
        }
        "youdao_app_key" | "youdao_app_secret" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &cfg,
                &[
                    ("youdao_app_key", "youdao_app_key"),
                    ("youdao_app_secret", "youdao_app_secret"),
                ],
                key,
                value,
            );
            drop(cfg);
            state
                .translator
                .update_provider_credentials("youdao", creds)
                .await?;
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
        "clipboard_monitor_enabled" => {
            if value == "true" {
                state.clipboard_monitor.resume();
            } else {
                state.clipboard_monitor.suspend();
            }
            tray::refresh_menu(app).await;
        }
        _ => {}
    }
    Ok(())
}

/// 广播主题变更事件给所有已打开的 WebviewWindow
fn emit_theme_changed(app: &AppHandle, theme: &str) {
    #[derive(serde::Serialize, Clone)]
    struct ThemePayload {
        theme: String,
    }

    let _ = app.emit(
        "theme-changed",
        ThemePayload {
            theme: theme.to_string(),
        },
    );
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
