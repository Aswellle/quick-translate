// src-tauri/src/commands/system.rs
// 系统操作 command handlers（剪贴板、浮窗控制、Toast、开机自启动）

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;
use crate::system::clipboard;
use crate::types::ToastPayload;

/// 将文本写入剪贴板（前端"复制"按钮触发）
///
/// 写入前标记 clipboard_monitor：下次监控轮询检测到此内容时静默吸收，
/// 不触发二次翻译。顺序刻意为"先标记后写入"（写后再标记会开出一个
/// 监控在间隙读到新内容而误翻译的反向竞态窗口）；写入失败则撤销标记，
/// 避免残留标志吞掉用户下一次真实复制（F5）。
#[tauri::command]
pub async fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), AppError> {
    let state = app.state::<crate::state::AppState>();
    state.clipboard_monitor.mark_app_write(&text);
    let result = clipboard::write_clipboard_text(&text);
    if result.is_err() {
        state.clipboard_monitor.unmark_app_write();
    }
    result
}

/// 隐藏翻译浮窗，并重置剪贴板监控的 last_text
///
/// 与看守线程共用同一条关闭路径（`hide_popup_now`）—— 那条路径里的每一步
/// （复位 focusable、停看守、重置 last_text）漏掉任何一步都有具体后果，
/// 见其文档注释。
#[tauri::command]
pub async fn hide_popup(app: AppHandle) -> Result<(), AppError> {
    crate::system::popup_watch::hide_popup_now(&app);
    Ok(())
}

/// 用户点击浮窗后把它转为可交互（Phase 8b / 计划第 19 节）。
///
/// 浮窗默认是**非激活**窗口：显示不抢焦点，但代价是它也收不到键盘事件，
/// Esc / 空格 / Enter 全部失效。只有用户真的点了它，才把窗口设为可激活
/// 并聚焦，这批快捷键才重新可用。
///
/// 关闭不再交给前端的 `onFocusChanged`：激活链路（set_focusable 异步落
/// 标志 + startDragging 模态循环 + set_focus）交错产生的瞬时 blur 会让
/// 浮窗在激活后 ~96ms 自关（popup_activated_by_user → popup_hidden 实录）。
/// 看守切换到交互态判定继续工作 —— 浮窗自己是前台时绝不关，用户点进
/// 其他应用才关。150ms 轮询对瞬时焦点抖动天然免疫。
#[tauri::command]
pub async fn activate_popup(app: AppHandle) -> Result<(), AppError> {
    let popup_watch = app.state::<crate::state::AppState>().popup_watch.clone();
    popup_watch.enter_interactive();

    let Some(window) = app.get_webview_window("popup") else {
        return Ok(());
    };

    // 顺手记住当前位置：用户点浮窗往往伴随拖动，把「此刻窗口在哪」固化
    // 下来，下次翻译直接回到这里，而不是被光标锚点冲掉
    remember_popup_position(&window);

    window
        .set_focusable(true)
        .map_err(|e: tauri::Error| AppError::WindowError(e.to_string()))?;

    if let Err(e) = window.set_focus() {
        // fail-safe（计划第 20 节）：聚焦失败时窗口已可激活却拿不到焦点，
        // 键盘事件不可用，交互态看守虽然兜着关闭，但空格这条路断了 ——
        // 退回被动态（用户可点红叉/点外部关闭），并如实报错。
        //
        // 退回必须**完整**：focusable 也要复位。只 begin() 不复位的话，
        // 窗口仍是可激活的 —— 用户的下一次点击会在 OS 层直接激活窗口，
        // 而 enter_interactive 要等 mouseup + IPC，150ms 轮询落在这个
        // 间隙里走被动分支就会把浮窗在点击中途关掉；且窗口在此后的
        // 生命周期里变成「点击抢焦点」窗口，违反 Phase 8b 的承诺。
        let _ = window.set_focusable(false);
        tracing::warn!(
            event = "popup_activate_focus_failed",
            "set_focus 失败，退回被动态: {}",
            e
        );
        popup_watch.begin();
        return Err(AppError::WindowError(e.to_string()));
    }

    tracing::info!(
        event = "popup_activated_by_user",
        "[popup] 用户点击，进入交互态"
    );
    Ok(())
}

