// src-tauri/src/system/tray.rs
// 系统托盘初始化、菜单构建、动态刷新、事件处理

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

use crate::error::AppError;
use crate::runtime::{ComponentState, RuntimeHealth, RuntimeStatusSnapshot};
use crate::state::AppState;
use crate::types::ProviderInfo;

/// 托盘图标的唯一 ID，用于后续通过 app.tray_by_id() 找回实例
const TRAY_ID: &str = "main";

/// 初始化系统托盘（在 lib.rs setup 中调用一次）
/// setup() 为同步上下文，blocking_read() 在此处安全。
pub fn init(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    let current_provider = state
        .config
        .blocking_read()
        .get("provider")
        .unwrap_or_else(|| "google".to_string());
    let current_lang = state
        .config
        .blocking_read()
        .get("target_lang")
        .unwrap_or_else(|| "zh".to_string());
    let clipboard_enabled = state
        .config
        .blocking_read()
        .get("clipboard_monitor_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);

    // 获取提供者列表以显示可用性（setup 阶段已注册完毕，sync 安全）
    let providers = state.translator.list_providers_sync();
    tracing::info!(
        "[tray::init] clipboard_monitor_enabled={} (from config)",
        clipboard_enabled
    );

    let menu = build_menu(
        app,
        &current_provider,
        &current_lang,
        clipboard_enabled,
        &providers,
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("QuickTranslate")
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(|_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                // 左键单击托盘图标：目前无操作，通过右键菜单操作
            }
        })
        .build(app)
        .map_err(|e| AppError::WindowError(format!("托盘初始化失败: {}", e)))?;

    tracing::info!(
        "系统托盘已初始化（clipboard_enabled={}）",
        clipboard_enabled
    );
    Ok(())
}

/// 动态刷新托盘菜单（切换 provider / target_lang 后调用）
///
/// async 版本：避免在 Tokio runtime 线程中调用 blocking_read() 导致 panic。
pub async fn refresh_menu(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config.read().await;
    let current_provider = cfg.get("provider").unwrap_or_else(|| "google".to_string());
    let current_lang = cfg.get("target_lang").unwrap_or_else(|| "zh".to_string());
    let clipboard_enabled = cfg
        .get("clipboard_monitor_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);
    drop(cfg);

    let providers = state.translator.list_providers().await;
    tracing::info!("[refresh_menu] clipboard_enabled={}", clipboard_enabled);

    match build_menu(
        app,
        &current_provider,
        &current_lang,
        clipboard_enabled,
        &providers,
    ) {
        Ok(new_menu) => {
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                if let Err(e) = tray.set_menu(Some(new_menu)) {
                    tracing::warn!("托盘菜单刷新失败: {}", e);
                } else {
                    tracing::info!("[refresh_menu] 菜单刷新成功");
                }
            }
        }
        Err(e) => {
            tracing::warn!("托盘菜单构建失败: {}", e);
        }
    }
}

