// src-tauri/src/lib.rs
// 库 crate 入口（Tauri 2 标准结构要求）
// [lib] 声明要求此文件存在；main.rs 调用 run() 启动应用

pub mod commands;
pub mod domain;
pub mod error;
pub mod infra;
pub mod runtime;
pub mod state;
pub mod system;
pub mod types;
pub mod util;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};

use domain::config::ConfigService;
use domain::history::HistoryRepository;
use domain::translator::{build_provider, TranslationEngine, CREDENTIAL_KEYS};
use infra::{crypto, database, http_client::HttpClient};
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
        // ── 单实例保护（必须最先注册）──────────────────────────────────────
        // 两个实例会争用同一个 WebView2 用户数据目录：安装器「运行程序」
        // 勾选项启动新实例时，若旧实例仍在（更新安装、手动双击、自启重叠），
        // 向导/浮窗等窗口创建会偶发失败。第二个实例在此回调后自动退出。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(wizard) = app.get_webview_window("onboarding") {
                let _ = wizard.show();
                let _ = wizard.set_focus();
            }
        }))
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
                    tracing::info!(
                        "[on_window_event] {} 窗口 close 请求，prevent_close+hide",
                        label
                    );
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let app_handle = app.handle().clone();

            // ── Step 0: WebView2 用户数据目录清理 ─────────────────────────────
            // 必须在任何 webview 进程创建之前执行（否则缓存文件被锁定删不掉）。
            // EBWebView 下 GPU/磁盘缓存无上限累积（Tauri#8145 / WebView2Feedback#4410），
            // 长期使用可膨胀至数 GB；这些目录全部可再生，删除无损。
            if let Ok(local_data_dir) = app.path().app_local_data_dir() {
                cleanup_webview_cache(local_data_dir.join("EBWebView"));
            }

            // ── Step 1: 初始化基础设施层 ──────────────────────────────────────
            let app_data_dir = app.path().app_data_dir().expect("无法获取 App Data 目录");

            // 日志系统需要 app_data_dir，在此初始化
            init_logging(&app_data_dir);

            tracing::info!("App Data 目录: {:?}", app_data_dir);

            // ── 启动韧性分级（计划第 26 节）────────────────────────────────
            // 数据库初始化失败是 Recoverable 故障：log + 降级 + 继续启动。
            // 历史与缓存功能降级（读空、写拒绝），翻译主链路完全不受影响。
            // 真正 Critical 的只有 Tauri runtime 本身起不来。
            let db = match database::init_db(&app_data_dir) {
                Ok(conn) => {
                    let db = database::Db::available(Arc::new(Mutex::new(conn)));
                    tracing::info!("[setup] 数据库初始化成功");
                    db
                }
                Err(e) => {
                    tracing::error!(
                        event = "database_init_failed",
                        "数据库初始化失败，历史/缓存功能降级，进程继续启动: {}",
                        e
                    );
                    database::Db::unavailable()
                }
            };
            let http_client = Arc::new(HttpClient::new());

            // 初始化机器绑定随机密钥（必须在 ConfigService::load 之前，确保
            // 加解密使用新版密钥；旧密钥数据会在 load 中自动迁移）。
            // 密钥文件损坏/不可写由 init 内部降级处理（重新生成或临时密钥），
            // 不再用 expect 中止启动。
            if let Err(e) = crypto::init_per_install_secret(&app_data_dir) {
                tracing::error!(
                    event = "machine_secret_init_failed",
                    "机器密钥初始化失败（加密功能降级）: {}",
                    e
                );
            }

            // ── Step 2: 初始化 Domain 层 ─────────────────────────────────────
            let config = ConfigService::load(db.clone());
            let config = Arc::new(RwLock::new(config));
            // 无外层 Mutex：HistoryRepository 内部已有 Arc<Mutex<Connection>>，双重加锁无益
            let history = Arc::new(HistoryRepository::new(db.clone()));
            let cache = Arc::new(domain::cache::TranslationCache::new(db.clone()));

            // ── Step 3: 注册翻译源 ────────────────────────────────────────────
            let translator = TranslationEngine::new(http_client.clone());

            // 凭证按 CREDENTIAL_KEYS 统一取；注册顺序即 fallback 默认优先级。
            // 与设置面板改 Key 的路径共用同一份字段定义，不会出现「加了字段
            // 却只改了一处」。CREDENTIAL_KEYS 的字段名就是配置 key。
            let creds_by_provider: Vec<(String, std::collections::HashMap<String, String>)> = {
                let cfg = config.blocking_read();
                CREDENTIAL_KEYS
                    .iter()
                    .map(|(id, keys)| {
                        let creds = keys
                            .iter()
                            .map(|k| ((*k).to_string(), cfg.get_credential(k)))
                            .collect();
                        ((*id).to_string(), creds)
                    })
                    .collect()
            };

            tauri::async_runtime::block_on(async {
                for (id, creds) in creds_by_provider {
                    match build_provider(&id, &creds, http_client.clone()) {
                        Ok(provider) => translator.register_provider(provider).await,
                        Err(e) => {
                            // 单个翻译源构造失败不该拖垮启动：跳过它，其余照常注册
                            tracing::error!("翻译源 {} 构造失败，已跳过: {}", id, e);
                        }
                    }
                }

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
            tracing::info!(
                "[setup] clipboard_monitor_enabled={} (from config)",
                clipboard_monitor_enabled
            );
            let monitor = Arc::new(system::clipboard::start_monitor(app_handle.clone()));
            if !clipboard_monitor_enabled {
                tracing::info!("[setup] 调用 monitor.suspend()（config 为 false）");
                monitor.suspend();
            }

            // ── Step 4.5: 运行时状态层（计划第 4 节）────────────────────────
            // 构造顺序有依赖：runtime 需要 controller 才能派生剪贴板健康，
            // 而 controller 需要在 runtime 建好之后才能接上（二者互相引用）。
            let runtime = runtime::RuntimeStatus::new(monitor.clone(), {
                let handle = app_handle.clone();
                Arc::new(move |snapshot: &runtime::RuntimeStatusSnapshot| {
                    use tauri::Emitter;
                    let _ = handle.emit("runtime-status-changed", snapshot);

                    // 托盘菜单一旦设定就是静态的，状态变了必须重建 ——
                    // 否则用户看到的永远是上一次重建时的状态。
                    // 这里只会在状态**真的变了**时被调到（RuntimeStatus
                    // 内部有指纹去重），所以不会变成高频重建。
                    let tray_handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        system::tray::refresh_menu(&tray_handle).await;
                    });
                })
            });
            monitor.attach_runtime(runtime.clone());

            // 存储降级也要让运行时层知道：托盘/设置显示「部分功能异常」，
            // 而不是让用户从「历史是空的」自行猜测（计划第 4/26 节）。
            if !db.is_available() {
                runtime.report_storage(
                    crate::runtime::ComponentState::Degraded,
                    Some("STORAGE_UNAVAILABLE"),
                );
            }

            // 历史与缓存的写入统一走这一条有界队列 + 单 worker（计划第 24 节）
            let persistence = Arc::new(system::persistence::PersistenceWriter::spawn(
                history.clone(),
                cache.clone(),
                runtime.clone(),
            ));

            // 被动态浮窗的关闭看守（Phase 8b）：浮窗不抢焦点后就失去了
            // onFocusChanged 这条关闭路径，由它按前台窗口变化补上。
            // 一条常驻线程，未显示浮窗时不动作。
            let popup_watch = system::popup_watch::PopupWatch::start(app_handle.clone());

            let app_state = AppState {
                translator,
                config: config.clone(),
                history,
                cache,
                persistence,
                runtime,
                popup_watch,
                http_client,
                coordinator: Arc::new(system::translation::TranslationCoordinator::new()),
                clipboard_monitor: monitor,
            };
            app.manage(app_state);

            // 托盘初始化失败同样是 Recoverable（计划第 26 节）：
            // 托盘是入口之一，但翻译、剪贴板、浮窗不依赖它。log + 继续启动，
            // 用户仍可通过浮窗完成翻译，重启后托盘通常能恢复。
            if let Err(e) = system::tray::init(&app_handle) {
                tracing::error!(
                    event = "tray_init_failed",
                    "系统托盘初始化失败，功能降级（翻译/浮窗不受影响）: {}",
                    e
                );
            }

            // 启动后 5s 后台静默检查更新
            let update_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                system::updater::check_and_notify(&update_handle).await;
            });

            // 首次启动：立即显示居中向导窗口（独立于剪贴板状态，popup 窗口
            // 此时通常尚未创建，App.tsx 的挂载检查鞭长莫及）。
            // 此前是一次性 spawn 且 `let _ =` 吞掉创建失败 —— 向导窗口偶发
            // 创建失败时首启就永远没有向导，也没有日志可查。这里改为有限
            // 重试：每次都记录结果，向导真正打开（或确认已完成）才停止。
            let onboarding_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                for attempt in 1..=3u32 {
                    // 递增间隔：给托盘/窗口初始化让路，也错开与前端并发检测的竞争
                    tokio::time::sleep(std::time::Duration::from_millis(300 * u64::from(attempt)))
                        .await;
                    let complete = onboarding_handle
                        .state::<crate::state::AppState>()
                        .is_onboarding_complete()
                        .await;
                    if complete {
                        tracing::info!("[setup] onboarding 已完成，无需打开向导");
                        return;
                    }
                    match commands::system::open_onboarding_window(onboarding_handle.clone()).await
                    {
                        Ok(()) => {
                            tracing::info!("[setup] 引导向导窗口已打开（第 {} 次尝试）", attempt);
                            return;
                        }
                        Err(e) => {
                            tracing::error!("[setup] 打开引导向导失败（第 {} 次）: {}", attempt, e);
                        }
                    }
                }
            });

            tracing::info!("QuickTranslate 初始化完成，剪贴板监控已启动");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::translate::translate_text,
            commands::translate::list_providers,
            commands::translate::get_provider_status,
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
            commands::system::activate_popup,
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
            commands::system::get_popup_geometry,
            commands::system::get_runtime_status,
            commands::system::get_runtime_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("QuickTranslate 启动失败");
}