/// 把窗口当前位置固化进拖拽位置记忆（逻辑像素）。
///
/// 读取失败（窗口刚销毁等）只跳过 —— 位置记忆是锦上添花，不该阻断交互。
fn remember_popup_position(window: &tauri::WebviewWindow) {
    if let (Ok(pos), Ok(scale)) = (window.outer_position(), window.scale_factor()) {
        let logical_x = pos.x as f64 / scale;
        let logical_y = pos.y as f64 / scale;
        crate::system::popup_geometry::remember_dragged(logical_x, logical_y);
        tracing::info!(
            event = "popup_position_remembered",
            "[popup] 记住当前位置 ({:.0}, {:.0})",
            logical_x,
            logical_y
        );
    }
}

/// 前端在拖拽结束后主动上报窗口位置。
#[tauri::command]
pub async fn remember_popup_position_now(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("popup") {
        remember_popup_position(&window);
    }
    Ok(())
}

/// 让浮窗的系统级材质跟随应用主题明暗。
///
/// Windows：`DWMWA_SYSTEMBACKDROP_TYPE` 施加的 Acrylic 跟随**系统**主题，
/// 而应用的明暗由前端 `.dark` class 决定 —— 两者可以不一致（应用深色 +
/// 系统浅色），此刻浮窗背后是一块浅色亚克力，深色玻璃观感全毁。
/// `DWMWA_USE_IMMERSIVE_DARK_MODE` 把 DWM 对该窗口的绘制（含 backdrop
/// 明暗）切到深色变体，与 CSS 着色层对齐。
///
/// 前端在 `.dark` class 变化时调用（MutationObserver 监听
/// documentElement），单一来源与 CSS 完全一致。失败只记 debug：
/// 旧系统不认识该属性属正常情况，不应刷 warn。
///
/// macOS 侧的 appearance 同步见下方 `#[cfg(not(windows))]` 版本。
#[cfg(windows)]
#[tauri::command]
pub async fn set_popup_theme(
    app: AppHandle,
    dark: bool,
    follow_system: bool,
) -> Result<(), AppError> {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};

    // Windows 的 DWM 明暗由 dark 单独决定，follow_system 只对 macOS 有意义。
    // 显式消费掉避免 unused 告警，同时保留完整参数集 —— 前端两个平台调用
    // 同一份 commands.ts，IPC 参数键必须一致。
    let _ = follow_system;

    let Some(window) = app.get_webview_window("popup") else {
        return Ok(());
    };
    let Ok(hwnd) = window.hwnd() else {
        return Ok(());
    };

    // tauri 的 hwnd() 返回 windows crate 的 HWND（裸指针包装），
    // windows-sys 的签名要 isize —— 按位转换，句柄值本身不变
    let hwnd_val = hwnd.0 as isize;
    let dark_val: i32 = if dark { 1 } else { 0 };
    // SAFETY: 标准的 DWM 属性写入；句柄来自 Tauri 自身窗口。
    // 旧系统返回错误码属预期（属性不存在），只记 debug 不上抛。
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd_val,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark_val as *const i32 as *const core::ffi::c_void,
            core::mem::size_of::<i32>() as u32,
        )
    };
    if hr != 0 {
        tracing::debug!(
            "[set_popup_theme] DWMWA_USE_IMMERSIVE_DARK_MODE 不可用: hr={}",
            hr
        );
    }
    Ok(())
}

