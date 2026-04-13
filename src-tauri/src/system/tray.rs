// src-tauri/src/system/tray.rs
// 系统托盘初始化、菜单构建、动态刷新、事件处理

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

use crate::error::AppError;
use crate::state::AppState;

/// 托盘图标的唯一 ID，用于后续通过 app.tray_by_id() 找回实例
const TRAY_ID: &str = "main";

/// 初始化系统托盘（在 lib.rs setup 中调用一次）
pub fn init(app: &AppHandle) -> Result<(), AppError> {
    let menu = build_menu(app)?;

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

    tracing::info!("系统托盘已初始化");
    Ok(())
}

/// 动态刷新托盘菜单（切换 provider / target_lang 后调用）
///
/// 重新构建 Menu 对象并通过 set_menu 原地热更新，无闪烁、无重启。
pub fn refresh_menu(app: &AppHandle) {
    match build_menu(app) {
        Ok(new_menu) => {
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                if let Err(e) = tray.set_menu(Some(new_menu)) {
                    tracing::warn!("托盘菜单刷新失败: {}", e);
                }
            }
        }
        Err(e) => {
            tracing::warn!("托盘菜单构建失败: {}", e);
        }
    }
}

/// 构建托盘右键菜单（从配置中读取当前勾选状态）
fn build_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, AppError> {
    let state = app.state::<AppState>();
    let current_provider = state
        .config
        .blocking_read()
        .get("provider")
        .unwrap_or_else(|| "deepl".to_string());

    let current_lang = state
        .config
        .blocking_read()
        .get("target_lang")
        .unwrap_or_else(|| "zh".to_string());

    // ---- 翻译源子菜单（可用性感知：未配置凭证的显示灰色标记）----
    let providers_meta = [
        ("deepl",   "DeepL"),
        ("tencent", "腾讯翻译"),
        ("baidu",   "百度翻译"),
        ("youdao",  "有道翻译"),
        ("google",  "Google Translate"),
    ];

    let provider_items: Vec<MenuItem<tauri::Wry>> = providers_meta
        .iter()
        .map(|(id, label)| {
            let check = if *id == current_provider { "✓" } else { "  " };
            let display = format!("{} {}", check, label);
            MenuItem::with_id(app, format!("provider:{}", id), display, true, None::<&str>)
                .map_err(|e| AppError::WindowError(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let provider_items_ref: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        provider_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();

    let provider_submenu = Submenu::with_items(app, "翻译服务", true, &provider_items_ref)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    // ---- 目标语言子菜单 ----
    let languages = [
        ("zh",    "🇨🇳 简体中文"),
        ("en",    "🇺🇸 English"),
        ("ja",    "🇯🇵 日本語"),
        ("ko",    "🇰🇷 한국어"),
        ("fr",    "🇫🇷 Français"),
        ("de",    "🇩🇪 Deutsch"),
        ("es",    "🇪🇸 Español"),
        ("ru",    "🇷🇺 Русский"),
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

    let lang_items_ref: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        lang_items
            .iter()
            .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
            .collect();

    let lang_submenu = Submenu::with_items(app, "目标语言", true, &lang_items_ref)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    // ---- 主菜单 ----
    let history_item = MenuItem::with_id(app, "history", "翻译历史", true, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let separator = PredefinedMenuItem::separator(app)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    let about_item =
        MenuItem::with_id(app, "about", "关于 QuickTranslate", true, None::<&str>)
            .map_err(|e| AppError::WindowError(e.to_string()))?;

    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|e| AppError::WindowError(e.to_string()))?;

    Menu::with_items(
        app,
        &[
            &provider_submenu,
            &lang_submenu,
            &history_item,
            &settings_item,
            &separator,
            &about_item,
            &quit_item,
        ],
    )
    .map_err(|e| AppError::WindowError(e.to_string()))
}

/// 处理托盘菜单点击事件
fn handle_menu_event(app: &AppHandle, event_id: &str) {
    tracing::debug!("托盘菜单事件: {}", event_id);

    match event_id {
        "settings" => open_settings_window(app),
        "history"  => open_history_window(app),
        "quit" => {
            tracing::info!("用户退出应用");
            app.exit(0);
        }
        "about" => { /* TODO: About 对话框 */ }
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
    .inner_size(600.0, 480.0)
    .min_inner_size(500.0, 400.0)
    .resizable(true)
    .center()
    .build()
    {
        Ok(w)  => { let _ = w.show(); }
        Err(e) => { tracing::error!("打开设置面板失败: {}", e); }
    }
}

fn open_history_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("history") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    match WebviewWindowBuilder::new(
        app,
        "history",
        WebviewUrl::App("index.html#history".into()),
    )
    .title("翻译历史")
    .inner_size(720.0, 540.0)
    .min_inner_size(600.0, 400.0)
    .resizable(true)
    .center()
    .build()
    {
        Ok(w)  => { let _ = w.show(); }
        Err(e) => { tracing::error!("打开历史面板失败: {}", e); }
    }
}

/// 切换翻译源：更新配置 + 翻译引擎 + 刷新托盘菜单
fn switch_provider(app: AppHandle, provider_id: String) {
    let state = app.state::<AppState>().inner().clone();
    let app_for_menu = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = state.config.write().await.set("provider", &provider_id).await {
            tracing::error!("切换翻译源失败: {}", e);
            return;
        }
        if let Err(e) = state.translator.set_active_provider(&provider_id).await {
            tracing::error!("更新翻译引擎失败: {}", e);
        }
        tracing::info!("已切换翻译源: {}", provider_id);
        // 配置已写入，刷新菜单勾选状态
        refresh_menu(&app_for_menu);
    });
}

/// 切换目标语言：更新配置 + 刷新托盘菜单
fn switch_target_lang(app: AppHandle, lang_code: String) {
    let state = app.state::<AppState>().inner().clone();
    let app_for_menu = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = state.config.write().await.set("target_lang", &lang_code).await {
            tracing::error!("切换目标语言失败: {}", e);
            return;
        }
        tracing::info!("已切换目标语言: {}", lang_code);
        // 配置已写入，刷新菜单勾选状态
        refresh_menu(&app_for_menu);
    });
}
