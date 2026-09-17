// src-tauri/src/system/clipboard/mod.rs
// 剪贴板子系统入口。
//
// 分两层：
//
//   原语层（本文件）—— 读写、文本归一化、光标位置、变更序列号。
//     与「谁在轮询、失败了怎么办」无关，任何调用方都可直接用。
//
//   监控层（子模块）—— backend / health / worker / supervisor / controller。
//     只依赖 `ClipboardBackend` trait，不直接依赖 arboard —— 这是 supervisor
//     能在没有 Windows、没有真实剪贴板的环境里被单元测试的前提（计划第 5 节）。
//
// 本文件内容自 system/clipboard.rs 与 system/clipboard_monitor.rs 平移，
// 原语实现与注释逐字未改。

pub mod backend;
pub mod backoff;
pub mod controller;
pub mod health;
pub mod supervisor;
pub mod worker;

pub use controller::MonitorController;
pub use supervisor::start_monitor;

use crate::error::AppError;

// ── 原语层 ────────────────────────────────────────────────────────────────

/// 写入文本到剪贴板
pub fn write_clipboard_text(text: &str) -> Result<(), AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| AppError::ClipboardError(e.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|e| AppError::ClipboardError(e.to_string()))
}

/// 文本规范化处理：
/// - 结构化多行文本（含段落双换行，或换行数 > 3）：
///   保留段落分隔（\n\n），仅合并 PDF 断行产生的单个换行
/// - 普通短文本：将所有换行替换为空格，合并多余空白
/// - 截断超长文本（> 5000 字符）
pub fn normalize_text(text: &str) -> String {
    // 统一 \r\n / \r 为 \n，便于后续处理
    let text = text.replace("\r\n", "\n").replace('\r', "\n");

    let newline_count = text.matches('\n').count();
    let has_paragraphs = text.contains("\n\n") || newline_count > 3;

    let normalized = if has_paragraphs {
        // 结构化文本：用占位符保护段落分隔，将单个换行（PDF 断行）合并为空格
        // 步骤：\n\n → 占位符，剩余单 \n → 空格，恢复占位符
        let placeholder = "\x00PARA\x00";
        let step1 = text.replace("\n\n", placeholder);
        let step2 = step1.replace('\n', " ");
        // 每个段落内部合并多余空白，但保留段落边界
        step2
            .split(placeholder)
            .map(|para| para.split_whitespace().collect::<Vec<&str>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        // 普通短文本：所有换行变空格，合并多余空白
        text.split_whitespace().collect::<Vec<&str>>().join(" ")
    };

    // 截断由调用方（translation_flow::execute_at_position）负责，并设置 truncated 标志
    normalized
}

/// 获取当前鼠标光标位置（屏幕坐标）
pub fn get_cursor_position() -> (f64, f64) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut point = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut point);
        }
        (point.x as f64, point.y as f64)
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: 使用 NSEvent.mouseLocation
        // 坐标系从左下角开始，需转换为从左上角
        (0.0, 0.0) // TODO: 接入 core-graphics
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        (0.0, 0.0)
    }
}

/// 剪贴板变更序列号。`Some(n)` = 平台提供权威序列号；`None` = 不可用。
///
/// 用于区分「同一段文本被用户重新复制」（序列号变化）与「文本原地未动」
/// （序列号不变）。仅靠文本内容比较无法区分这两种情况。
///
/// 返回 Option 而非哨兵 0 —— 此前非 Windows 恒返回 0 使判定退化为纯文本
/// 比较，「关闭浮窗后重新复制同一段文本仍能触发」这一承诺静默失效，与
/// `reset_last_text()` 的文档相矛盾（F8）。现在不可用时改由调用方选择
/// 显式的降级策略。
#[cfg(target_os = "windows")]
pub fn clipboard_seq() -> Option<u32> {
    // SAFETY: 无参数、无副作用的 Win32 查询调用
    Some(unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() })
}

/// 非 Windows 平台无对应原语。
#[cfg(not(target_os = "windows"))]
pub fn clipboard_seq() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_collapses_whitespace() {
        assert_eq!(normalize_text("a\r\nb"), "a b");
        assert_eq!(normalize_text("  a   b  "), "a b");
    }

    #[test]
    fn structured_text_keeps_paragraph_breaks() {
        // PDF 式断行：段内单换行合并，段间双换行保留
        let input = "line one\nline two\n\npara two";
        assert_eq!(normalize_text(input), "line one line two\n\npara two");
    }

    #[test]
    fn more_than_three_newlines_counts_as_structured() {
        let input = "a\nb\nc\nd\ne";
        // 5 个换行 > 3 → 走结构化分支
        assert_eq!(normalize_text(input), "a b c d e");
    }
}
