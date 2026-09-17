// src-tauri/src/system/translation_flow.rs
// 浮窗的呈现层：建窗、定位、loading / result / error 事件。
//
// Phase 3 起，请求编排（代际、取消、结果闸门、历史入队）已移入
// `system::translation::coordinator`。本模块现在只回答「显示在哪里」，
// 不再回答「这个结果还该不该显示」—— 后者的答案只有一个地方能给，
// 就是 coordinator 的代际闸门。
//
// 依赖方向单向：coordinator → translation_flow，反向没有任何引用。
// Phase 8 会重构本模块的窗口激活策略与几何计算。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::system::popup_geometry;
use crate::types::{
    PopupPosition, TranslationErrorPayload, TranslationLoadingPayload, TranslationResult,
    TranslationResultPayload,
};

const POPUP_LABEL: &str = "popup";

/// 浮窗初始逻辑尺寸 —— 取自 popup_geometry（尺寸契约唯一来源，C3）
const POPUP_LOGICAL_W: f64 = popup_geometry::WIDTH_NORMAL;
const POPUP_LOGICAL_H: f64 = popup_geometry::HEIGHT_INITIAL;

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
    let _ = WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App("index.html#popup".into()))
        .title("QuickTranslate")
        .additional_browser_args(crate::system::BROWSER_ARGS)
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

/// 创建或复用浮窗，发送 loading 事件。
///
/// 返回 false 表示浮窗没能显示出来 —— 调用方据此上报浮窗组件的健康状态。
pub(crate) async fn show_popup_loading(app: &AppHandle, position: &PopupPosition) -> bool {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        tracing::info!(
            "[show_popup_loading] 找到 popup，设置位置={:?}，调用 show()+focus()",
            position
        );
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
        .additional_browser_args(crate::system::BROWSER_ARGS)
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
                return false;
            }
        }
    }

    let _ = app.emit(
        "translation-loading",
        TranslationLoadingPayload {
            position: position.clone(),
        },
    );
    true
}

/// 下发翻译结果。**调用方必须已通过 coordinator 的代际闸门** ——
/// 本函数不做任何「该不该显示」的判断。
pub(crate) fn emit_result(app: &AppHandle, result: &TranslationResult) {
    let _ = app.emit(
        "translation-result",
        TranslationResultPayload {
            result: result.clone(),
        },
    );
}

pub(crate) fn emit_error(app: &AppHandle, code: &str, message: &str) {
    let _ = app.emit(
        "translation-error",
        TranslationErrorPayload {
            code: code.to_string(),
            message: message.to_string(),
        },
    );
}

/// DPI 感知的浮窗位置计算（公开，供 coordinator 调用）
///
/// cursor_x/y 是 OS 原生物理像素坐标（GetCursorPos 返回物理像素）。
/// Tauri WebviewWindowBuilder.position() 接受逻辑像素。
/// 必须除以 scale_factor 才能在高 DPI 屏幕（150%/200%）上正确定位。
///
/// 定位策略：计算光标四周可用空间，选择面积最大的方向放置浮窗，
/// 避免遮挡光标附近的目标文本。
///
/// 边界保证全部交给 `popup_geometry::place_popup` —— 那是个纯函数，
/// 多显示器负坐标、任务栏在四边、浮窗比工作区还大都在那里被穷举测试过。
/// 本函数只做三件事：找显示器、把物理坐标换成逻辑坐标、取出工作区。
pub(crate) fn compute_popup_position_dpi(
    app: &AppHandle,
    cursor_x: f64,
    cursor_y: f64,
) -> PopupPosition {
    // 记住锚点：resize 之后要按同一个锚点重新定位，否则结果变高时
    // 窗口只会往下长、下半截跑出屏幕
    popup_geometry::remember_anchor((cursor_x, cursor_y));

    let (scale, monitor_logical, work_area) = desktop_for(app, (cursor_x, cursor_y));
    let popup = popup_geometry::actual_size();

    let (x, y) =
        popup_geometry::place_popup((cursor_x / scale, cursor_y / scale), work_area, popup);

    tracing::info!(
        "[compute_popup_position] cursor=({:.0},{:.0}) work=({:.0},{:.0},{:.0}x{:.0}) popup={:.0}x{:.0} → ({:.0},{:.0})",
        cursor_x / scale,
        cursor_y / scale,
        work_area.x,
        work_area.y,
        work_area.width,
        work_area.height,
        popup.0,
        popup.1,
        x,
        y
    );

    PopupPosition {
        x,
        y,
        monitor_width: monitor_logical.0 as u32,
        monitor_height: monitor_logical.1 as u32,
    }
}