/// 用户开启「降低透明度」时清空浮窗的 OS 原生材质，关闭时重新施加。
///
/// 为什么光靠 CSS 不够：CSS 层把底色加厚只能压住 webview 自己画的东西，
/// 而窗口背后的 Acrylic / Liquid Glass 仍由系统合成 —— 边缘反光、亮度
/// 调制照旧。用户明确要求「减少透明效果」时，材质本身也必须撤掉，否则
/// 观感依然是半透明的。
///
/// 恢复路径复用 `translation_flow::popup_effects()` 而不是另抄一份参数：
/// 材质参数只有那一个来源，抄一份就会在两处漂移（改了建窗忘了改恢复，
/// 用户切换一次「降低透明度」就永久丢失玻璃材质）。
///
/// `enhanced = true` 表示「需要更实的外观」，即 reduce 生效。
#[tauri::command]
pub async fn set_popup_transparency(app: AppHandle, enhanced: bool) -> Result<(), AppError> {
    let Some(window) = app.get_webview_window("popup") else {
        return Ok(());
    };
    let effects = if enhanced {
        // None → tauri 走 clear_effects()，把窗口的原生材质整个撤掉
        None
    } else {
        Some(crate::system::translation_flow::popup_effects())
    };
    window
        .set_effects(effects)
        .map_err(|e| AppError::WindowError(e.to_string()))?;
    // 日志只陈述「打算做什么」，不断言「已经生效」。
    // tauri 的 set_effects 内层是 `let _ = vibrancy::set_window_effects(...)`
    // （tauri-2.12.5/src/window/mod.rs），恒返回 Ok —— 上面那句 map_err 只
    // 可能捕获主线程投递失败，材质是否真的画出来/撤掉了，这里无从得知。
    // 写成「已清空/已恢复」就是在断言一个本函数证明不了的结论（D9）。
    tracing::debug!(
        event = "popup_transparency_toggle",
        enhanced = enhanced,
        "[set_popup_transparency] enhanced={} → 投递{}原生材质的请求",
        enhanced,
        if enhanced { "撤除" } else { "恢复" }
    );
    Ok(())
}

/// macOS：把浮窗的 `NSWindow` appearance 同步到与 CSS 一致的明暗。
///
/// 为什么不能用 `tauri::Window::set_theme`：它在 macOS 上落到
/// `[NSApplication setAppearance:]`（见 tao `set_ns_theme`），是**应用级**
/// 的 —— 用它修浮窗会连带改掉设置页与历史窗口的原生外观。这里只动浮窗
/// 自己的 NSWindow（DECISIONS D3）。
///
/// `follow_system = true` 时必须 `setAppearance(nil)` 而不是设成当前明暗：
/// 给窗口钉死 appearance 会同时钉死其 webview 的 `prefers-color-scheme`，
/// 此后系统主题变化时浮窗自己的 matchMedia 永远读到旧值 —— 等于自断
/// 「跟随系统」的检测链路（DECISIONS D3 里记的那个环）。
///
/// 参数名用 `follow_system`（不带下划线）：tauri 宏把它转成 IPC 键
/// `followSystem`，与前端 `commands.ts` 的 `{ followSystem }` 对上。
/// （tauri 的 `unraw()` 只处理 `r#type` 这类 raw identifier，前缀下划线
/// 由 heck 的 `to_lower_camel_case()` 一并丢弃 —— 实测 `_follow_system`
/// 与 `follow_system` 都得到 `followSystem`，所以不必靠下划线维持键名。）
#[cfg(not(windows))]
#[tauri::command]
pub async fn set_popup_theme(
    app: AppHandle,
    dark: bool,
    follow_system: bool,
) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        let Some(window) = app.get_webview_window("popup") else {
            return Ok(());
        };
        // AppKit 的窗口操作只能在主线程做。run_on_main_thread 只是投递，
        // 不等待 —— 同步失败也无从回报，因此内部自行记录。
        //
        // 必须 clone：闭包是 move 的，直接捕获 `window` 会让
        // `window.run_on_main_thread(...)` 这个借用与闭包内的所有权
        // 转移冲突（E0505）。WebviewWindow 的 clone 只是句柄计数递增，
        // 代价可忽略。
        let window_for_closure = window.clone();
        window
            .run_on_main_thread(move || {
                apply_popup_appearance(&window_for_closure, dark, follow_system);
            })
            .map_err(|e| AppError::WindowError(e.to_string()))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, dark, follow_system);
    }
    Ok(())
}

