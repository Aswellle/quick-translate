// src-tauri/src/system/translation_flow.rs
// 浮窗的呈现层：建窗、定位、loading / result / error 事件。
//
// Phase 3 起，请求编排（代际、取消、结果闸门、历史入队）已移入
// `system::translation::coordinator`。本模块现在只回答「显示在哪里」，
// 不再回答「这个结果还该不该显示」—— 后者的答案只有一个地方能给，
// 就是 coordinator 的代际闸门。
//
// 依赖方向单向：coordinator → translation_flow，反向没有任何引用。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::system::popup_geometry;
use crate::types::{
    PopupPosition, TranslationErrorPayload, TranslationLoadingPayload, TranslationResult,
    TranslationResultPayload,
};

pub(crate) const POPUP_LABEL: &str = "popup";

/// 浮窗初始逻辑尺寸 —— 取自 popup_geometry（尺寸契约唯一来源，C3）
const POPUP_LOGICAL_W: f64 = popup_geometry::WIDTH_NORMAL;
const POPUP_LOGICAL_H: f64 = popup_geometry::HEIGHT_INITIAL;

/// 给浮窗套上系统级材质（PRD 5.4：Windows Acrylic / macOS NSVisualEffectView）。
///
/// 为什么必须有这一步：CSS 的 `backdrop-filter` 只能模糊「同一 webview 内
/// 已经绘制的内容」。窗口本身不透明时，窗口背后是桌面/其他应用，webview
/// 采不到那些像素，`backdrop-filter` 等于作用在纯色底上 —— 视觉上就是一块
/// 不透明板，这正是「PRD 要求液态玻璃但看不到」的根因。
///
/// 只有先让窗口透明（`transparent(true)`）再让 OS 画材质，背后的桌面才会
/// 真正透过来。两者缺一不可：
///   - Windows：Acrylic 由 DWM 合成，需要 `transparent` + 非装饰窗口
///   - macOS：NSVisualEffectView 需要 `macOSPrivateApi` 才允许透明背景
///
/// 失败不致命：材质不可用时退回 CSS 的半透明背景（观感退化为磨砂纯色，
/// 功能不受影响），因此只记 warn。
fn apply_popup_material(window: &WebviewWindow) {
    use tauri::window::{Effect, EffectsBuilder};

    #[cfg(target_os = "windows")]
    let effects = EffectsBuilder::new()
        // Acrylic：Windows 10 1803+ 的实时模糊 + 噪点，最接近「液态玻璃」
        .effect(Effect::Acrylic)
        // 极淡的白色着色：让深色桌面背景下文字仍可读
        .color(tauri::utils::config::Color(255, 255, 255, 18))
        .build();

    #[cfg(target_os = "macos")]
    let effects = {
        use tauri::window::EffectState;
        EffectsBuilder::new()
            // Popover：macOS 原生弹出层材质，带正确的明暗自适应
            .effect(Effect::Popover)
            .state(EffectState::FollowsWindowActiveState)
            .radius(popup_geometry::RADIUS_LG)
            .build()
    };

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let effects = {
        let _ = Effect::Acrylic;
        EffectsBuilder::new().build()
    };

    if let Err(e) = window.set_effects(effects) {
        tracing::warn!(
            event = "popup_material_unavailable",
            "系统级浮窗材质应用失败，退回 CSS 半透明背景: {}",
            e
        );
    }
}

/// 浮窗窗口的唯一构建入口。
///
/// 三条创建路径（ensure_popup_window / show_popup_loading / 未来新增）
/// 共用同一份属性集：透明、无装饰、置顶、免激活。属性分散复制是这份
/// 代码里已经发生过两次漂移的地方（材质只在一条路径上缺失就会表现为
/// 「有时是玻璃有时不是」）。
fn build_popup_window(
    app: &AppHandle,
    position: (f64, f64),
    visible: bool,
) -> tauri::Result<WebviewWindow> {
    let window =
        WebviewWindowBuilder::new(app, POPUP_LABEL, WebviewUrl::App("index.html#popup".into()))
            .title("QuickTranslate")
            .additional_browser_args(crate::system::BROWSER_ARGS)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            // 非激活窗口（Windows 上即 WS_EX_NOACTIVATE）：显示时不抢焦点，
            // 不把用户从正在编辑的地方带走（Phase 8b / 计划第 19 节）
            .focusable(false)
            .visible(visible)
            .resizable(false)
            // 透明背景：CSS 的圆角与阴影需要它才能真正透出桌面，
            // 也是系统级材质生效的前提（见 apply_popup_material）
            .transparent(true)
            .inner_size(POPUP_LOGICAL_W, POPUP_LOGICAL_H)
            .position(position.0, position.1)
            .build()?;

    apply_popup_material(&window);
    Ok(window)
}

