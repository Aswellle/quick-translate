// src-tauri/src/system/hotkeys.rs
// 全局快捷键（计划第 6 节 Task 11）。
//
// 三条动作都复用既有入口，不另建业务路径：
// - 翻译剪贴板 → coordinator::execute_at_position（敏感过滤、代际闸门、
//   浮窗与历史/缓存落盘全部照旧）
// - 打开手动翻译 → tray::open_manual_translate_window（同一个窗口实例）
// - 暂停/恢复监控 → 与设置页同一条配置写入 + 副作用路径
//
// 注册失败（被其他应用占用、加速键语法非法、平台不支持）**不是启动
// 失败**：状态记录在 HotkeyManager，设置页如实显示，应用照常运行。
// 快捷键改动先注册新值、成功后才注销旧值 —— 新值冲突时旧的仍然可用。

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::state::AppState;
use crate::types::{AppConfig, ToastPayload};

/// 动作标识（前端与状态表共用）
pub const ACTION_TRANSLATE_CLIPBOARD: &str = "translate_clipboard";
pub const ACTION_OPEN_MANUAL: &str = "open_manual";
pub const ACTION_TOGGLE_MONITOR: &str = "toggle_monitor";
pub const ACTION_TOGGLE_POPUP: &str = "toggle_popup";

pub struct HotkeyActionDef {
    pub id: &'static str,
    pub config_key: &'static str,
    pub default_accelerator: &'static str,
    pub label: &'static str,
}

/// 动作清单：默认加速键、配置键、展示名。
///
/// 默认值避开最常见的全局占用（Ctrl+Alt+字母在 Windows/macOS 上通常
/// 空闲；冲突仍可能发生，冲突时设置页会显示真实错误并保留旧值）。
pub const ACTIONS: &[HotkeyActionDef] = &[
    HotkeyActionDef {
        id: ACTION_TRANSLATE_CLIPBOARD,
        config_key: "hotkey_translate_clipboard",
        default_accelerator: "CmdOrControl+Alt+T",
        label: "翻译剪贴板",
    },
    HotkeyActionDef {
        id: ACTION_OPEN_MANUAL,
        config_key: "hotkey_open_manual",
        default_accelerator: "CmdOrControl+Alt+M",
        label: "打开手动翻译",
    },
    HotkeyActionDef {
        id: ACTION_TOGGLE_MONITOR,
        config_key: "hotkey_toggle_monitor",
        default_accelerator: "CmdOrControl+Alt+P",
        label: "暂停/恢复剪贴板监控",
    },
    HotkeyActionDef {
        id: ACTION_TOGGLE_POPUP,
        config_key: "hotkey_toggle_popup",
        default_accelerator: "CmdOrControl+Alt+H",
        label: "显示/隐藏翻译浮窗",
    },
];

fn action_def(id: &str) -> Option<&'static HotkeyActionDef> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// 配置键 → 动作 id（设置管线把三个加速键当作普通字符串键持久化，
/// 副作用分支据此重新注册）。
pub fn action_for_config_key(key: &str) -> Option<&'static str> {
    ACTIONS.iter().find(|a| a.config_key == key).map(|a| a.id)
}

/// 单个动作的注册状态（前端「快捷键」分组的数据源）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HotkeyStatus {
    pub action: String,
    pub label: String,
    /// 配置里保存的加速键（空串 = 已禁用）
    pub configured: String,
    /// 当前真正注册成功的加速键。注册失败时保留旧值 —— 失败不清空
    /// 正在工作的快捷键。
    pub active: Option<String>,
    /// 最近一次注册失败的原因（如被其他应用占用）。成功时为 None。
    pub error: Option<String>,
}