/// 在 macOS 主线程上把浮窗 NSWindow 的 appearance 设为指定明暗。
///
/// 拆成独立函数是为了让「传什么参数」的决策可被单测覆盖
/// （`appearance_choice`），AppKit 调用本身留在条件编译边界内。
#[cfg(target_os = "macos")]
fn apply_popup_appearance(window: &tauri::WebviewWindow, dark: bool, follow_system: bool) {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSWindow,
    };

    if MainThreadMarker::new().is_none() {
        tracing::warn!("[set_popup_theme] 不在主线程，跳过 appearance 同步");
        return;
    }
    // 以下四条早退都必须留痕。D9 的全部动机就是「材质/外观决策要可观测」
    // —— set_effects 内层 `let _ =` 吞掉结果后，日志是唯一线索。这里若
    // 静默 return，排查「macOS 外观没跟上主题」时完全看不出是哪个环节
    // 断的（拿不到指针 / 指针为空 / retain 失败 / 类型不符，四者表现一样）。
    let ns_ptr = match window.ns_window() {
        Ok(ptr) => ptr,
        Err(e) => {
            tracing::warn!("[set_popup_theme] ns_window() 失败，跳过 appearance 同步: {e}");
            return;
        }
    };
    if ns_ptr.is_null() {
        tracing::warn!("[set_popup_theme] ns_window() 返回空指针，跳过 appearance 同步");
        return;
    }

    // 用 Retained::retain + downcast 而不是 `&*(ptr as *const NSWindow)`：
    // NSWindow 是 MainThreadOnly 类型，裸解引用绕过了它的线程约束，而且
    // 生命周期完全靠调用方口头保证。retain 出来的 Retained 在作用域结束
    // 时自动 release，与 tao 处理同一个 ns_window() 指针的方式一致
    // （tao platform_impl/macos/window.rs 的 downcast::<NSWindow>()）。
    //
    // SAFETY: ns_window() 返回的是本窗口的有效 NSWindow 指针；retain 在
    // 主线程上做一次 +1，函数返回时配对释放。窗口本身由 tauri 持有，
    // 不会在这次调用期间销毁。
    //
    // 关于 ns_window() 内部的 expect：tauri 的实现是
    // `view.window().expect("view to be installed in window")`
    // （tauri-2.12.5/src/window/mod.rs）。本函数只经由已加载 webview 的
    // 窗口调用，视图必然已装入窗口，该分支不可达；且 release profile 是
    // panic = "abort"，真要触发就是整个进程退出 —— 所以调用点保持在
    // 主线程回调内，不引入任何可能提前调用的路径。
    let Some(any) = (unsafe { Retained::retain(ns_ptr as *mut AnyObject) }) else {
        tracing::warn!("[set_popup_theme] NSWindow retain 失败，跳过 appearance 同步");
        return;
    };
    let Some(ns_window) = any.downcast::<NSWindow>().ok() else {
        tracing::warn!("[set_popup_theme] ns_window 不是 NSWindow，跳过 appearance 同步");
        return;
    };
    let ns_window: &NSWindow = &ns_window;

    match appearance_choice(dark, follow_system) {
        AppearanceChoice::FollowSystem => {
            // nil：窗口跟随系统外观，webview 的 prefers-color-scheme 也随之
            // 保持可被系统变化刷新
            ns_window.setAppearance(None);
            tracing::debug!("[set_popup_theme] macOS appearance 清空（跟随系统）");
        }
        AppearanceChoice::ForceDark => {
            let name = unsafe { NSAppearanceNameDarkAqua };
            match NSAppearance::appearanceNamed(name) {
                Some(appearance) => {
                    ns_window.setAppearance(Some(&appearance));
                    tracing::debug!("[set_popup_theme] macOS appearance → DarkAqua");
                }
                None => tracing::warn!(
                    "[set_popup_theme] 取不到 NSAppearanceNameDarkAqua，appearance 保持不变"
                ),
            }
        }
        AppearanceChoice::ForceLight => {
            let name = unsafe { NSAppearanceNameAqua };
            match NSAppearance::appearanceNamed(name) {
                Some(appearance) => {
                    ns_window.setAppearance(Some(&appearance));
                    tracing::debug!("[set_popup_theme] macOS appearance → Aqua");
                }
                None => {
                    tracing::warn!(
                        "[set_popup_theme] 取不到 NSAppearanceNameAqua，appearance 保持不变"
                    )
                }
            }
        }
    }
}