/// 确保 popup 浮窗已创建（onboarding 关闭后调用，此时 popup 可能从未被创建）
pub fn ensure_popup_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        // popup 已存在但可能被隐藏，重新显示。
        // 不 set_focus()：浮窗默认不抢焦点（Phase 8b）。
        tracing::info!("[ensure_popup] popup 已存在，调用 show()");
        let _ = window.show();
        return;
    }
    // 不存在，创建新的
    tracing::info!("[ensure_popup] popup 不存在，创建新窗口");
    let fallback_pos = compute_popup_position_dpi(app, 400.0, 400.0);
    // 创建失败必须留痕：这条路径跑在 onboarding 关闭之后，浮窗缺失的
    // 现场极难重现，静默吞掉 Result 会让排查无从下手（孪生路径
    // show_popup_loading 是有 error! 的，此处此前独缺）
    if let Err(e) = build_popup_window(app, (fallback_pos.x, fallback_pos.y), true) {
        tracing::error!("[ensure_popup] 创建浮窗失败: {}", e);
        app.state::<crate::state::AppState>().runtime.report_popup(
            crate::runtime::ComponentState::Degraded,
            Some("POPUP_CREATE_FAILED"),
        );
    }
}

/// 创建或复用浮窗，发送 loading 事件。
///
/// 返回 false 表示浮窗没能显示出来 —— 调用方据此上报浮窗组件的健康状态。
pub(crate) async fn show_popup_loading(
    app: &AppHandle,
    position: &PopupPosition,
    request_id: &str,
) -> bool {
    // 位置策略：用户拖拽过窗口就沿用拖拽位置，否则按光标锚点重算。
    //
    // 这是「拖到任意位置后被下次翻译冲掉」的修复点。此前每次显示都无条件
    // set_position(光标锚点)，用户摆好的位置在下次复制时立刻失效 —— 对
    // 于习惯把浮窗固定在一角的用户，这等于每次都要重新拖一次。
    // 拖拽位置是显式用户意图，优先级高于「跟着光标跑」的默认行为。
    let target = match popup_geometry::dragged_position() {
        Some((x, y)) => {
            tracing::info!("[show_popup_loading] 沿用用户拖拽位置 ({}, {})", x, y);
            (x, y)
        }
        None => (position.x, position.y),
    };

    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        tracing::info!("[show_popup_loading] 找到 popup，设置位置={:?}", target);
        let _ = window.set_position(tauri::LogicalPosition::new(target.0, target.1));
        // 每次显示都复位为非激活：用户上一次点过浮窗的话，窗口仍是可激活的，
        // 不复位就会在这一轮直接抢走焦点（Phase 8b）
        let _ = window.set_focusable(false);
        let _ = window.show();
    } else {
        tracing::info!("[show_popup_loading] popup 不存在，创建新窗口并立即 show()");
        match build_popup_window(app, target, false) {
            Ok(window) => {
                tracing::info!("[show_popup_loading] 新窗口创建成功，调用 show()");
                let _ = window.show();
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
            request_id: request_id.to_string(),
        },
    );
    // 计划第 32 节：被动显示（未抢焦点）也要可观测 —— 排查「浮窗没弹出来」
    // 时，这条事件与 popup_watch 的关闭事件是同一条时间线的两端。
    tracing::info!(
        event = "popup_show_passive",
        request_id = %request_id,
        "[popup] 浮窗以被动态显示"
    );
    true
}

/// 下发翻译结果。**调用方必须已通过 coordinator 的代际闸门** ——
/// 本函数不做任何「该不该显示」的判断。
pub(crate) fn emit_result(app: &AppHandle, result: &TranslationResult, request_id: &str) {
    let _ = app.emit(
        "translation-result",
        TranslationResultPayload {
            result: result.clone(),
            request_id: request_id.to_string(),
        },
    );
}

pub(crate) fn emit_error(app: &AppHandle, code: &str, message: &str, request_id: &str) {
    let _ = app.emit(
        "translation-error",
        TranslationErrorPayload {
            code: code.to_string(),
            message: message.to_string(),
            request_id: request_id.to_string(),
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
/// ```text
/// Loading（160px，位置合适）
///   ↓
/// Result（480px，位置没动）
///   ↓
/// 下半截跑到屏幕外
/// ```
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