/// 删除 WebView2 用户数据目录下可再生的缓存子目录（磁盘体积控制）。
///
/// 必须在任何 webview 创建之前调用；删除失败（文件被占用）只告警不阻塞启动。
/// 保留 `Default/` 根目录下的 profile 数据（Local Storage 等），只清纯缓存。
fn cleanup_webview_cache(ebwebview_dir: PathBuf) {
    const VOLATILE_DIRS: &[&str] = &[
        "GrShaderCache",
        "ShaderCache",
        "GPUCache",
        "BrowserMetrics",
        "GPUPersistentCache",
        "DawnWebGPUCache",
        "DawnGraphiteCache",
        "Default/Cache",
        "Default/Code Cache",
        "Default/GPUCache",
        "Default/DawnWebGPUCache",
        "Default/DawnGraphiteCache",
    ];
    for rel in VOLATILE_DIRS {
        let dir = ebwebview_dir.join(rel);
        if !dir.exists() {
            continue;
        }
        match std::fs::remove_dir_all(&dir) {
            Ok(_) => tracing::info!("[cleanup_webview_cache] 已删除缓存目录: {}", rel),
            Err(e) => tracing::warn!("[cleanup_webview_cache] 删除 {} 失败: {}", rel, e),
        }
    }
}

/// 初始化日志系统（stdout + 持久化滚动文件）
fn init_logging(app_data_dir: &Path) {
    use tracing_appender::rolling;
    use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let logs_dir = app_data_dir.join("logs");
    // 用 Builder 显式限定 max_log_files：`rolling::daily()` 简写永不清理旧日志，
    // 日滚动文件会无限累积。保留最近 14 天。
    let file_appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("quicktranslate.log")
        .max_log_files(14)
        .build(&logs_dir)
        .expect("日志 appender 初始化失败");
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
