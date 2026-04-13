// src-tauri/src/state.rs
// 全局共享状态容器，通过 app.manage() 注入，command handler 通过 tauri::State<AppState> 获取

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::domain::config::ConfigService;
use crate::domain::history::HistoryRepository;
use crate::domain::translator::TranslationEngine;
use crate::infra::http_client::HttpClient;

/// Tauri managed state
///
/// 所有字段均用 Arc 包装，Clone 仅复制引用计数，开销极低。
/// tray.rs 中的 tokio::spawn 闭包需要 'static，通过 clone 传入拥有值。
#[derive(Clone)]
pub struct AppState {
    pub translator: Arc<TranslationEngine>,
    pub config: Arc<RwLock<ConfigService>>,
    pub history: Arc<Mutex<HistoryRepository>>,
    pub http_client: Arc<HttpClient>,
    pub current_translation: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
}
