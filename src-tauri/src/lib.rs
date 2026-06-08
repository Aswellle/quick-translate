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

use std::path::Path;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};

use domain::config::ConfigService;
use domain::history::HistoryRepository;
use domain::translator::{
    baidu::BaiduProvider, deepl::DeepLProvider, google::GoogleProvider, tencent::TencentProvider,
    youdao::YoudaoProvider, TranslationEngine,
};
use infra::{database, http_client::HttpClient};
use state::AppState;

/// 应用主入口，由 main.rs 的 fn main() 调用
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 安装 panic hook，将 panic 信息写入日志（日志系统尚未初始化时 eprintln 兜底）
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        tracing::error!("PANIC at {}: {}", location, payload);
        eprintln!("PANIC at {}: {}", location, payload);
    }));

    tauri::Builder::default()
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
                let label = window.label();
                if label == "onboarding" {
                    // 向导窗口关闭时，确保 popup 窗口存在（首次启动时 popup 可能从未被创建）
                    // ensure_popup_window 已调用 show()，无需再次 show/focus
                    tracing::info!("[on_window_event] onboarding 关闭，开始 ensure_popup_window");
                    let app_h = window.app_handle();
                    system::translation_flow::ensure_popup_window(app_h);
                    let _ = window.close();
                    tracing::info!("[on_window_event] onboarding 已关闭，popup 应该可见");
                } else {
                    // popup 窗口正常关闭（由 hide_popup command 控制），
                    // settings/history 窗口 × 按钮 → 隐藏而非销毁
                    tracing::info!("[on_window_event] {} 窗口 close 请求，prevent_close+hide", label);
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let app_handle = app.handle().clone();

            // ── Step 1: 初始化基础设施层 ──────────────────────────────────────
            let app_data_dir = app.path().app_data_dir().expect("无法获取 App Data 目录");

            // 日志系统需要 app_data_dir，在此初始化
            init_logging(&app_data_dir);

            tracing::info!("App Data 目录: {:?}", app_data_dir);

            let conn = database::init_db(&app_data_dir).expect("数据库初始化失败");
            let db = Arc::new(Mutex::new(conn));
            let http_client = Arc::new(HttpClient::new());

            // ── Step 2: 初始化 Domain 层 ─────────────────────────────────────
            let config = ConfigService::load(db.clone()).expect("配置加载失败");
            let config = Arc::new(RwLock::new(config));
            // 无外层 Mutex：HistoryRepository 内部已有 Arc<Mutex<Connection>>，双重加锁无益
            let history = Arc::new(HistoryRepository::new(db.clone()));

            // ── Step 3: 注册翻译源 ────────────────────────────────────────────
            let translator = TranslationEngine::new(http_client.clone());

            let deepl_api_key = config.blocking_read().get_credential("deepl_api_key");
            let tencent_secret_id = config.blocking_read().get_credential("tencent_secret_id");
            let tencent_secret_key = config.blocking_read().get_credential("tencent_secret_key");
            let baidu_app_id = config.blocking_read().get_credential("baidu_app_id");
            let baidu_secret_key = config.blocking_read().get_credential("baidu_secret_key");
            let youdao_app_key = config.blocking_read().get_credential("youdao_app_key");
            let youdao_app_secret = config.blocking_read().get_credential("youdao_app_secret");

            tauri::async_runtime::block_on(async {
                translator
                    .register_provider(Box::new(DeepLProvider::new(
                        http_client.clone(),
                        deepl_api_key,
                    )))
                    .await;
                translator
                    .register_provider(Box::new(TencentProvider::new(
                        http_client.clone(),
                        tencent_secret_id,
                        tencent_secret_key,
                    )))
                    .await;
                translator
                    .register_provider(Box::new(BaiduProvider::new(
                        http_client.clone(),
                        baidu_app_id,
                        baidu_secret_key,
                    )))
                    .await;
                translator
                    .register_provider(Box::new(YoudaoProvider::new(
                        http_client.clone(),
                        youdao_app_key,
                        youdao_app_secret,
                    )))
                    .await;
                translator
                    .register_provider(Box::new(GoogleProvider::new(http_client.clone())))
                    .await;

                let active_provider = config
                    .read()
                    .await
                    .get("provider")
                    .unwrap_or_else(|| "google".to_string());
                let _ = translator.set_active_provider(&active_provider).await;

                let fallback = config
                    .read()
                    .await
                    .get("fallback_enabled")
                    .map(|v| v == "true")
                    .unwrap_or(true);
                translator.set_fallback_enabled(fallback).await;
            });

            let translator = Arc::new(translator);

            // ── Step 4: 组装全局状态 ─────────────────────────────────────────
            // 启动剪贴板监控（读取配置决定初始是否暂停）
            let clipboard_monitor_enabled = config
                .blocking_read()
                .get("clipboard_monitor_enabled")
                .map(|v| v == "true")
                .unwrap_or(true);
            tracing::info!("[setup] clipboard_monitor_enabled={} (from config)", clipboard_monitor_enabled);
            let monitor = system::clipboard_monitor::start_monitor(app_handle.clone());
            if !clipboard_monitor_enabled {
                tracing::info!("[setup] 调用 monitor.suspend()（config 为 false）");
                monitor.suspend();
            }

            let app_state = AppState {
                translator,
                config: config.clone(),
                history,
                http_client,
                current_translation: Arc::new(Mutex::new(None)),
                clipboard_monitor: Arc::new(monitor),
            };
            app.manage(app_state);

            system::tray::init(&app_handle).expect("系统托盘初始化失败");

            // 启动后 5s 后台静默检查更新
            let update_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                system::updater::check_and_notify(&update_handle).await;
            });

            // 首次启动：立即显示居中向导窗口（独立于剪贴板状态）
            // App.tsx 中的 useEffect 也会检查并打开，作为双重保障
            let onboarding_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                // 短暂延迟确保 popup webview 已完成初始化
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let needed = onboarding_handle
                    .state::<crate::state::AppState>()
                    .is_onboarding_complete()
                    .await;
                tracing::info!("[setup] is_onboarding_complete={}，是否需要打开向导={}", needed, !needed);
                if !needed {
                    let _ = commands::system::open_onboarding_window(onboarding_handle).await;
                }
            });

            tracing::info!("QuickTranslate 初始化完成，剪贴板监控已启动");
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
            commands::history::delete_history_record,
            commands::history::toggle_star_record,
            commands::history::export_history,
            commands::history::get_stats,
            commands::system::copy_to_clipboard,
            commands::system::hide_popup,
            commands::system::resize_popup,
            commands::system::get_app_version,
            commands::system::notify_toast,
            commands::system::get_autostart,
            commands::system::set_autostart,
            commands::system::check_update,
            commands::system::check_onboarding,
            commands::system::complete_onboarding,
            commands::system::open_onboarding_window,
            commands::system::open_url,
            commands::system::set_clipboard_monitor_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("QuickTranslate 启动失败");
}

/// 初始化日志系统（stdout + 持久化滚动文件）
fn init_logging(app_data_dir: &Path) {
    use tracing_appender::rolling;
    use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let logs_dir = app_data_dir.join("logs");
    let file_appender = rolling::daily(&logs_dir, "quicktranslate.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // _guard must stay alive for the duration of the process;
    // leak it intentionally so it is never dropped
    std::mem::forget(_guard);

    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .compact();

    let file_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_ansi(false)
        .with_writer(non_blocking);

    let subscriber = Registry::default()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber).expect("日志系统初始化失败");
}