pub struct HotkeyManager {
    statuses: Mutex<Vec<HotkeyStatus>>,
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotkeyManager {
    pub fn new() -> Self {
        let statuses = ACTIONS
            .iter()
            .map(|a| HotkeyStatus {
                action: a.id.to_string(),
                label: a.label.to_string(),
                configured: a.default_accelerator.to_string(),
                active: None,
                error: None,
            })
            .collect();
        Self {
            statuses: Mutex::new(statuses),
        }
    }

    /// 状态快照（前端命令使用）
    pub fn snapshot(&self) -> Vec<HotkeyStatus> {
        self.statuses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 启动时按配置注册全部动作。键缺失（旧库升级）用默认值。
    pub fn register_from_config(&self, app: &AppHandle, config: &AppConfig) {
        for action in ACTIONS {
            let configured = configured_value(config, action.id)
                .unwrap_or_else(|| action.default_accelerator.to_string());
            if let Err(e) = self.apply(app, action.id, &configured) {
                tracing::warn!(
                    event = "hotkey_register_failed",
                    action = action.id,
                    accelerator = %configured,
                    "[hotkeys] 快捷键注册失败（不影响启动）: {}",
                    e
                );
            }
        }
    }

    /// 应用一个动作的加速键：先注册新值，成功后注销旧值。
    ///
    /// 空串 = 禁用该动作。注册失败时旧注册保持不动，错误返回给调用方
    /// （设置页显示「已保存，但生效失败」）。
    pub fn apply(&self, app: &AppHandle, action: &str, configured: &str) -> Result<(), String> {
        let Some(def) = action_def(action) else {
            return Err(format!("未知快捷键动作: {}", action));
        };

        let mut statuses = self.statuses.lock().unwrap_or_else(|e| e.into_inner());
        let Some(status) = statuses.iter_mut().find(|s| s.action == def.id) else {
            return Err(format!("快捷键动作缺少状态槽: {}", action));
        };

        let previous_active = status.active.clone();
        let configured = configured.trim().to_string();

        // 值没变且已注册：无事可做（避免「注销自己再注册」的窗口期）
        if status.active.as_deref() == Some(configured.as_str()) {
            status.configured = configured;
            status.error = None;
            return Ok(());
        }

        if configured.is_empty() {
            if let Some(old) = previous_active {
                if let Err(e) = app.global_shortcut().unregister(old.as_str()) {
                    tracing::warn!("[hotkeys] 注销 {} 失败: {}", old, e);
                }
            }
            status.configured = configured;
            status.active = None;
            status.error = None;
            return Ok(());
        }

        let action_id = def.id.to_string();
        let registered =
            app.global_shortcut()
                .on_shortcut(configured.as_str(), move |app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        dispatch(app, &action_id);
                    }
                });

        match registered {
            Ok(()) => {
                if let Some(old) = previous_active {
                    if old != configured {
                        if let Err(e) = app.global_shortcut().unregister(old.as_str()) {
                            tracing::warn!("[hotkeys] 注销旧快捷键 {} 失败: {}", old, e);
                        }
                    }
                }
                status.configured = configured.clone();
                status.active = Some(configured);
                status.error = None;
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                status.configured = configured;
                status.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// 注销全部（当前未在运行时调用；为完整性与测试保留）
    #[allow(dead_code)]
    pub fn unregister_all(&self, app: &AppHandle) {
        if let Err(e) = app.global_shortcut().unregister_all() {
            tracing::warn!("[hotkeys] 注销全部快捷键失败: {}", e);
        }
        let mut statuses = self.statuses.lock().unwrap_or_else(|e| e.into_inner());
        for status in statuses.iter_mut() {
            status.active = None;
        }
    }
}

/// 从配置对象读某个动作的加速键。None = 配置里没有该键（用默认值）。
fn configured_value(config: &AppConfig, action: &str) -> Option<String> {
    match action {
        ACTION_TRANSLATE_CLIPBOARD => Some(config.hotkey_translate_clipboard.clone()),
        ACTION_OPEN_MANUAL => Some(config.hotkey_open_manual.clone()),
        ACTION_TOGGLE_MONITOR => Some(config.hotkey_toggle_monitor.clone()),
        ACTION_TOGGLE_POPUP => Some(config.hotkey_toggle_popup.clone()),
        _ => None,
    }
}

/// 快捷键动作分发。全部转到既有入口，不新增业务逻辑。
fn dispatch(app: &AppHandle, action: &str) {
    match action {
        ACTION_TRANSLATE_CLIPBOARD => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                translate_current_clipboard(&app).await;
            });
        }
        ACTION_OPEN_MANUAL => {
            crate::system::tray::open_manual_translate_window(app);
        }
        ACTION_TOGGLE_MONITOR => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                toggle_monitor(&app).await;
            });
        }
        ACTION_TOGGLE_POPUP => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                toggle_popup(&app).await;
            });
        }
        _ => {}
    }
}

/// 读取当前剪贴板并走与剪贴板监控完全相同的入口。
///
/// 显式快捷键动作不改变监控开关 —— 监控暂停时用户仍可手动触发一次。
/// 敏感过滤照常生效（隐私默认值不因触发方式改变）。
async fn translate_current_clipboard(app: &AppHandle) {
    let text = match tauri::async_runtime::spawn_blocking(read_clipboard_text).await {
        Ok(Some(text)) => text,
        Ok(None) => {
            tracing::info!(
                event = "hotkey_clipboard_unavailable",
                "[hotkeys] 剪贴板无文本内容，忽略翻译快捷键"
            );
            return;
        }
        Err(e) => {
            tracing::warn!("[hotkeys] 读取剪贴板任务失败: {}", e);
            return;
        }
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }

    let (x, y) = crate::system::clipboard::get_cursor_position();
    tracing::info!(
        event = "hotkey_translate_clipboard",
        len = text.len(),
        "[hotkeys] 快捷键触发翻译剪贴板"
    );
    crate::system::translation::coordinator::execute_at_position(app, x, y, text).await;
}

