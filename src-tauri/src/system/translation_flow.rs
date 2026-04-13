// src-tauri/src/system/translation_flow.rs
// 翻译主流程编排：快捷键触发 → 捕获文本 → 展示浮窗 → 翻译 → 推送结果
// Stage 2B 更新：DPI 感知浮窗定位、窗口 Acrylic 效果

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::domain::config::ConfigService;
use crate::state::AppState;
use crate::system::clipboard;
use crate::types::{
    now_unix_ms, PopupPosition, TranslationErrorPayload, TranslationLoadingPayload,
    TranslationRecord, TranslationResultPayload,
};

const POPUP_LABEL: &str = "popup";

/// 浮窗逻辑尺寸（与前端 CSS 保持一致）
const POPUP_LOGICAL_W: f64 = 400.0;
const POPUP_LOGICAL_H: f64 = 300.0;

/// 翻译主流程入口（由 hotkey 回调触发）
pub async fn execute(app: &AppHandle) {
    let (cursor_x, cursor_y) = clipboard::get_cursor_position();

    cancel_current_translation(app).await;

    let text = match clipboard::capture_selected_text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("文本捕获失败: {}", e);
            emit_error(app, e.error_code(), &e.to_string());
            return;
        }
    };

    if text.trim().is_empty() {
        emit_error(app, "EMPTY_TEXT", "未检测到选中文本");
        return;
    }

    let truncated = text.chars().count() >= 5000;
    let text_to_translate = if truncated {
        text.chars().take(5000).collect::<String>()
    } else {
        text.clone()
    };

    let state = app.state::<AppState>();
    let target_lang = state
        .config
        .read()
        .await
        .get("target_lang")
        .unwrap_or_else(|| "zh".to_string());

    // DPI 感知的浮窗定位
    let position = compute_popup_position_dpi(app, cursor_x, cursor_y);

    show_popup_loading(app, &position).await;

    let app_clone = app.clone();
    let text_clone = text_to_translate.clone();
    let target_clone = target_lang.clone();

    let task = tauri::async_runtime::spawn(async move {
        do_translate(&app_clone, &text_clone, &target_clone, truncated).await;
    });

    *state.current_translation.lock().await = Some(task);
}

async fn do_translate(app: &AppHandle, text: &str, target_lang: &str, truncated: bool) {
    let state = app.state::<AppState>();
    let start_ms = now_unix_ms();

    match state.translator.translate(text, target_lang).await {
        Ok(mut result) => {
            result.truncated = truncated;
            result.duration_ms = (now_unix_ms() - start_ms) as u64;

            let history = state.history.clone();
            let record = TranslationRecord::from_result(&result, text, target_lang);
            let limit = state.config.read().await.cache_history_limit();
            let record_clone = record.clone();

            tauri::async_runtime::spawn(async move {
                let h = history.lock().await;
                if let Err(e) = h.insert(&record_clone).await {
                    tracing::error!("历史记录写入失败: {}", e);
                }
                if let Err(e) = h.enforce_limit(limit).await {
                    tracing::error!("历史清理失败: {}", e);
                }
            });

            let _ = app.emit("translation-result", TranslationResultPayload { result });
        }
        Err(e) => {
            tracing::error!("翻译失败: {}", e);
            emit_error(app, e.error_code(), &e.to_string());
        }
    }
}

async fn cancel_current_translation(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut current = state.current_translation.lock().await;
    if let Some(handle) = current.take() {
        handle.abort();
    }
}

/// 创建或复用浮窗，发送 loading 事件
async fn show_popup_loading(app: &AppHandle, position: &PopupPosition) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        // 复用已有窗口：更新位置后显示
        let _ = window.set_position(tauri::LogicalPosition::new(position.x, position.y));
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        // 首次创建浮窗
        match WebviewWindowBuilder::new(
            app,
            POPUP_LABEL,
            WebviewUrl::App("index.html#popup".into()),
        )
        .title("QuickTranslate")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .resizable(false)
        .inner_size(POPUP_LOGICAL_W, POPUP_LOGICAL_H)
        .position(position.x, position.y)
        .build()
        {
            Ok(window) => {
                let _ = window.show();
                let _ = window.set_focus();
            }
            Err(e) => {
                tracing::error!("创建浮窗失败: {}", e);
                return;
            }
        }
    }

    let _ = app.emit(
        "translation-loading",
        TranslationLoadingPayload { position: position.clone() },
    );
}


/// DPI 感知的浮窗位置计算
///
/// cursor_x/y 是 OS 原生物理像素坐标（GetCursorPos 返回物理像素）。
/// Tauri WebviewWindowBuilder.position() 接受逻辑像素。
/// 必须除以 scale_factor 才能在高 DPI 屏幕（150%/200%）上正确定位。
fn compute_popup_position_dpi(app: &AppHandle, cursor_x: f64, cursor_y: f64) -> PopupPosition {
    const OFFSET: f64 = 12.0;

    // 找到光标所在的显示器（遍历所有 monitor，取包含光标的那个）
    let monitor = app
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos  = m.position();
                let size = m.size();
                let mx = pos.x as f64;
                let my = pos.y as f64;
                let mw = size.width  as f64;
                let mh = size.height as f64;
                cursor_x >= mx && cursor_x < mx + mw &&
                cursor_y >= my && cursor_y < my + mh
            })
        });

    let (scale, phys_w, phys_h, mon_x, mon_y) = monitor
        .as_ref()
        .map(|m| {
            let s  = m.scale_factor();
            let sz = m.size();
            let ps = m.position();
            (s, sz.width as f64, sz.height as f64, ps.x as f64, ps.y as f64)
        })
        .unwrap_or((1.0, 1920.0, 1080.0, 0.0, 0.0));

    // 转为逻辑像素（Tauri window position 单位）
    let logical_cursor_x = cursor_x / scale;
    let logical_cursor_y = cursor_y / scale;
    let logical_mon_w    = phys_w   / scale;
    let logical_mon_h    = phys_h   / scale;
    let logical_mon_x    = mon_x    / scale;
    let logical_mon_y    = mon_y    / scale;

    let mut lx = logical_cursor_x + OFFSET;
    let mut ly = logical_cursor_y + OFFSET;

    // 右边界翻转
    if lx + POPUP_LOGICAL_W > logical_mon_x + logical_mon_w {
        lx = logical_cursor_x - POPUP_LOGICAL_W - OFFSET;
    }
    // 下边界翻转
    if ly + POPUP_LOGICAL_H > logical_mon_y + logical_mon_h {
        ly = logical_cursor_y - POPUP_LOGICAL_H - OFFSET;
    }

    // 确保不超出显示器左/上边界
    lx = lx.max(logical_mon_x);
    ly = ly.max(logical_mon_y);

    PopupPosition {
        x: lx,
        y: ly,
        monitor_width:  logical_mon_w as u32,
        monitor_height: logical_mon_h as u32,
    }
}

fn emit_error(app: &AppHandle, code: &str, message: &str) {
    let _ = app.emit(
        "translation-error",
        TranslationErrorPayload {
            code: code.to_string(),
            message: message.to_string(),
        },
    );
}

trait ConfigExt {
    fn cache_history_limit(&self) -> i64;
}

impl ConfigExt for ConfigService {
    fn cache_history_limit(&self) -> i64 {
        self.get("history_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(200)
    }
}
