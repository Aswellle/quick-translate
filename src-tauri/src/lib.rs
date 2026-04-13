// src-tauri/src/lib.rs
// 库 crate 入口（Tauri 2 标准结构要求）
// [lib] 声明要求此文件存在；main.rs 调用 run() 启动应用

pub mod commands;
pub mod domain;
pub mod error;
pub mod infra;
pub mod state;
pub mod system;
pub mod types;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};

use domain::config::ConfigService;
use domain::history::HistoryRepository;
use domain::translator::{
    baidu::BaiduProvider, deepl::DeepLProvider, google::GoogleProvider,
    tencent::TencentProvider, youdao::YoudaoProvider, TranslationEngine,
};
use infra::{database, http_client::HttpClient};
use state::AppState;

/// 应用主入口，由 main.rs 的 fn main() 调用
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        // ── 关闭窗口时隐藏而非退出进程 ──────────────────────────────────────
        // Tauri 2 默认：最后一个窗口关闭 → 进程退出。
        // QuickTranslate 是系统托盘应用，窗口仅是辅助 UI，不应控制进程生命周期。
        // on_window_close_requested 返回 false 阻止关闭，改为 hide()。
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // popup 窗口正常关闭（由 hide_popup command 控制），
                // settings/history 窗口 × 按钮 → 隐藏而非销毁
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let app_handle = app.handle().clone();

            // ── Step 1: 初始化基础设施层 ──────────────────────────────────────
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("无法获取 App Data 目录");

            tracing::info!("App Data 目录: {:?}", app_data_dir);

            let conn = database::init_db(&app_data_dir).expect("数据库初始化失败");
            let db = Arc::new(Mutex::new(conn));
            let http_client = Arc::new(HttpClient::new());

            // ── Step 2: 初始化 Domain 层 ─────────────────────────────────────
            let config = ConfigService::load(db.clone()).expect("配置加载失败");
            let config = Arc::new(RwLock::new(config));
            let history = Arc::new(Mutex::new(HistoryRepository::new(db.clone())));

            // ── Step 3: 注册翻译源 ────────────────────────────────────────────
            let translator = TranslationEngine::new(http_client.clone());

            let deepl_api_key      = config.blocking_read().get_credential("deepl_api_key");
            let tencent_secret_id  = config.blocking_read().get_credential("tencent_secret_id");
            let tencent_secret_key = config.blocking_read().get_credential("tencent_secret_key");
            let baidu_app_id       = config.blocking_read().get_credential("baidu_app_id");
            let baidu_secret_key   = config.blocking_read().get_credential("baidu_secret_key");
            let youdao_app_key     = config.blocking_read().get_credential("youdao_app_key");
            let youdao_app_secret  = config.blocking_read().get_credential("youdao_app_secret");

            tauri::async_runtime::block_on(async {
                translator.register_provider(Box::new(DeepLProvider::new(
                    http_client.clone(), deepl_api_key))).await;
                translator.register_provider(Box::new(TencentProvider::new(
                    http_client.clone(), tencent_secret_id, tencent_secret_key))).await;
                translator.register_provider(Box::new(BaiduProvider::new(
                    http_client.clone(), baidu_app_id, baidu_secret_key))).await;
                translator.register_provider(Box::new(YoudaoProvider::new(
                    http_client.clone(), youdao_app_key, youdao_app_secret))).await;
                translator.register_provider(Box::new(GoogleProvider::new(
                    http_client.clone()))).await;

                let active_provider = config.read().await
                    .get("provider")
                    .unwrap_or_else(|| "google".to_string());
                let _ = translator.set_active_provider(&active_provider).await;

                let fallback = config.read().await
                    .get("fallback_enabled")
                    .map(|v| v == "true")
                    .unwrap_or(true);
                translator.set_fallback_enabled(fallback).await;
            });

            let translator = Arc::new(translator);

            // ── Step 4: 组装全局状态 ─────────────────────────────────────────
            let app_state = AppState {
                translator,
                config: config.clone(),
                history,
                http_client,
                current_translation: Arc::new(Mutex::new(None)),
            };
            app.manage(app_state);

            // ── Step 5: 初始化系统服务 ────────────────────────────────────────
            let hotkey = config
                .blocking_read()
                .get("hotkey")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Ctrl+Shift+D".to_string());

            system::hotkey::register_initial(&app_handle, &hotkey)
                .expect("全局快捷键注册失败");

            system::tray::init(&app_handle).expect("系统托盘初始化失败");

            // 启动后 5s 后台静默检查更新
            let update_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                system::updater::check_and_notify(&update_handle).await;
            });

            tracing::info!("QuickTranslate 初始化完成，快捷键: {}", hotkey);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::translate::translate_text,
            commands::translate::list_providers,
            commands::translate::validate_provider,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::set_config_batch,
            commands::history::query_history,
            commands::history::count_history,
            commands::history::clear_history,
            commands::system::copy_to_clipboard,
            commands::system::hide_popup,
            commands::system::get_app_version,
            commands::system::notify_toast,
            commands::system::get_autostart,
            commands::system::set_autostart,
            commands::system::check_update,
            commands::system::check_onboarding,
            commands::system::complete_onboarding,
        ])
        .run(tauri::generate_context!())
        .expect("QuickTranslate 启动失败");
}

/// 初始化日志系统
fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .compact()
        .init();
}
