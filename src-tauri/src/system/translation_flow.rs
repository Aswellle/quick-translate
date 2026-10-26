// src-tauri/src/system/translation_flow.rs
// 浮窗的呈现层：建窗、定位、loading / result / error 事件。
//
// Phase 3 起，请求编排（代际、取消、结果闸门、历史入队）已移入
// `system::translation::coordinator`。本模块现在只回答「显示在哪里」，
// 不再回答「这个结果还该不该显示」—— 后者的答案只有一个地方能给，
// 就是 coordinator 的代际闸门。
//
// 依赖方向单向：coordinator → translation_flow，反向没有任何引用。

use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

// 平台专有的材质参数，按 cfg 引入以免在另一平台产生未使用告警
#[cfg(target_os = "windows")]
use tauri::window::Color;
#[cfg(target_os = "macos")]
use tauri::window::EffectState;

use crate::system::popup_geometry;
use crate::types::{
    PopupPosition, TranslationErrorPayload, TranslationLoadingPayload, TranslationResult,
    TranslationResultPayload,
};

pub(crate) const POPUP_LABEL: &str = "popup";

/// 浮窗初始逻辑尺寸 —— 取自 popup_geometry（尺寸契约唯一来源，C3）
const POPUP_LOGICAL_W: f64 = popup_geometry::WIDTH_NORMAL;
const POPUP_LOGICAL_H: f64 = popup_geometry::HEIGHT_INITIAL;

/// 浮窗的系统级材质声明。
///
/// 为什么必须有这一步：CSS 的 `backdrop-filter` 只能模糊「同一 webview 内
/// 已经绘制的内容」。窗口本身不透明时，窗口背后是桌面/其他应用，webview
/// 采不到那些像素，`backdrop-filter` 等于作用在纯色底上 —— 视觉上就是一块
/// 不透明板，这正是「PRD 要求液态玻璃但看不到」的根因。
///
/// 只有先让窗口透明（`transparent(true)`）再让 OS 画材质，背后的桌面才会
/// 真正透过来。两者缺一不可：
///   - Windows：Acrylic 由 DWM 合成，需要 `transparent` + 非装饰窗口
///   - macOS：NSVisualEffectView / NSGlassEffectView 需要 `macOSPrivateApi`
///
/// 声明 ≠ 生效：这里只构造配置。真正的施加由 builder 投递到主线程队列，
/// 而 tauri 内层用 `let _ =` 丢弃了施加结果（`vibrancy::set_window_effects`
/// 恒返回 `Ok`），所以**任何返回值都不能证明材质画出来了**。能确认的只有
/// `log_material_decision` 记下的「本机打算用哪一档」。
///
/// `pub(crate)` 供 `commands::system::set_popup_transparency` 在用户关掉
/// 「降低透明度」后重新施加同一份材质 —— 材质参数只有这一个来源，恢复
/// 路径不能另抄一份（抄了就会在两处漂移）。
pub(crate) fn popup_effects() -> WindowEffectsConfig {
    #[cfg(target_os = "windows")]
    {
        EffectsBuilder::new()
            // Acrylic：Windows 10 1803+ 的实时模糊 + 噪点，最接近「液态玻璃」
            .effect(Effect::Acrylic)
            // 极淡的白色着色。注意 Win11 22H2+ 走 DWMWA_SYSTEMBACKDROP_TYPE，
            // 这个颜色会被系统静默忽略 —— 明暗改由 set_popup_theme 的
            // DWMWA_USE_IMMERSIVE_DARK_MODE 控制
            .color(Color(255, 255, 255, 18))
            .build()
    }

    #[cfg(target_os = "macos")]
    {
        EffectsBuilder::new()
            // macOS 26+ 的原生 Liquid Glass
            .effect(Effect::LiquidGlassRegular)
            // 回退档：tauri 在 Liquid Glass 返回 UnsupportedPlatformVersion
            // （macOS 15 及以下）时会自动改用这一项。两项必须同时声明，
            // 否则旧系统上窗口完全没有材质
            .effect(Effect::Popover)
            // 浮窗是 focusable(false)，永远不会成为 key window。保持
            // FollowsWindowActiveState 会稳定渲染 inactive（更淡）外观，
            // 与当前「玻璃偏淡」的取向一致。要改浓才换成 Active
            .state(EffectState::FollowsWindowActiveState)
            .radius(popup_geometry::RADIUS_LG)
            .build()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        EffectsBuilder::new().build()
    }
}

