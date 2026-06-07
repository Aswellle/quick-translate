// src-tauri/src/state.rs
// 全局共享状态容器，通过 app.manage() 注入，command handler 通过 tauri::State<AppState> 获取

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::domain::config::ConfigService;
use crate::domain::history::HistoryRepository;
use crate::domain::translator::TranslationEngine;
use crate::infra::http_client::HttpClient;
use crate::system::clipboard_monitor::MonitorController;

/// Tauri managed state
///
/// 所有字段均用 Arc 包装，Clone 仅复制引用计数，开销极低。
/// tray.rs 中的 tokio::spawn 闭包需要 'static，通过 clone 传入拥有值。
///
/// history 使用 Arc<HistoryRepository>（无外层 Mutex）：
/// HistoryRepository 内部已通过 Arc<Mutex<Connection>> 序列化所有 DB 操作，
/// 外层再加 Mutex 是多余的双重加锁，会将所有历史命令串行化到单一队列。
#[derive(Clone)]
pub struct AppState {
    pub translator: Arc<TranslationEngine>,
    pub config: Arc<RwLock<ConfigService>>,
    pub history: Arc<HistoryRepository>,
    pub http_client: Arc<HttpClient>,
    pub current_translation: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub clipboard_monitor: Arc<MonitorController>,
}

impl AppState {
    /// 检查引导向导是否已完成（异步，供 async context 调用）
    pub async fn is_onboarding_complete(&self) -> bool {
        self.config
            .read()
            .await
            .get("onboarding_completed")
            .map(|v| v == "true")
            .unwrap_or(false)
    }
}