/// 原生 appearance 的取值决策 —— 纯函数，可在任意平台单测。
///
/// `follow_system` 优先于 `dark`：system 档位下即便此刻是深色也不能钉死，
/// 否则系统主题后续变化检测不到。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceChoice {
    /// 清空 appearance，让窗口跟随系统（theme = system）
    FollowSystem,
    ForceDark,
    ForceLight,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn appearance_choice(dark: bool, follow_system: bool) -> AppearanceChoice {
    match (follow_system, dark) {
        (true, _) => AppearanceChoice::FollowSystem,
        (false, true) => AppearanceChoice::ForceDark,
        (false, false) => AppearanceChoice::ForceLight,
    }
}

/// 清除拖拽位置记忆，回到「跟随光标」的默认行为。
#[tauri::command]
pub async fn reset_popup_position() -> Result<(), AppError> {
    crate::system::popup_geometry::forget_dragged();
    tracing::info!("[popup] 位置记忆已清除，恢复跟随光标");
    Ok(())
}

/// 动态调整翻译浮窗尺寸（根据内容自适应）
#[tauri::command]
pub async fn resize_popup(app: AppHandle, width: f64, height: f64) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("popup") {
        // 边界来自 popup_geometry —— 尺寸契约的唯一来源（C3）
        let (w, h) = crate::system::popup_geometry::clamp_size(width, height);
        window
            .set_size(tauri::LogicalSize::new(w, h))
            .map_err(|e: tauri::Error| AppError::WindowError(e.to_string()))?;

        // 记下真实尺寸：定位必须以它为准，而不是一个估算常数（Phase 8a）。
        let previous = crate::system::popup_geometry::actual_size();
        crate::system::popup_geometry::record_actual_size(w, h);

        // 尺寸变了就必须重新定位。此前只 resize 不重定位，于是
        // Loading→Result 变高时浮窗下半截会直接跑出屏幕。
        if (w - previous.0).abs() > 0.5 || (h - previous.1).abs() > 0.5 {
            crate::system::translation_flow::reposition_popup(&app, w, h);
        }
    }
    Ok(())
}

/// 下发浮窗尺寸契约，供前端替代硬编码常量（C3）
#[tauri::command]
pub fn get_popup_geometry() -> crate::system::popup_geometry::PopupGeometry {
    crate::system::popup_geometry::PopupGeometry::current()
}

