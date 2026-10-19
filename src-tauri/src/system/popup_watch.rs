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
// 用户主动点击浮窗后走另一条路：窗口被重新设为可激活并聚焦，键盘事件
// 恢复。但关闭**不再**交给前端的 `onFocusChanged` —— 激活链路
// （set_focusable 异步落flag + startDragging 模态循环 + set_focus）交错时
// 产生的瞬时 blur 会立刻触发 hidePopup，日志表现为
// popup_activated_by_user +96ms → popup_hidden（还带一份 WM_KILLFOCUS
// 回声的双重隐藏）。交互态改为看守继续工作，判定条件升级为
// 「前台 ≠ 基线 且 前台 ≠ 浮窗自己」：浮窗在前台 → 不关；用户点进
// 其他应用 → 关。150ms 轮询对瞬时焦点抖动天然免疫。

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

/// macOS：以最前应用的进程 ID 作为前台标识。
///
/// 被动态浮窗在此平台同样不会收到焦点事件（focusable(false)），若不
/// 提供前台查询，看守永远无法判定「用户切走了」，浮窗只能靠用户手动
/// 点掉 —— 发布流水线实际在分发 macOS dmg，这条路径每个用户每次翻译
/// 都会踩到。NSWorkspace.frontmostApplication 的进程 ID 天然随应用
/// 切换变化，语义与 Windows 的 GetForegroundWindow 等价。
#[cfg(target_os = "macos")]
fn foreground_window() -> isize {
    // objc2 0.6 中这些 NSWorkspace/NSRunningApplication 方法标注为安全
    // （无指针解引用风险），不需要 unsafe 块
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

    let workspace = NSWorkspace::sharedWorkspace();
    let front = workspace.frontmostApplication();
    match front {
        Some(app) => {
            // 守护进程/后台常驻可能返回 nil policy；pid 本身即够作前台标识
            let pid = app.processIdentifier();
            if pid == 0 {
                return 0;
            }
            // 浮窗属于本应用：本应用成为最前应用时不该作为「用户切走」
            // 的信号（与 Windows 行为一致 —— activate 后看守已停，这里是
            // 兜底防线）
            if app.activationPolicy() == NSApplicationActivationPolicy::Regular
                && app
                    .bundleIdentifier()
                    .map(|b| b.to_string().contains("quicktranslate"))
                    .unwrap_or(false)
            {
                return 0;
            }
            pid as isize
        }
        None => 0,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
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

/// 交互态判定：浮窗自己持有前台时**绝不**关 —— 用户刚点完浮窗正准备
/// 按空格，此刻任何「浮窗 ≠ 基线」都是激活本身的产物，不是切走。
///
/// `popup_identity` 语义（`Option`，两平台含义不同）：
///   - `Some(p)`：p 是浮窗的可比标识。`foreground == p` → 浮窗在前台，不关。
///   - `None`：平台无需/无法自证，穿透到基线判定。macOS 上这是**常态**
///     （foreground_window 把本应用折叠成 0：浮窗激活后前台查询即 0 →
///     不关本就是预期；前台变成非 0 的别的应用 → 正常关）。Windows 上
///     hwnd() 取不到只可能是窗口已销毁（窗口存在则句柄必在），此时
///     hide 本就是空操作，穿透无碍。
///
/// `foreground == 0 || baseline == 0` 仍一律不关：查询失败是本地问题，
/// 不该让浮窗凭空消失。
fn should_close_interactive(foreground: isize, baseline: isize, popup: Option<isize>) -> bool {
    if foreground == 0 || baseline == 0 {
        return false;
    }
    if let Some(p) = popup {
        if foreground == p {
            return false;
        }
    }
    foreground != baseline
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
    tracing::info!(event = "popup_hidden", "[popup] 浮窗已隐藏");
    let state = app.state::<AppState>();
    state.popup_watch.stop();
    state.clipboard_monitor.reset_last_text();
    // 注意：**不清除**拖拽位置记忆。用户拖到某处后关闭浮窗，下次翻译
    // 应当回到同一位置 —— 那是用户明确表达过的偏好。只有窗口被销毁
    // （应用退出/窗口重建）时才由 forget_dragged 清掉。
}

/// 被动态看守。
///
/// 一条常驻线程，空闲时不动作。不用「每次显示都起一条线程」是因为那样
/// 每条线程都要考虑何时退出，而浮窗一天可能显示上千次（计划第 41 节：
/// 不要每个事件起一个长期任务）。
pub struct PopupWatch {
    /// 是否处于监视中（被动态与交互态共用同一线程）
    watching: AtomicBool,
    /// 浮窗出现时的前台窗口句柄
    baseline: AtomicIsize,
    /// 当前是否为交互态（用户点击过浮窗）
    interactive: AtomicBool,
}

/// 交互态下用「前台是否是浮窗自己」自证。Windows 用 HWND（取不到 =
/// `None`，交互态保守不关）；macOS 没有 per-window 句柄可比，且
/// `foreground_window()` 已把本应用折叠成 0 —— 前台非 0 就必然是别的
/// 应用，无需自证，恒 `None`。
#[cfg(target_os = "windows")]
fn popup_identity(app: &AppHandle) -> Option<isize> {
    app.get_webview_window(POPUP_LABEL)
        .and_then(|win| win.hwnd().ok().map(|h| h.0 as isize))
}

#[cfg(not(target_os = "windows"))]
fn popup_identity(_app: &AppHandle) -> Option<isize> {
    None
}

impl PopupWatch {
    pub fn start(app: AppHandle) -> Arc<Self> {
        let watch = Arc::new(Self {
            watching: AtomicBool::new(false),
            baseline: AtomicIsize::new(0),
            interactive: AtomicBool::new(false),
        });

        let w = watch.clone();
        thread::spawn(move || loop {
            thread::sleep(POLL_INTERVAL);

            if !w.watching.load(Ordering::SeqCst) {
                continue;
            }

            let current = foreground_window();
            if w.interactive.load(Ordering::SeqCst) {
                // 交互态：浮窗自己可以是前台（用户正读译文/按空格），
                // 只有前台落到别的应用才关。Windows 句柄缺失时保守不关
                // —— 自证手段缺失时延迟关闭好过误关（F 系教训）。
                let popup = popup_identity(&app);
                if should_close_interactive(current, w.baseline.load(Ordering::SeqCst), popup) {
                    tracing::info!(
                        event = "popup_closed_by_foreground_change",
                        "[popup_watch] 交互态用户已切走，关闭浮窗"
                    );
                    w.stop();
                    hide_popup_now(&app);
                }
            } else if should_close(current, w.baseline.load(Ordering::SeqCst)) {
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
        self.interactive.store(false, Ordering::SeqCst);
        self.watching.store(true, Ordering::SeqCst);
    }

    /// 用户点击浮窗、转为交互态：看守**继续**运行，只是判定升级。
    ///
    /// 此前交互态直接停掉看守、把关闭交给前端的 `onFocusChanged` ——
    /// 但激活链路（set_focusable 异步 + startDragging 模态循环 +
    /// set_focus）交错产生的瞬时 blur 会让浮窗在激活后 ~96ms 自关。
    /// 前台轮询是唯一对焦点抖动免疫的关闭权威，两个状态都由它管。
    pub fn enter_interactive(&self) {
        self.interactive.store(true, Ordering::SeqCst);
    }

    /// 停止监视。浮窗被关闭时调用。
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
            interactive: AtomicBool::new(false),
        };
        assert!(!w.is_watching(), "未显示浮窗时看守不该动作");

        w.stop();
        assert!(!w.is_watching());
    }

    // ── 交互态判定 ─────────────────────────────────────────────────────

    /// 核心回归：激活后浮窗自己成了前台 —— 这不是「用户切走」，
    /// 绝不能触发关闭（此前交给前端 onFocusChanged 的路径会在这里自关）
    #[test]
    fn interactive_popup_itself_in_foreground_never_closes() {
        assert!(
            !should_close_interactive(777, 1234, Some(777)),
            "浮窗是前台 → 不关"
        );
    }

    #[test]
    fn interactive_user_switched_away_closes() {
        assert!(
            should_close_interactive(999, 1234, Some(777)),
            "前台换了别的应用 → 关"
        );
    }

    #[test]
    fn interactive_still_on_baseline_does_not_close() {
        assert!(!should_close_interactive(1234, 1234, Some(777)));
    }

    /// 前台查询失败（=0）时不关；None 身份（macOS 常态 / Windows 窗口
    /// 已销毁）穿透到基线判定 —— 见 should_close_interactive 的文档注释
    #[test]
    fn interactive_failed_query_never_closes() {
        assert!(!should_close_interactive(0, 1234, Some(777)));
        assert!(
            should_close_interactive(999, 1234, None),
            "None 穿透：前台非 0 且 ≠ 基线 → 关（macOS 切走的主路径）"
        );
    }

    /// macOS 形态：本应用持有前台时 foreground_window() 返回 0（本应用被
    /// 折叠），浮窗激活后查询即 0 → 不关；切到别的应用后前台非 0 → 正常关。
    /// None 身份在 macOS 上不能触发保守分支，否则交互态永远不自动关闭。
    #[test]
    fn interactive_macos_foreground_folded_to_zero_stays_open_then_closes() {
        assert!(
            !should_close_interactive(0, 1234, None),
            "前台 0 = 本应用（浮窗在其中）→ 不关"
        );
        assert!(
            should_close_interactive(999, 1234, None),
            "前台是非 0 的别的应用 → 关"
        );
    }
}