/// 记录本机打算用哪一档材质。
///
/// 存在的理由：`set_effects` 的失败日志在 tauri 里是死代码（内层 `let _ =`），
/// 材质没画出来时日志一片空白。这条决策日志至少能区分「声明了 Liquid Glass」
/// 与「声明了 Popover 回退」，排查时不必靠猜。
fn log_material_decision() {
    #[cfg(target_os = "windows")]
    tracing::info!(
        event = "popup_material_decision",
        platform = "windows",
        effect = "acrylic",
        "浮窗材质：Windows Acrylic（Win11 22H2+ 由 DWM 系统背板承担，着色参数被忽略）"
    );

    #[cfg(target_os = "macos")]
    tracing::info!(
        event = "popup_material_decision",
        platform = "macos",
        effect = "liquid_glass_regular|popover",
        "浮窗材质：macOS 26+ 用 Liquid Glass Regular，15 及以下回退 Popover"
    );

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    tracing::info!(
        event = "popup_material_decision",
        platform = "other",
        effect = "none",
        "浮窗材质：当前平台无原生材质，退回 CSS 半透明背景"
    );
}

/// 浮窗窗口的唯一构建入口。
///
/// 三条创建路径（ensure_popup_window / show_popup_loading / 未来新增）
/// 共用同一份属性集：透明、无装饰、置顶、免激活。属性分散复制是这份
/// 代码里已经发生过两次漂移的地方（材质只在一条路径上缺失就会表现为
/// 「有时是玻璃有时不是」）。
///
/// `visible` 参数保留原语义，但**建窗一律先隐藏**，材质声明走 builder 的
/// `effects()`，最后才按需 `show()`。
///
/// 为什么要这样绕一圈：`WindowEffectsConfig` 在 builder 内部是投递到主线程
/// 队列执行的（`build_internal` 里的 `run_on_main_thread`），而 `.visible(true)`
/// 会让窗口在 `build()` 返回前就可见。两者叠加的结果是「窗口先亮出来、材质
/// 后画上去」，首帧是一块没有玻璃的透明板。先隐藏再按序 show，队列顺序保证
/// 材质先于可见。
fn build_popup_window(
    app: &AppHandle,
    position: (f64, f64),
    visible: bool,
) -> tauri::Result<WebviewWindow> {
    log_material_decision();

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
            // 始终隐藏建窗：见上方注释，可见性由末尾的 show() 决定
            .visible(false)
            .resizable(false)
            // 透明背景：CSS 的圆角与阴影需要它才能真正透出桌面，
            // 也是系统级材质生效的前提（见 popup_effects）
            .transparent(true)
            // 材质声明进 builder，由 build_internal 投递到主线程队列，
            // 排在下面那次 show() 之前
            .effects(popup_effects())
            .inner_size(POPUP_LOGICAL_W, POPUP_LOGICAL_H)
            .position(position.0, position.1)
            .build()?;

    if visible {
        let _ = window.show();
    }

    Ok(window)
}

/// 确保 popup 浮窗已创建（onboarding 关闭后调用，此时 popup 可能从未被创建）
pub fn ensure_popup_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        // popup 已存在但可能被隐藏，重新显示。
        // 不 set_focus()：浮窗默认不抢焦点（Phase 8b）。
        tracing::info!("[ensure_popup] popup 已存在，调用 show()");
        if !submit_show(|| window.show()) {
            app.state::<crate::state::AppState>().runtime.report_popup(
                crate::runtime::ComponentState::Degraded,
                Some("POPUP_SHOW_FAILED"),
            );
        }
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