/// 读取统一运行时状态（计划第 50 节）。
///
/// 界面用它回答「应用现在怎么样」，而不是从各种失败迹象自行推测。
/// 需要事件时监听 `runtime-status-changed` —— 那个事件只在状态**变化**时
/// 发出，不会按固定频率推送。
/// 本地诊断快照（计划第 33 节）：debug / support 用，不参与状态广播，不上传。
#[tauri::command]
pub async fn get_runtime_diagnostics(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<crate::runtime::lifecycle::RuntimeDiagnostics, AppError> {
    let mut d = state.runtime.diagnostics();
    d.fallback_count = state.translator.fallback_count();
    Ok(d)
}

#[tauri::command]
pub async fn get_runtime_status(
    app: AppHandle,
) -> Result<crate::runtime::RuntimeStatusSnapshot, AppError> {
    Ok(app.state::<crate::state::AppState>().runtime.snapshot())
}

/// 获取应用版本号
#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 向所有已打开窗口广播 Toast 通知（前端监听 "toast" 事件）
#[tauri::command]
pub async fn notify_toast(app: AppHandle, payload: ToastPayload) -> Result<(), AppError> {
    emit_toast(&app, payload);
    Ok(())
}

/// 内部函数：广播 Toast 事件（供 Rust 其他模块直接调用）
pub fn emit_toast(app: &AppHandle, payload: ToastPayload) {
    let _ = app.emit("toast", &payload);
}

/// 获取开机自启动状态
#[tauri::command]
pub async fn get_autostart(app: AppHandle) -> Result<bool, AppError> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| AppError::ConfigError(format!("获取自启动状态失败: {}", e)))
}

/// 设置开机自启动（Windows 注册表 / macOS LaunchAgent，由插件处理平台差异）
#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|e| AppError::ConfigError(format!("启用自启动失败: {}", e)))?;
    } else {
        // 仅在当前确实已启用时才执行 disable，
        // 避免注册表项不存在时 disable() 返回 os error 2
        let is_enabled = autolaunch.is_enabled().unwrap_or(false);
        if is_enabled {
            autolaunch
                .disable()
                .map_err(|e| AppError::ConfigError(format!("禁用自启动失败: {}", e)))?;
        }
    }

    // 同步到配置数据库
    let state = app.state::<crate::state::AppState>();
    state
        .config
        .write()
        .await
        .set("auto_start", if enabled { "true" } else { "false" })
        .await?;

    Ok(())
}

/// 检查是否需要显示首次引导向导
#[tauri::command]
pub async fn check_onboarding(app: AppHandle) -> Result<bool, AppError> {
    let state = app.state::<crate::state::AppState>();
    let completed = state
        .config
        .read()
        .await
        .get("onboarding_completed")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(!completed)
}

/// 使用系统默认浏览器打开指定 URL
/// 仅允许 http/https，使用 explorer 而非 cmd shell 避免 & | 等字符被解析为 shell 指令
/// scheme 比较不区分大小写（HTTPS:// 与 https:// 等效）
#[tauri::command]
pub fn open_url(url: String) -> Result<(), AppError> {
    let scheme = url.split("://").next().unwrap_or("").to_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::WindowError("仅支持 http/https 链接".to_string()));
    }
    std::process::Command::new("explorer")
        .arg(&url)
        .spawn()
        .map_err(|e| AppError::WindowError(format!("打开链接失败: {}", e)))?;
    Ok(())
}

/// 设置剪贴板监控开关状态
#[tauri::command]
pub async fn set_clipboard_monitor_enabled(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    let state = app.state::<crate::state::AppState>();
    tracing::info!("[set_clipboard_monitor_enabled] enabled={}", enabled);

    // 更新配置数据库
    state
        .config
        .write()
        .await
        .set(
            "clipboard_monitor_enabled",
            if enabled { "true" } else { "false" },
        )
        .await?;

    // 更新监控线程状态
    if enabled {
        state.clipboard_monitor.resume();
        tracing::info!("[set_clipboard_monitor_enabled] 已调用 resume()，监控已启用");
    } else {
        state.clipboard_monitor.suspend();
        tracing::info!("[set_clipboard_monitor_enabled] 已调用 suspend()，监控已禁用");
    }

    Ok(())
}

/// 标记引导向导已完成
#[tauri::command]
pub async fn complete_onboarding(app: AppHandle) -> Result<(), AppError> {
    tracing::info!("[complete_onboarding] 开始设置 onboarding_completed=true");
    let state = app.state::<crate::state::AppState>();
    let result = state
        .config
        .write()
        .await
        .set("onboarding_completed", "true")
        .await;
    tracing::info!("[complete_onboarding] 设置完成: {:?}", result.is_ok());
    result
}

