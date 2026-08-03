pub mod clipboard;
pub mod clipboard_monitor;
pub mod popup_geometry;
pub mod translation_flow;
pub mod tray;
pub mod updater;

/// WebView2 附加浏览器参数（所有窗口统一使用）。
///
/// 使用 `additional_browser_args` 时必须显式带上 wry 默认禁用的功能参数
/// （`msWebOOUI,msPdfOOUI,msSmartScreenProtection`），否则这些组件会被重新启用。
///
/// 体积控制（EBWebView 无上限膨胀问题的修复，见 Tauri#8145 / WebView2Feedback#4410）：
/// - `--disk-cache-size`：把磁盘缓存封顶为 8 MiB。本应用只在 webview 内加载本地
///   打包资源，HTTP 磁盘缓存基本用不到，8 MiB 绰绰有余。
/// - `--disable-gpu-shader-disk-cache`：禁用 GPU 着色器磁盘缓存，`GrShaderCache`
///   不再随每次 WebView2 运行时更新而重新累积。
pub const BROWSER_ARGS: &str = concat!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    " --disk-cache-size=8388608",
    " --disable-gpu-shader-disk-cache",
);
