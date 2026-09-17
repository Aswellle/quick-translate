// src-tauri/src/system/popup_watch.rs
// 被动态浮窗的关闭看守（计划第 19/20 节）。
//
// 背景：Phase 8b 把浮窗改成**非激活**窗口（`focusable(false)` →
// Windows 上的 `WS_EX_NOACTIVATE`）。它因此不再抢焦点，但代价是——
//
//   `onFocusChanged(false)` 永远不会触发。
//
// 而那是此前**唯一**的自动关闭机制。窗口从不激活，就不会失去焦点，
// 也就没有事件可听。不补一套新的关闭机制，浮窗就会永远留在屏幕上
// （计划第 20 节明令禁止的「为了看起来不抢焦点而留下 Popup 永远不消失」）。
//
// 方案（计划第 20 节的方案 A）：记住浮窗出现时前台窗口是谁，
// 之后只要前台**变了**，就说明用户已经切去别处，关闭浮窗。
//
// 用户主动点击浮窗时走另一条路：那一刻窗口被重新设为可激活并聚焦，
// 焦点事件恢复，关闭交回给前端的 `onFocusChanged`。两条路互斥，
// 由 `watching` 标志切换 —— 否则刚被用户点亮的浮窗会被看守立刻关掉
// （它自己成了前台窗口，看起来就像「用户切走了」）。

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::system::translation_flow::POPUP_LABEL;

/// 轮询间隔。
///
/// 用户切走窗口后，浮窗最多多留这么久。150ms 短到察觉不出，
/// 又不至于像 16ms 那样做无谓的 CPU 唤醒。
const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// 当前前台窗口的句柄。0 表示查询不可用。
#[cfg(target_os = "windows")]
fn foreground_window() -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: 无参数、无副作用的 Win32 查询
    unsafe { GetForegroundWindow() as isize }
}

#[cfg(not(target_os = "windows"))]
fn foreground_window() -> isize {
    0
}

/// 是否应因「用户切走了」而关闭浮窗。
///
/// 抽成纯函数：Win32 那半边在单元测试里跑不起来，但这套判定可以 ——
/// 而判错的代价是浮窗莫名其妙消失，或者永远不消失。
///
/// `foreground == 0`（查询失败）时**不关**：查询失败是本地问题，
/// 不该让用户的浮窗凭空消失。
fn should_close(foreground: isize, baseline: isize) -> bool {
    foreground != 0 && baseline != 0 && foreground != baseline
}

/// 关闭浮窗并停止看守。
///
/// `hide_popup` 命令与看守线程共用这一条路径。两条关闭路径各写一遍的话，
/// 迟早会漏掉某一步 —— 这几步每一步都有非踩不可的理由：
///
///   `set_focusable(false)`  浮窗复用同一个窗口。用户点过它之后窗口是
///                           可激活的，不复位的话**下一次翻译会直接抢焦点**，
///                           8b 的成果当场作废。
///   `stop()`                停掉看守，否则它会在下次显示前误判。
///   `reset_last_text()`     不重置的话，关闭后约 1 秒浮窗会因剪贴板里的
///                           残留内容自己弹回来（F8）。
pub fn hide_popup_now(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        let _ = window.set_focusable(false);
        if let Err(e) = window.hide() {
            // 关不掉只记录：这条路径可能来自看守线程，没有调用方能接住错误
            tracing::warn!("[hide_popup_now] 隐藏浮窗失败: {}", e);
        }
    }
    let state = app.state::<AppState>();
    state.popup_watch.stop();
    state.clipboard_monitor.reset_last_text();
}

/// 被动态看守。
///
/// 一条常驻线程，空闲时不动作。不用「每次显示都起一条线程」是因为那样
/// 每条线程都要考虑何时退出，而浮窗一天可能显示上千次（计划第 41 节：
/// 不要每个事件起一个长期任务）。
pub struct PopupWatch {
    /// 是否处于被动态监视中
    watching: AtomicBool,
    /// 浮窗出现时的前台窗口句柄
    baseline: AtomicIsize,
}

impl PopupWatch {
    pub fn start(app: AppHandle) -> Arc<Self> {
        let watch = Arc::new(Self {
            watching: AtomicBool::new(false),
            baseline: AtomicIsize::new(0),
        });

        let w = watch.clone();
        thread::spawn(move || loop {
            thread::sleep(POLL_INTERVAL);

            if !w.watching.load(Ordering::SeqCst) {
                continue;
            }

            let current = foreground_window();
            if should_close(current, w.baseline.load(Ordering::SeqCst)) {
                tracing::info!(
                    event = "popup_closed_by_foreground_change",
                    "[popup_watch] 用户已切走，关闭被动态浮窗"
                );
                w.stop();
                hide_popup_now(&app);
            }
        });

        watch
    }

    /// 浮窗以被动方式显示之后调用：记录当前前台窗口作为基线。
    ///
    /// **必须在窗口显示之后调用** —— 基线要反映「浮窗出现时用户正在用谁」。
    pub fn begin(&self) {
        self.baseline.store(foreground_window(), Ordering::SeqCst);
        self.watching.store(true, Ordering::SeqCst);
    }

    /// 停止监视。用户点击浮窗（转为交互态）或浮窗被关闭时调用。
    pub fn stop(&self) {
        self.watching.store(false, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn is_watching(&self) -> bool {
        self.watching.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_when_the_foreground_window_changes() {
        assert!(should_close(1234, 5678), "前台换了 → 用户切走，应关闭");
    }

    #[test]
    fn staying_open_while_the_foreground_is_unchanged() {
        assert!(
            !should_close(1234, 1234),
            "前台没变 → 用户还在原处，不该关闭"
        );
    }

    /// 查询失败（返回 0）时绝不能关闭 —— 那会让浮窗凭空消失
    #[test]
    fn a_failed_query_never_closes_the_popup() {
        assert!(!should_close(0, 1234), "查询失败不该被当成「用户切走了」");
    }

    /// 基线为 0（显示时就没查到前台窗口）时同样不关
    #[test]
    fn a_zero_baseline_never_closes_the_popup() {
        assert!(!should_close(1234, 0), "没有基线就无法判断，宁可不关");
        assert!(!should_close(0, 0));
    }

    #[test]
    fn watcher_starts_idle() {
        let w = PopupWatch {
            watching: AtomicBool::new(false),
            baseline: AtomicIsize::new(0),
        };
        assert!(!w.is_watching(), "未显示浮窗时看守不该动作");

        w.stop();
        assert!(!w.is_watching());
    }
}
