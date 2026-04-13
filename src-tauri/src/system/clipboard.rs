// src-tauri/src/system/clipboard.rs
// 剪贴板操作：备份 → 模拟 Ctrl+C → 读取 → 恢复
// 使用 arboard (跨平台剪贴板) + enigo (按键模拟)

use crate::error::AppError;

/// 完整的"选中文本捕获"流程
/// 1. 备份当前剪贴板
/// 2. 清空剪贴板（便于检测 Ctrl+C 是否成功）
/// 3. 模拟 Ctrl+C
/// 4. 等待 50ms（等待剪贴板更新）
/// 5. 读取剪贴板
/// 6. 恢复剪贴板
/// 7. 返回文本或错误
pub async fn capture_selected_text() -> Result<String, AppError> {
    // 1. 备份当前剪贴板内容
    let backup = read_clipboard_text().ok();

    // 2. 清空剪贴板（用于检测 Ctrl+C 是否生效）
    let _ = write_clipboard_text("");

    // 3. 模拟 Ctrl+C
    simulate_copy()?;

    // 4. 等待 50ms（经验值：低于此值模拟 Ctrl+C 可能还未写入剪贴板）
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 5. 读取剪贴板
    let content = read_clipboard_text();

    // 6. 恢复剪贴板（best effort，失败不影响主流程）
    if let Some(backup_text) = &backup {
        let _ = write_clipboard_text(backup_text);
    }

    // 7. 判断结果
    match content {
        Ok(text) if !text.trim().is_empty() => Ok(normalize_text(&text)),
        Ok(_) => {
            // 剪贴板为空，可能 Ctrl+C 未生效，再等 250ms 重试一次
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let retry = read_clipboard_text();

            // 恢复剪贴板
            if let Some(backup_text) = &backup {
                let _ = write_clipboard_text(backup_text);
            }

            match retry {
                Ok(text) if !text.trim().is_empty() => Ok(normalize_text(&text)),
                _ => Err(AppError::EmptyText),
            }
        }
        Err(e) => {
            tracing::warn!("读取剪贴板失败: {}", e);
            Err(AppError::EmptyText)
        }
    }
}

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

/// 模拟 Ctrl+C 按键（跨平台）
fn simulate_copy() -> Result<(), AppError> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| AppError::ClipboardError(format!("enigo 初始化失败: {}", e)))?;

    // 按下 Control（Windows/Linux）或 Meta（macOS 的 Cmd 通过 enigo 的平台抽象处理）
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| AppError::ClipboardError(format!("按键模拟失败: {}", e)))?;

    enigo
        .key(Key::C, Direction::Click)
        .map_err(|e| AppError::ClipboardError(format!("按键模拟失败: {}", e)))?;

    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| AppError::ClipboardError(format!("按键模拟失败: {}", e)))?;

    Ok(())
}

/// 文本规范化处理：
/// - 去除 PDF 选中文本的硬换行（\r\n、\n → 空格）
/// - 合并多余空白
/// - 截断超长文本（> 5000 字符）
pub fn normalize_text(text: &str) -> String {
    let cleaned = text
        .replace("\r\n", " ")
        .replace('\r', " ")
        .replace('\n', " ");

    let normalized: String = cleaned
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    // 截断超过 5000 字符的文本（在调用方标记 truncated=true）
    if normalized.chars().count() > 5000 {
        normalized.chars().take(5000).collect()
    } else {
        normalized
    }
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
