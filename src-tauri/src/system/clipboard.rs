// src-tauri/src/system/clipboard.rs
// 剪贴板读写操作（供 clipboard_monitor 后台线程使用）

use crate::error::AppError;

/// 读取剪贴板文本内容
pub fn read_clipboard_text() -> Result<String, AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| AppError::ClipboardError(e.to_string()))?;
    clipboard
        .get_text()
        .map_err(|e| AppError::ClipboardError(e.to_string()))
}

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