/// 构建托盘右键菜单（接收已读好的配置值，不再自行访问 RwLock）
fn build_menu(
    app: &AppHandle,
    current_provider: &str,
    current_lang: &str,
    clipboard_monitor_enabled: bool,
    providers: &[ProviderInfo],
) -> Result<Menu<tauri::Wry>, AppError> {
    // ---- 翻译源子菜单（⚠ 标记需要 API Key 但尚未配置的服务）----
    let providers_meta = [
        ("deepl", "DeepL"),
        ("tencent", "腾讯翻译"),
        ("baidu", "百度翻译"),
        ("youdao", "有道翻译"),
        ("google", "Google Translate"),
    ];

    let provider_items: Vec<MenuItem<tauri::Wry>> = providers_meta
        .iter()
        .map(|(id, label)| {
            let check = if *id == current_provider { "✓" } else { "  " };
            // 找到对应的运行时 provider 信息，判断是否已配置凭证
            let is_unavailable = providers
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.requires_api_key && !p.is_available)
                .unwrap_or(false);
            let suffix = if is_unavailable { " ⚠" } else { "" };
            let display = format!("{} {}{}", check, label, suffix);
            MenuItem::with_id(app, format!("provider:{}", id), display, true, None::<&str>)
                .map_err(|e| AppError::WindowError(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let provider_items_ref: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = provider_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();

    let provider_submenu = Submenu::with_items(app, "翻译服务", true, &provider_items_ref)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    // ---- 目标语言子菜单 ----
    let languages = [
        ("zh", "🇨🇳 简体中文"),
        ("en", "🇺🇸 English"),
        ("ja", "🇯🇵 日本語"),
        ("ko", "🇰🇷 한국어"),
        ("fr", "🇫🇷 Français"),
        ("de", "🇩🇪 Deutsch"),
        ("es", "🇪🇸 Español"),
        ("ru", "🇷🇺 Русский"),
    ];

    let lang_items: Vec<MenuItem<tauri::Wry>> = languages
        .iter()
        .map(|(code, label)| {
            let display = if *code == current_lang {
                format!("✓ {}", label)
            } else {
                format!("   {}", label)
            };
            MenuItem::with_id(app, format!("lang:{}", code), display, true, None::<&str>)
                .map_err(|e| AppError::WindowError(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let lang_items_ref: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = lang_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();

    let lang_submenu = Submenu::with_items(app, "目标语言", true, &lang_items_ref)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    // ---- 状态区（计划第 27 节）----
    //
    // 只读本地运行时状态。托盘菜单里绝不发网络请求 —— 打开菜单是个
    // 高频动作，而状态早就在内存里了。
    let status = app.state::<AppState>().runtime.snapshot();
    let status_item = MenuItem::with_id(
        app,
        "runtime_status",
        status_headline(status.overall),
        false,
        None::<&str>,
    )
    .map_err(|e| AppError::WindowError(e.to_string()))?;

    let mut status_items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> =
        vec![Box::new(status_item)];
    for line in status_details(&status) {
        status_items.push(Box::new(
            MenuItem::with_id(app, "runtime_detail", line, false, None::<&str>)
                .map_err(|e| AppError::WindowError(e.to_string()))?,
        ));
    }
    let status_separator =
        PredefinedMenuItem::separator(app).map_err(|e| AppError::WindowError(e.to_string()))?;

    // ---- 剪贴板监控开关 ----
    let clip_check = if clipboard_monitor_enabled {
        "✓"
    } else {
        "  "
    };
    let clip_label = format!(
        "{} 剪贴板监控{}",
        clip_check,
        clipboard_suffix(status.clipboard.state)
    );
    let clipboard_item = MenuItem::with_id(
        app,
        "clipboard_monitor_toggle",
        clip_label,
        true,
        None::<&str>,
    )
    .map_err(|e| AppError::WindowError(e.to_string()))?;

    // ---- 主菜单 ----
    let history_item = MenuItem::with_id(app, "history", "翻译历史", true, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let separator =
        PredefinedMenuItem::separator(app).map_err(|e| AppError::WindowError(e.to_string()))?;

    let version = app.package_info().version.to_string();
    let about_label = format!("QuickTranslate v{}", version);
    let about_item = MenuItem::with_id(app, "about", about_label, false, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = Vec::new();
    for item in &status_items {
        items.push(item.as_ref());
    }
    items.push(&status_separator);
    items.push(&provider_submenu);
    items.push(&lang_submenu);
    items.push(&history_item);
    items.push(&settings_item);
    items.push(&clipboard_item);
    items.push(&separator);
    items.push(&about_item);
    items.push(&quit_item);

    Menu::with_items(app, &items).map_err(|e| AppError::WindowError(e.to_string()))
}

/// 托盘顶部那一行总状态（计划第 27 节）。
///
/// 用「部分功能异常」而不是「已损坏」之类的词：故障的 provider 只该
/// 影响它自己那一行，不该让整个应用看起来像坏了（计划第 28 节的同一条原则）。
fn status_headline(health: RuntimeHealth) -> &'static str {
    match health {
        RuntimeHealth::Healthy => "● 正常运行",
        RuntimeHealth::Degraded => "● 部分功能异常",
        RuntimeHealth::Recovering => "● 正在恢复…",
    }
}

/// 剪贴板开关项上的状态后缀。健康时不加任何字，保持菜单干净。
fn clipboard_suffix(state: ComponentState) -> &'static str {
    match state {
        ComponentState::Degraded => "  ⚠ 暂时异常",
        ComponentState::Recovering => "  ⚠ 自动恢复中…",
        // Disabled 由前面的 ✓ 空格已经表达，健康则无需提示
        ComponentState::Healthy | ComponentState::Disabled => "",
    }
}

/// 出问题时才追加的说明行。健康时返回空 —— 一个正常的应用不该在托盘里
/// 铺满「一切正常」的噪音。
fn status_details(status: &RuntimeStatusSnapshot) -> Vec<String> {
    let mut lines = Vec::new();

    if status.network.state == ComponentState::Degraded {
        lines.push("  ⚠ 翻译服务暂时不可用，将自动恢复".to_string());
    }
    if status.storage.state == ComponentState::Degraded {
        lines.push("  ⚠ 历史记录暂时无法写入".to_string());
    }
    if status.popup.state == ComponentState::Degraded {
        lines.push("  ⚠ 浮窗显示异常".to_string());
    }

    lines
}

/// 处理托盘菜单点击事件
fn handle_menu_event(app: &AppHandle, event_id: &str) {
    tracing::debug!("托盘菜单事件: {}", event_id);

    match event_id {
        "settings" => open_settings_window(app),
        "history" => open_history_window(app),
        "quit" => {
            tracing::info!("用户退出应用");
            app.exit(0);
        }
        "about" => { /* 版本号已直接展示在菜单项 label 中，此处无需操作 */ }
        "clipboard_monitor_toggle" => {
            toggle_clipboard_monitor(app.clone());
        }
        id if id.starts_with("provider:") => {
            let provider_id = id["provider:".len()..].to_string();
            switch_provider(app.clone(), provider_id);
        }
        id if id.starts_with("lang:") => {
            let lang_code = id["lang:".len()..].to_string();
            switch_target_lang(app.clone(), lang_code);
        }
        _ => {}
    }
}

fn open_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    match WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("index.html#settings".into()),
    )
    .title("QuickTranslate 设置")
    .additional_browser_args(crate::system::BROWSER_ARGS)
    .inner_size(600.0, 480.0)
    .min_inner_size(500.0, 400.0)
    .resizable(true)
    .center()
    .build()
    {
        Ok(w) => {
            let _ = w.show();
        }
        Err(e) => {
            tracing::error!("打开设置面板失败: {}", e);
        }
    }
}

fn open_history_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("history") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    match WebviewWindowBuilder::new(app, "history", WebviewUrl::App("index.html#history".into()))
        .title("翻译历史")
        .additional_browser_args(crate::system::BROWSER_ARGS)
        .inner_size(720.0, 540.0)
        .min_inner_size(600.0, 400.0)
        .resizable(true)
        .center()
        .build()
    {
        Ok(w) => {
            let _ = w.show();
        }
        Err(e) => {
            tracing::error!("打开历史面板失败: {}", e);
        }
    }
}

/// 切换翻译源：先更新翻译引擎（验证合法性），再持久化配置，最后刷新托盘
/// 顺序很关键：引擎更新在前，确保 provider_id 有效；若持久化失败则回滚引擎
fn switch_provider(app: AppHandle, provider_id: String) {
    let state = app.state::<AppState>().inner().clone();
    let app_for_menu = app.clone();

    tauri::async_runtime::spawn(async move {
        // 先更新引擎（会检查 provider_id 是否已注册）
        if let Err(e) = state.translator.set_active_provider(&provider_id).await {
            tracing::error!("切换翻译源失败（未知 provider '{}'）: {}", provider_id, e);
            return;
        }
        // 再持久化到 DB
        if let Err(e) = state
            .config
            .write()
            .await
            .set("provider", &provider_id)
            .await
        {
            tracing::error!("切换翻译源持久化失败: {}", e);
            // 持久化失败时回滚引擎，保持引擎与 DB 一致
            let fallback = state
                .config
                .read()
                .await
                .get("provider")
                .unwrap_or_default();
            let _ = state.translator.set_active_provider(&fallback).await;
            return;
        }
        tracing::info!("已切换翻译源: {}", provider_id);
        refresh_menu(&app_for_menu).await;
    });
}

/// 切换目标语言：更新配置 + 刷新托盘菜单
fn switch_target_lang(app: AppHandle, lang_code: String) {
    let state = app.state::<AppState>().inner().clone();
    let app_for_menu = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = state
            .config
            .write()
            .await
            .set("target_lang", &lang_code)
            .await
        {
            tracing::error!("切换目标语言失败: {}", e);
            return;
        }
        tracing::info!("已切换目标语言: {}", lang_code);
        refresh_menu(&app_for_menu).await;
    });
}

/// 切换剪贴板监控状态：更新配置 + 控制器 + 刷新托盘菜单
fn toggle_clipboard_monitor(app: AppHandle) {
    let state = app.state::<AppState>().inner().clone();
    let app_for_menu = app.clone();

    tauri::async_runtime::spawn(async move {
        let current = state
            .config
            .read()
            .await
            .get("clipboard_monitor_enabled")
            .map(|v| v == "true")
            .unwrap_or(true);
        let new_value = !current;
        tracing::info!(
            "[toggle_clipboard_monitor] 当前值={}, 切换到={}",
            current,
            new_value
        );

        if let Err(e) = state
            .config
            .write()
            .await
            .set(
                "clipboard_monitor_enabled",
                if new_value { "true" } else { "false" },
            )
            .await
        {
            tracing::error!("切换剪贴板监控失败: {}", e);
            return;
        }

        if new_value {
            state.clipboard_monitor.resume();
            tracing::info!("[toggle_clipboard_monitor] 已调用 resume()，剪贴板监控已启用");
        } else {
            state.clipboard_monitor.suspend();
            tracing::info!("[toggle_clipboard_monitor] 已调用 suspend()，剪贴板监控已禁用");
        }

        refresh_menu(&app_for_menu).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::ComponentHealth;

    fn comp(state: ComponentState) -> ComponentHealth {
        ComponentHealth {
            state,
            ..ComponentHealth::default()
        }
    }

    fn snapshot(
        overall: RuntimeHealth,
        network: ComponentState,
        storage: ComponentState,
        popup: ComponentState,
    ) -> RuntimeStatusSnapshot {
        RuntimeStatusSnapshot {
            overall,
            uptime_ms: 0,
            clipboard: comp(ComponentState::Healthy),
            network: comp(network),
            popup: comp(popup),
            storage: comp(storage),
        }
    }

    fn healthy() -> RuntimeStatusSnapshot {
        snapshot(
            RuntimeHealth::Healthy,
            ComponentState::Healthy,
            ComponentState::Healthy,
            ComponentState::Healthy,
        )
    }

    // ── 顶部状态行 ───────────────────────────────────────────────────────

    #[test]
    fn headline_covers_every_health_level() {
        assert_eq!(status_headline(RuntimeHealth::Healthy), "● 正常运行");
        assert_eq!(status_headline(RuntimeHealth::Degraded), "● 部分功能异常");
        assert_eq!(status_headline(RuntimeHealth::Recovering), "● 正在恢复…");
    }

    // ── 剪贴板开关的后缀 ─────────────────────────────────────────────────

    /// 健康时不加任何字样 —— 菜单要干净，正常不需要被反复告知
    #[test]
    fn healthy_clipboard_adds_no_noise() {
        assert_eq!(clipboard_suffix(ComponentState::Healthy), "");
    }

    /// 用户自己关掉的，前面那个 ✓ 空格已经说明了，不必再写一遍
    #[test]
    fn disabled_clipboard_adds_no_noise() {
        assert_eq!(clipboard_suffix(ComponentState::Disabled), "");
    }

    #[test]
    fn degraded_and_recovering_clipboard_are_flagged() {
        assert!(clipboard_suffix(ComponentState::Degraded).contains("异常"));
        assert!(clipboard_suffix(ComponentState::Recovering).contains("恢复中"));
    }

    // ── 说明行 ───────────────────────────────────────────────────────────

    /// 一切正常时不该在托盘里铺满「一切正常」的噪音
    #[test]
    fn a_healthy_snapshot_has_no_detail_lines() {
        assert!(status_details(&healthy()).is_empty());
    }

    #[test]
    fn only_the_faulty_components_get_a_line() {
        let s = snapshot(
            RuntimeHealth::Degraded,
            ComponentState::Degraded,
            ComponentState::Healthy,
            ComponentState::Healthy,
        );
        let lines = status_details(&s);

        assert_eq!(lines.len(), 1, "只该有网络那一行：{:?}", lines);
        assert!(lines[0].contains("翻译服务"));
        assert!(
            !lines.iter().any(|l| l.contains("历史")),
            "存储是健康的，不该出现它的说明行"
        );
    }

    /// 计划第 28 节的原则：故障的 provider 不该让整个应用看起来像坏了。
    /// 对应到托盘 —— 说明行只描述出问题的那个组件，不扩散。
    #[test]
    fn one_broken_component_does_not_produce_a_wall_of_warnings() {
        let s = snapshot(
            RuntimeHealth::Degraded,
            ComponentState::Degraded,
            ComponentState::Degraded,
            ComponentState::Healthy,
        );
        let lines = status_details(&s);

        assert_eq!(lines.len(), 2, "只有两个组件有问题：{:?}", lines);
        for line in &lines {
            assert!(line.starts_with("  ⚠ "), "说明行格式应一致: {}", line);
        }
    }

    #[test]
    fn every_faulty_component_gets_exactly_one_line() {
        let s = snapshot(
            RuntimeHealth::Degraded,
            ComponentState::Degraded,
            ComponentState::Degraded,
            ComponentState::Degraded,
        );
        assert_eq!(status_details(&s).len(), 3);
    }
}
