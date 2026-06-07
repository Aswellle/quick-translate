// src-tauri/src/system/translation_flow.rs
// 翻译主流程编排：
// - clipboard_monitor 触发 → 调用 do_translate() → 推送结果事件
// - popup 窗口复用时也可调用 execute_text()

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;
use crate::types::{
    now_unix_ms, PopupPosition, TranslationErrorPayload, TranslationLoadingPayload,
    TranslationRecord, TranslationResult, TranslationResultPayload,
};

const POPUP_LABEL: &str = "popup";

/// 浮窗逻辑尺寸（与前端 CSS 保持一致）
const POPUP_LOGICAL_W: f64 = 400.0;
const POPUP_LOGICAL_H: f64 = 300.0;

/// 在指定光标位置执行翻译（clipboard_monitor 直接调用，传入已捕获的文本）
pub async fn execute_at_position(app: &AppHandle, cursor_x: f64, cursor_y: f64, text: String) {
    cancel_current_translation(app).await;

    if text.trim().is_empty() {
        emit_error(app, "EMPTY_TEXT", "未检测到选中文本");
        return;
    }

    let truncated = text.chars().count() > 5000;
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
        // 源语言与目标语言相同：显示原文而非报错，与 clipboard_monitor 路径保持一致
        Err(crate::error::AppError::SameLanguage { lang }) => {
            tracing::info!("[do_translate] 源语言与目标语言相同（{}），显示原文", lang);
            let original_result = TranslationResult {
                source_text: text.to_string(),
                translated_text: text.to_string(),
                detected_source_lang: lang,
                target_lang: target_lang.to_string(),
                provider: "none".to_string(),
                duration_ms: 0,
                truncated,
            };
            let _ = app.emit("translation-result", TranslationResultPayload { result: original_result });
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

/// 确保 popup 浮窗已创建（onboarding 关闭后调用，此时 popup 可能从未被创建）
pub fn ensure_popup_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        // popup 已存在但可能被隐藏，重新显示
        tracing::info!("[ensure_popup] popup 已存在，调用 show() + set_focus()");
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    // 不存在，创建新的
    tracing::info!("[ensure_popup] popup 不存在，创建新窗口");
    let fallback_pos = compute_popup_position_dpi(app, 400.0, 400.0);
    let _ = WebviewWindowBuilder::new(
        app,
        POPUP_LABEL,
        WebviewUrl::App("index.html#popup".into()),
    )
    .title("QuickTranslate")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(true)
    .resizable(false)
    .inner_size(POPUP_LOGICAL_W, POPUP_LOGICAL_H)
    .position(fallback_pos.x, fallback_pos.y)
    .build();
}

/// 创建或复用浮窗，发送 loading 事件
pub async fn show_popup_loading(app: &AppHandle, position: &PopupPosition) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        tracing::info!("[show_popup_loading] 找到 popup，设置位置={:?}，调用 show()+focus()", position);
        let _ = window.set_position(tauri::LogicalPosition::new(position.x, position.y));
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        tracing::info!("[show_popup_loading] popup 不存在，创建新窗口并立即 show()");
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
                tracing::info!("[show_popup_loading] 新窗口创建成功，调用 show()+focus()");
                let _ = window.show();
                let _ = window.set_focus();
            }
            Err(e) => {
                tracing::error!("[show_popup_loading] 创建浮窗失败: {}", e);
                return;
            }
        }
    }

    let _ = app.emit(
        "translation-loading",
        TranslationLoadingPayload {
            position: position.clone(),
        },
    );
}