/// 打开设置窗口（可选直达某个 Tab）。
///
/// 供浮窗错误视图的「前往设置」复用托盘同款路径：窗口已存在时发
/// `settings-navigate` 切 Tab，不存在时经 URL hash 携带初始 Tab。
/// `tab` 目前支持 "provider"（翻译源配置）与 "general"。
#[tauri::command]
pub async fn open_settings_window(app: AppHandle, tab: Option<String>) -> Result<(), AppError> {
    match tab.as_deref() {
        Some("provider") => crate::system::tray::open_provider_settings(&app),
        _ => crate::system::tray::open_settings_window(&app),
    }
    Ok(())
}

/// 打开居中的引导向导窗口（独立窗口，不复用 popup webview）
#[tauri::command]
pub async fn open_onboarding_window(app: AppHandle) -> Result<(), AppError> {
    // 若向导窗口已存在，则聚焦显示
    if let Some(window) = app.get_webview_window("onboarding") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    // 隐藏 popup 窗口（它被创建在光标位置，不应用于向导）
    if let Some(popup) = app.get_webview_window("popup") {
        tracing::info!("[open_onboarding] 隐藏现有 popup");
        let _ = popup.hide();
    } else {
        tracing::info!("[open_onboarding] popup 不存在，无需隐藏");
    }

    // 创建居中的向导窗口
    match tauri::WebviewWindowBuilder::new(
        &app,
        "onboarding",
        tauri::WebviewUrl::App("index.html#onboarding".into()),
    )
    .title("QuickTranslate 设置向导")
    .additional_browser_args(crate::system::BROWSER_ARGS)
    .inner_size(480.0, 600.0)
    .min_inner_size(400.0, 500.0)
    .resizable(true)
    .center()
    .build()
    {
        Ok(window) => {
            let _ = window.show();
            // 由安装器「运行程序」拉起时进程可能没有前台权限，显式抢一次
            let _ = window.set_focus();
            tracing::info!("引导向导窗口已打开（居中）");
            Ok(())
        }
        Err(e) => {
            tracing::error!("创建引导向导窗口失败: {}", e);
            Err(AppError::WindowError(format!("创建向导窗口失败: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{appearance_choice, AppearanceChoice};

    // 主题偏好 × 当前明暗的六种组合 —— 对应开发规格的验收矩阵。
    // 前两行是「应用显式主题与系统相反」，最容易出错的两种。
    #[test]
    fn explicit_light_forces_aqua_regardless_of_system() {
        assert_eq!(
            appearance_choice(false, false),
            AppearanceChoice::ForceLight
        );
    }

    #[test]
    fn explicit_dark_forces_dark_aqua_regardless_of_system() {
        assert_eq!(appearance_choice(true, false), AppearanceChoice::ForceDark);
    }

    // system 档位：无论此刻系统是深是浅，都必须清空 appearance。
    // 钉死会让 webview 的 prefers-color-scheme 不再响应系统变化，
    // 浮窗自己的 matchMedia 轮询从此读到死值。
    #[test]
    fn follow_system_never_pins_appearance() {
        assert_eq!(
            appearance_choice(false, true),
            AppearanceChoice::FollowSystem
        );
        assert_eq!(
            appearance_choice(true, true),
            AppearanceChoice::FollowSystem
        );
    }

    // follow_system 必须压过 dark：否则「深色偏好 → 改成跟随系统」
    // 且系统正好深色时，窗口会继续被钉死。
    #[test]
    fn follow_system_takes_precedence_over_dark() {
        for dark in [false, true] {
            assert_eq!(
                appearance_choice(dark, true),
                AppearanceChoice::FollowSystem,
                "dark={dark} 时 follow_system 仍应胜出"
            );
        }
    }
}
