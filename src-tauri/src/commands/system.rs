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
/// `DWMWA_SYSTEMBACKDROP_TYPE` 施加的 Acrylic 跟随**系统**主题，而应用的
/// 明暗由前端 `.dark` class 决定 —— 两者可以不一致（应用深色 + 系统浅色），
/// 此刻浮窗背后是一块浅色亚克力，深色玻璃观感全毁。
/// `DWMWA_USE_IMMERSIVE_DARK_MODE` 把 DWM 对该窗口的绘制（含 backdrop
/// 明暗）切到深色变体，与 CSS 着色层对齐。
///
/// 前端在 `.dark` class 变化时调用（MutationObserver 监听
/// documentElement），单一来源与 CSS 完全一致。失败只记 debug：
/// 旧系统不认识该属性属正常情况，不应刷 warn。
#[cfg(windows)]
#[tauri::command]
pub async fn set_popup_theme(app: AppHandle, dark: bool) -> Result<(), AppError> {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};

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

/// macOS 无 DWM：NSVisualEffectView 的明暗本就由窗口 appearance 驱动，
/// 无需此命令，空实现保持 IPC 契约两侧一致。
///
/// 参数写作 `_dark` 而非 `dark` 是**有意的**：tauri 宏生成参数键时会把
/// 标识符 unraw 后转 camelCase，前导下划线被丢弃 —— 键仍是 `dark`，与
/// 前端 `commands.ts` 的 `{ dark }` 对得上。改成 `dark`（无下划线）也能
/// 工作但会触发 unused 告警；改成其他名字则 IPC 键漂移，macOS 上每次
/// 调用静默报错。别动这个名字。
#[cfg(not(windows))]
#[tauri::command]
pub async fn set_popup_theme(_dark: bool) -> Result<(), AppError> {
    Ok(())
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