/// DPI 感知的浮窗位置计算（公开，供 clipboard_monitor 调用）
///
/// cursor_x/y 是 OS 原生物理像素坐标（GetCursorPos 返回物理像素）。
/// Tauri WebviewWindowBuilder.position() 接受逻辑像素。
/// 必须除以 scale_factor 才能在高 DPI 屏幕（150%/200%）上正确定位。
///
/// 定位策略：计算光标四周可用空间，选择面积最大的方向放置浮窗，
/// 避免遮挡光标附近的目标文本。
pub fn compute_popup_position_dpi(app: &AppHandle, cursor_x: f64, cursor_y: f64) -> PopupPosition {
    const OFFSET: f64 = 14.0;
    const POPUP_H: f64 = 300.0; // 估算高度（实际由前端动态调整）

    let monitor = app.available_monitors().ok().and_then(|monitors| {
        monitors.into_iter().find(|m| {
            let pos = m.position();
            let size = m.size();
            let mx = pos.x as f64;
            let my = pos.y as f64;
            let mw = size.width as f64;
            let mh = size.height as f64;
            cursor_x >= mx && cursor_x < mx + mw && cursor_y >= my && cursor_y < my + mh
        })
    });

    let (scale, phys_w, phys_h, mon_x, mon_y) = monitor
        .as_ref()
        .map(|m| {
            let s = m.scale_factor();
            let sz = m.size();
            let ps = m.position();
            (
                s,
                sz.width as f64,
                sz.height as f64,
                ps.x as f64,
                ps.y as f64,
            )
        })
        .unwrap_or((1.0, 1920.0, 1080.0, 0.0, 0.0));

    let cx = cursor_x / scale;  // 光标逻辑 x
    let cy = cursor_y / scale; // 光标逻辑 y
    let mon_w = phys_w / scale;
    let mon_h = phys_h / scale;
    let mon_min_x = mon_x / scale;
    let mon_min_y = mon_y / scale;
    let mon_max_x = mon_min_x + mon_w;
    let mon_max_y = mon_min_y + mon_h;

    // 计算四个方向的可用空间
    let space_right  = mon_max_x - (cx + OFFSET);              // 右侧可用宽度
    let space_left   = cx - OFFSET - mon_min_x;                  // 左侧可用宽度
    let space_below  = mon_max_y - (cy + OFFSET);                 // 下方可用高度
    let space_above  = cy - OFFSET - mon_min_y;                  // 上方可用高度

    // 判断各方向是否能容纳浮窗
    let can_right  = space_right  >= POPUP_LOGICAL_W;
    let can_left   = space_left   >= POPUP_LOGICAL_W;
    let can_below  = space_below  >= POPUP_H;
    let can_above  = space_above  >= POPUP_H;

    // 优先方向：右下 → 右上方 → 左下方 → 左上方 → 下方 → 上方 → 右方 → 左方
    let (lx, ly) = if can_right && can_below {
        (cx + OFFSET, cy + OFFSET)                                    // 右下（最佳）
    } else if can_right && can_above {
        (cx + OFFSET, cy - OFFSET - POPUP_H)                          // 右上方
    } else if can_left && can_below {
        (cx - OFFSET - POPUP_LOGICAL_W, cy + OFFSET)                  // 左下方
    } else if can_left && can_above {
        (cx - OFFSET - POPUP_LOGICAL_W, cy - OFFSET - POPUP_H)         // 左上方
    } else if can_below {
        // 下方空间足够，水平居中于光标
        let lx_center = (cx - POPUP_LOGICAL_W / 2.0).clamp(mon_min_x, mon_max_x - POPUP_LOGICAL_W);
        (lx_center, cy + OFFSET)
    } else if can_above {
        let lx_center = (cx - POPUP_LOGICAL_W / 2.0).clamp(mon_min_x, mon_max_x - POPUP_LOGICAL_W);
        (lx_center, cy - OFFSET - POPUP_H)
    } else if can_right {
        // 右侧空间足够，垂直居中于光标
        let ly_center = (cy - POPUP_H / 2.0).clamp(mon_min_y, mon_max_y - POPUP_H);
        (cx + OFFSET, ly_center)
    } else if can_left {
        let ly_center = (cy - POPUP_H / 2.0).clamp(mon_min_y, mon_max_y - POPUP_H);
        (cx - OFFSET - POPUP_LOGICAL_W, ly_center)
    } else {
        // 所有方向都不够，强制放在左上角
        (mon_min_x + 10.0, mon_min_y + 10.0)
    };

    // 最终边界钳制（确保完全在屏幕内）
    let final_x = lx.clamp(mon_min_x, (mon_max_x - POPUP_LOGICAL_W).max(mon_min_x));
    let final_y = ly.clamp(mon_min_y, (mon_max_y - 60.0).max(mon_min_y));

    tracing::info!(
        "[compute_popup_position] cursor=({:.0},{:.0}) mon=({:.0},{:.0},{:.0}x{:.0}) space_right={:.0} below={:.0} → pos=({:.0},{:.0})",
        cx, cy, mon_min_x, mon_min_y, mon_w, mon_h, space_right, space_below, final_x, final_y
    );

    PopupPosition {
        x: final_x,
        y: final_y,
        monitor_width: mon_w as u32,
        monitor_height: mon_h as u32,
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

pub trait ConfigExt {
    fn cache_history_limit(&self) -> i64;
}

impl ConfigExt for crate::domain::config::ConfigService {
    fn cache_history_limit(&self) -> i64 {
        self.get("history_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(200)
    }
}
