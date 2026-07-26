// src-tauri/src/system/clipboard_monitor.rs
// 后台剪贴板监控：监听剪贴板文本变化，自动触发翻译浮窗
// 使用跨平台 arboard + Tokio 实现轮询式监控

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::system::clipboard;

/// 读取系统剪贴板序列号：每次剪贴板被写入都会自增。
///
/// 用于区分"同一段文本被用户重新复制"（序列号变化）与"文本原地未动"
/// （序列号不变）。仅靠文本内容比较无法区分这两种情况。
#[cfg(target_os = "windows")]
fn clipboard_seq() -> u32 {
    // SAFETY: 无参数、无副作用的 Win32 查询调用，失败时返回 0
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

/// 非 Windows 平台无对应原语，返回常量 0 使判定退化为纯文本比较。
/// 代价：关闭浮窗后重新复制同一段文本不会再触发（不会出现浮窗弹回）。
#[cfg(not(target_os = "windows"))]
fn clipboard_seq() -> u32 {
    0
}

/// 监控任务配置
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// 防抖延迟：剪贴板内容变化后等待此时间再触发翻译
const DEBOUNCE_DELAY: Duration = Duration::from_millis(400);

/// 控制器句柄：可在运行时暂停/恢复监控，并请求重置 last_text
#[derive(Clone)]
pub struct MonitorController {
    /// 暂停标志（true = 暂停中，不执行翻译检测）
    pub suspended: Arc<AtomicBool>,
    /// hide_popup 请求重置标志：下次循环时清空 last_text，
    /// 确保关闭浮窗后再次复制相同文本仍能触发翻译
    pub reset_requested: Arc<AtomicBool>,
    /// app 主动写剪贴板标志：下次循环时将当前内容静默吸收进 last_text，
    /// 防止 copy_to_clipboard（复制译文/原文按钮）触发二次翻译
    pub app_wrote_clipboard: Arc<AtomicBool>,
}

/// 启动剪贴板监控后台任务（在 lib.rs setup 中调用）
pub fn start_monitor(app: AppHandle) -> MonitorController {
    let controller = MonitorController {
        suspended: Arc::new(AtomicBool::new(false)),
        reset_requested: Arc::new(AtomicBool::new(false)),
        app_wrote_clipboard: Arc::new(AtomicBool::new(false)),
    };
    let controller_thread = controller.clone();

    std::thread::spawn(move || {
        clipboard_monitor_thread(app.clone(), Arc::new(controller_thread));
    });

    tracing::info!("[start_monitor] 监控线程已启动，initial suspended=false");
    controller
}

impl MonitorController {
    /// 暂停监控（暂停期间不触发翻译）
    pub fn suspend(&self) {
        let was = self.suspended.load(Ordering::SeqCst);
        self.suspended.store(true, Ordering::SeqCst);
        tracing::info!("[MonitorController] suspend() called: {} -> suspended={}", was, true);
    }

    /// 恢复监控
    pub fn resume(&self) {
        let was = self.suspended.load(Ordering::SeqCst);
        self.suspended.store(false, Ordering::SeqCst);
        tracing::info!("[MonitorController] resume() called: {} -> suspended={}", was, false);
    }

    /// 是否处于暂停状态
    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::SeqCst)
    }

    /// 请求监控线程在下次循环时重置 last_text。
    /// 由 hide_popup 调用，确保关闭浮窗后再次复制相同文本能重新触发翻译。
    pub fn reset_last_text(&self) {
        self.reset_requested.store(true, Ordering::SeqCst);
        tracing::info!("[MonitorController] reset_last_text() 已请求");
    }

    /// 通知监控线程 app 刚主动写入剪贴板（复制译文/原文按钮）。
    /// 下次循环时将新内容静默吸收进 last_text，不触发翻译。
    pub fn mark_app_write(&self) {
        self.app_wrote_clipboard.store(true, Ordering::SeqCst);
        tracing::info!("[MonitorController] mark_app_write() 已标记");
    }

    /// 撤销 mark_app_write：写剪贴板失败时调用，避免残留标志
    /// 吞掉用户下一次真实复制（F5）。
    pub fn unmark_app_write(&self) {
        self.app_wrote_clipboard.store(false, Ordering::SeqCst);
        tracing::info!("[MonitorController] unmark_app_write() 已撤销（写入失败）");
    }
}