/// 创建或复用浮窗并显示。**只做窗口操作，不发事件。**
///
/// 事件提交已上移到 `coordinator::commit_popup_show` 的代际闸门内
/// （QT-003）：loading 事件携带 request id，会改变前端的当前请求身份，
/// 因此「发不发」必须由代际闸门原子裁决；本函数只回答「窗口是否显示成功」。
///
/// 失败语义（QT-007）：`show()` 是核心可见性操作 —— 失败必须如实上报
/// （返回 false，运行时层据此置 Degraded），绝不能吞掉后假装显示成功。
/// `set_position` / `set_focusable` 是非关键优化：失败只降级为「位置沿用
/// 上一次 / 焦点态未复位」，浮窗仍然可见，记录 warn 即可，不值得为它们
/// 放弃整次显示。
///
/// 注意 `show()` 返回 Ok 的含义是「显示请求已提交到主线程队列」，而非
/// 「此刻已可见」—— tauri 的窗口操作从非主线程调用时是排队投递的。
/// 立即反查 `is_visible()` 会得到竞态假阴性，因此成功路径以「已提交」
/// 为准，由 `popup_show_passive` 事件与关闭看守在时间线上观测真实可见性。
///
/// 返回 false 表示浮窗没能显示出来 —— 调用方据此上报浮窗组件的健康状态。
pub(crate) async fn show_popup_window(app: &AppHandle, position: &PopupPosition) -> bool {
    // 位置策略：用户拖拽过窗口就沿用拖拽位置，否则按光标锚点重算。
    //
    // 这是「拖到任意位置后被下次翻译冲掉」的修复点。此前每次显示都无条件
    // set_position(光标锚点)，用户摆好的位置在下次复制时立刻失效 —— 对
    // 于习惯把浮窗固定在一角的用户，这等于每次都要重新拖一次。
    // 拖拽位置是显式用户意图，优先级高于「跟着光标跑」的默认行为。
    let target = match popup_geometry::dragged_position() {
        Some((x, y)) => {
            tracing::info!("[show_popup_window] 沿用用户拖拽位置 ({}, {})", x, y);
            (x, y)
        }
        None => (position.x, position.y),
    };

    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        tracing::info!("[show_popup_window] 找到 popup，设置位置={:?}", target);
        if let Err(e) = window.set_position(tauri::LogicalPosition::new(target.0, target.1)) {
            // 位置失败不阻断显示：浮窗沿用上一次的位置，仍然可见
            tracing::warn!(
                event = "popup_reposition_degraded",
                code = "POPUP_SET_POSITION_FAILED",
                "[show_popup_window] 定位失败（沿用旧位置）: {}",
                e
            );
        }
        // 每次显示都复位为非激活：用户上一次点过浮窗的话，窗口仍是可激活的，
        // 不复位就会在这一轮直接抢走焦点（Phase 8b）
        if let Err(e) = window.set_focusable(false) {
            tracing::warn!(
                event = "popup_reposition_degraded",
                code = "POPUP_SET_FOCUSABLE_FAILED",
                "[show_popup_window] 焦点态复位失败: {}",
                e
            );
        }
        return submit_show(|| window.show());
    }

    tracing::info!("[show_popup_window] popup 不存在，创建新窗口并立即 show()");
    match build_popup_window(app, target, false) {
        Ok(window) => {
            tracing::info!("[show_popup_window] 新窗口创建成功，调用 show()");
            submit_show(|| window.show())
        }
        Err(e) => {
            tracing::error!(
                event = "popup_show_failed",
                code = "POPUP_CREATE_FAILED",
                "[show_popup_window] 创建浮窗失败: {}",
                e
            );
            false
        }
    }
}

/// 提交 show 请求并如实回报结果（QT-007）。
///
/// 失败必须带稳定错误码落地日志 —— 此前的 `let _ = window.show()` 会让
/// 「用户看不到浮窗」与「运行时报告 healthy」同时成立，排查无从下手。
fn submit_show(show: impl FnOnce() -> Result<(), tauri::Error>) -> bool {
    match show() {
        Ok(()) => true,
        Err(e) => {
            tracing::error!(
                event = "popup_show_failed",
                code = "POPUP_SHOW_FAILED",
                "[show_popup_window] show() 提交失败: {}",
                e
            );
            false
        }
    }
}

/// 发送 loading 事件。**必须经 coordinator 的代际闸门调用** ——
/// 本函数不做任何「该不该显示」的判断。
pub(crate) fn emit_loading(app: &AppHandle, position: &PopupPosition, request_id: &str) {
    let _ = app.emit(
        "translation-loading",
        TranslationLoadingPayload {
            position: position.clone(),
            request_id: request_id.to_string(),
        },
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows 只声明 Acrylic 一项，并带着极淡的白色着色。
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_declares_acrylic_with_faint_white_tint() {
        let config = popup_effects();
        assert_eq!(config.effects, vec![Effect::Acrylic]);
        assert_eq!(config.color, Some(Color(255, 255, 255, 18)));
    }

    /// macOS 必须同时声明 Liquid Glass 与回退材质。
    ///
    /// 这是本模块最容易静默回归的契约：只留 LiquidGlassRegular 时，
    /// macOS 26 上一切正常，macOS 15 及以下则完全没有材质 —— 而回退
    /// 逻辑在 tauri 内部，Windows 上的任何测试都看不见这个缺陷。
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_declares_liquid_glass_with_popover_fallback() {
        let config = popup_effects();
        assert_eq!(
            config.effects,
            vec![Effect::LiquidGlassRegular, Effect::Popover]
        );
        assert_eq!(
            config.state,
            Some(EffectState::FollowsWindowActiveState),
            "浮窗不可聚焦，保持跟随活跃态才会渲染偏淡的 inactive 外观"
        );
        assert_eq!(config.radius, Some(popup_geometry::RADIUS_LG));
    }

    /// 材质声明为空时不应该被当成「有材质」传给系统。
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn other_platforms_declare_no_effects() {
        assert!(popup_effects().effects.is_empty());
    }
}
