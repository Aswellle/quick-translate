// src-tauri/src/system/window_util.rs
// 窗口唤起助手。

/// 把一个已存在的窗口带到前台。
///
/// 只 `show()` + `set_focus()` 对**最小化**的窗口是双重空操作：wry 的
/// show 是 `SW_SHOW`（不还原最小化），tao 的 set_focus 显式跳过最小化
/// 窗口。用户最小化过设置/历史窗口后，托盘点「配置翻译源…」就会表现成
/// 「点了没反应」。先 unminimize 再 show/focus 才是完整语义。
pub(crate) fn show_and_focus(window: &tauri::WebviewWindow) {
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}