/// 后台线程主循环：轮询剪贴板，检测文本变化并防抖触发翻译
fn clipboard_monitor_thread(app: AppHandle, controller: Arc<MonitorController>) {
    tracing::info!("[clipboard_monitor_thread] 线程开始运行");

    // 创建一次剪贴板句柄并全程复用：
    // 避免每 500ms 反复调用 arboard::Clipboard::new()（Windows 下每次
    // 都打开/关闭系统剪贴板 API，约 120 次/分钟）
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => {
            tracing::error!("[clipboard_monitor] 剪贴板初始化失败，监控线程退出: {}", e);
            return;
        }
    };

    let mut last_text: Option<String> = None;
    let mut pending_text: Option<String> = None;
    let mut pending_timer: Option<std::time::Instant> = None;
    // 上次判定时的剪贴板序列号，用于识别"同一段文本被重新复制"
    let mut last_seq: u32 = clipboard_seq();

    loop {
        // 暂停时等待，不消耗 CPU
        if controller.is_suspended() {
            thread::sleep(Duration::from_millis(200));
            // 重置待处理内容，避免恢复时立即触发
            pending_text = None;
            pending_timer = None;
            continue;
        }

        // 处理 hide_popup 发出的重置请求。
        //
        // 此前这里把 last_text 置为 None，意图是"关闭后再次复制相同文本仍能触发"。
        // 但剪贴板里那段文本并没有消失，下一轮轮询就把它当成全新内容，防抖到期后
        // 立刻重新翻译 —— 浮窗关掉约 1 秒又自己弹回来，红色按钮/空格键看起来失效。
        //
        // 改为"吸收"当前剪贴板内容并记录剪贴板序列号：
        //   同文本 + 同序列号 → 原地未动，不触发（浮窗保持关闭）
        //   同文本 + 序列号变化 → 用户真的重新复制了一次，正常触发
        if controller.reset_requested.swap(false, Ordering::SeqCst) {
            last_text = clipboard.get_text().ok();
            last_seq = clipboard_seq();
            pending_text = None;
            pending_timer = None;
            tracing::info!(
                "[clipboard_monitor] hide_popup 触发：已吸收当前剪贴板内容 seq={}",
                last_seq
            );
        }

        thread::sleep(POLL_INTERVAL);

        // 暂停检测（避免在 sleep 期间被暂停导致丢失一轮检测）
        if controller.is_suspended() {
            pending_text = None;
            pending_timer = None;
            continue;
        }

        let current = match clipboard.get_text() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let current_normalized = clipboard::normalize_text(&current);

        // app 主动写入剪贴板（复制译文/原文）：静默吸收，不触发翻译
        if controller.app_wrote_clipboard.swap(false, Ordering::SeqCst) {
            tracing::info!("[clipboard_monitor] 检测到 app 写入，已吸收 len={}", current_normalized.len());
            last_text = Some(current);
            // 同步序列号：app 的写入本身会让序列号自增，若不同步则下一轮
            // 会因"文本相同但序列号变化"被误判成用户重新复制
            last_seq = clipboard_seq();
            pending_text = None;
            pending_timer = None;
            continue;
        }

        // 跳过空白或极短内容
        if current_normalized.trim().len() < 2 {
            pending_text = None;
            pending_timer = None;
            continue;
        }

        // 检测是否是新内容。
        // 文本变化 → 新内容；文本相同但剪贴板序列号变化 → 用户重新复制了同一段文本，
        // 同样视为新内容（这是关闭浮窗后重复复制仍能触发翻译的依据）。
        let current_seq = clipboard_seq();
        let is_new = match &last_text {
            Some(prev) => {
                let prev_norm = clipboard::normalize_text(prev);
                current_normalized != prev_norm || current_seq != last_seq
            }
            None => true,
        };

        if is_new {
            last_text = Some(current);
            last_seq = current_seq;
            pending_text = Some(current_normalized);
            pending_timer = Some(std::time::Instant::now());
        } else {
            // 内容未变，检查防抖是否到期
            if let (Some(text), Some(start)) = (&pending_text, pending_timer) {
                if start.elapsed() >= DEBOUNCE_DELAY {
                    let text_clone = text.clone();
                    pending_text = None;
                    pending_timer = None;
                    tracing::info!("[clipboard_monitor] 防抖到期，触发翻译 len={}", text_clone.len());

                    let app_clone = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let did_trigger = trigger_translation(&app_clone, &text_clone).await;
                        if !did_trigger {
                            tracing::info!("[clipboard_monitor] 翻译被跳过（onboarding 未完成），下次复制相同文本将重新触发");
                        }
                    });
                }
            }
        }
    }
}

/// 触发翻译流程（异步，在 Tokio task 中调用）
/// 返回 true = popup 已显示；返回 false = 被跳过（onboarding 未完成）
async fn trigger_translation(app: &AppHandle, text: &str) -> bool {
    let state = app.state::<AppState>();

    // 向导未完成时，不弹出翻译浮窗
    let onboarding_done = state.is_onboarding_complete().await;
    tracing::info!(
        "[trigger_translation] is_onboarding_complete={}, text_len={}",
        onboarding_done,
        text.len()
    );
    if !onboarding_done {
        tracing::info!("[trigger_translation] 跳过：向导未完成");
        return false;
    }

    if text.trim().is_empty() {
        return false;
    }

    let (cx, cy) = clipboard::get_cursor_position();
    // 将翻译流程完整委托给 translation_flow，包含：
    // 取消前序任务、读 target_lang、计算浮窗位置、显示 loading、执行翻译
    crate::system::translation_flow::execute_at_position(app, cx, cy, text.to_string()).await;
    true
}