fn read_clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
}

/// 暂停/恢复剪贴板监控：与设置页同一条配置写入 + 副作用路径。
async fn toggle_monitor(app: &AppHandle) {
    let state = app.state::<AppState>();
    let enabled = state
        .config
        .read()
        .await
        .get("clipboard_monitor_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);
    let next = !enabled;
    let value = if next { "true" } else { "false" };

    if let Err(e) = state
        .config
        .write()
        .await
        .set("clipboard_monitor_enabled", value)
        .await
    {
        tracing::warn!("[hotkeys] 切换剪贴板监控失败: {}", e);
        crate::commands::system::emit_toast(app, ToastPayload::error("切换剪贴板监控失败，请重试"));
        return;
    }

    crate::commands::config::apply_clipboard_monitor_switch(app, state.inner(), next).await;
    crate::commands::config::emit_config_changed(app, &state.config.read().await.get_all());
    crate::commands::system::emit_toast(
        app,
        ToastPayload::info(if next {
            "剪贴板监控已恢复"
        } else {
            "剪贴板监控已暂停"
        }),
    );
}

/// 显示/隐藏翻译浮窗。
///
/// 窗口不存在（从未翻译过）时如实提示，而不是创建一个空白浮窗。
/// 显示复用 translation_flow 的完整路径（拖拽位置优先、复位不可聚焦、
/// 看守基线重置）；隐藏复用 popup_watch 的唯一关闭路径。
async fn toggle_popup(app: &AppHandle) {
    use crate::system::{popup_watch, translation_flow};

    if app.get_webview_window("popup").is_none() {
        crate::commands::system::emit_toast(app, ToastPayload::info("还没有翻译内容可显示"));
        return;
    }

    let visible = app
        .get_webview_window("popup")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        popup_watch::hide_popup_now(app);
        return;
    }

    let (x, y) = crate::system::clipboard::get_cursor_position();
    let position = translation_flow::compute_popup_position_dpi(app, x, y);
    if translation_flow::show_popup_window(app, &position).await {
        let state = app.state::<AppState>();
        state.popup_watch.begin();
        state
            .runtime
            .report_popup(crate::runtime::ComponentState::Healthy, None);
        tracing::info!(event = "popup_show_by_hotkey", "[hotkeys] 快捷键显示浮窗");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_are_unique_and_resolvable() {
        let mut ids: Vec<&str> = ACTIONS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ACTIONS.len(), "动作 id 必须唯一");
        for action in ACTIONS {
            assert!(action_def(action.id).is_some());
            assert!(action_def(action.id).map(|a| a.config_key) == Some(action.config_key));
        }
    }

    #[test]
    fn default_accelerators_are_non_empty_and_distinct() {
        let mut accels: Vec<&str> = ACTIONS.iter().map(|a| a.default_accelerator).collect();
        accels.sort_unstable();
        accels.dedup();
        assert_eq!(accels.len(), ACTIONS.len(), "默认加速键不能重复");
        for accel in accels {
            assert!(!accel.trim().is_empty());
            assert!(
                accel.contains('+'),
                "默认加速键必须带修饰键，避免抢占普通按键"
            );
        }
    }

    /// 默认配置下每个动作都能从 AppConfig 读到自己的默认值。
    #[test]
    fn default_config_exposes_every_action_accelerator() {
        let config = AppConfig::default();
        for action in ACTIONS {
            assert_eq!(
                configured_value(&config, action.id).as_deref(),
                Some(action.default_accelerator),
                "动作 {} 的默认值未贯通到 AppConfig",
                action.id
            );
        }
    }

    fn status_for(action: &str) -> HotkeyStatus {
        HotkeyManager::new()
            .snapshot()
            .into_iter()
            .find(|s| s.action == action)
            .expect("每个动作都有状态槽")
    }

    #[test]
    fn initial_status_is_unregistered_with_default_value() {
        let status = status_for(ACTION_TRANSLATE_CLIPBOARD);
        assert_eq!(status.configured, "CmdOrControl+Alt+T");
        assert_eq!(status.active, None);
        assert_eq!(status.error, None);
        assert!(!status.label.is_empty());
    }
}