/// 找出光标所在显示器，返回 (scale_factor, 显示器逻辑尺寸, 工作区逻辑矩形)。
///
/// 用 `work_area()` 而不是 `size()`：工作区已经扣掉任务栏等保留区域。
/// 拿整块屏幕定位，浮窗就会压在任务栏底下 —— 这正是此前那个
/// `mon_max_y - 60.0` 想兜住、但只对「底部任务栏 + 100% DPI」成立的场景。
fn desktop_for(app: &AppHandle, cursor: (f64, f64)) -> (f64, (f64, f64), popup_geometry::Rect) {
    let (cursor_x, cursor_y) = cursor;

    let monitor = app.available_monitors().ok().and_then(|monitors| {
        monitors.into_iter().find(|m| {
            let pos = m.position();
            let size = m.size();
            let (mx, my) = (pos.x as f64, pos.y as f64);
            let (mw, mh) = (size.width as f64, size.height as f64);
            cursor_x >= mx && cursor_x < mx + mw && cursor_y >= my && cursor_y < my + mh
        })
    });

    match monitor.as_ref() {
        Some(m) => {
            let s = m.scale_factor();
            let size = m.size();
            let wa = m.work_area();
            (
                s,
                (size.width as f64 / s, size.height as f64 / s),
                popup_geometry::Rect::new(
                    wa.position.x as f64 / s,
                    wa.position.y as f64 / s,
                    wa.size.width as f64 / s,
                    wa.size.height as f64 / s,
                ),
            )
        }
        // 取不到显示器信息（理论上不会发生）：退回一个保守的主屏工作区，
        // 而不是让整条翻译流程失败
        None => (
            1.0,
            (1920.0, 1080.0),
            popup_geometry::Rect::new(0.0, 0.0, 1920.0, 1080.0),
        ),
    }
}

/// 尺寸变化后按原锚点重新定位。
///
/// 前端量高 → resize_popup → 窗口变高，此时**必须重新定位**：
///
///     Loading（160px，位置合适）
///       ↓
///     Result（480px，位置没动）
///       ↓
///     下半截跑到屏幕外
///
/// 返回 false 表示没有可用锚点（浮窗还没因翻译显示过），此时不动作。
pub(crate) fn reposition_popup(app: &AppHandle, width: f64, height: f64) -> bool {
    let Some(cursor) = popup_geometry::anchor() else {
        return false;
    };
    let Some(window) = app.get_webview_window(POPUP_LABEL) else {
        return false;
    };

    let (scale, _, work_area) = desktop_for(app, cursor);
    let (x, y) = popup_geometry::place_popup(
        (cursor.0 / scale, cursor.1 / scale),
        work_area,
        (width, height),
    );

    if let Err(e) = window.set_position(tauri::LogicalPosition::new(x, y)) {
        tracing::warn!("[reposition_popup] 重新定位失败: {}", e);
        return false;
    }

    tracing::info!(
        event = "popup_repositioned",
        "[reposition_popup] 尺寸 {:.0}x{:.0} → 位置 ({:.0},{:.0})",
        width,
        height,
        x,
        y
    );
    true
}
