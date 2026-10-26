// src-tauri/src/system/clipboard/mod.rs
// 剪贴板子系统入口。
//
// 分两层：
//
//   原语层（本文件）—— 读写、文本归一化、光标位置、变更序列号。
//     与「谁在轮询、失败了怎么办」无关，任何调用方都可直接用。
//
//   监控层（子模块）—— backend / health / worker / supervisor / controller。
//     只依赖 `ClipboardBackend` trait，不直接依赖 arboard —— 这是 supervisor
//     能在没有 Windows、没有真实剪贴板的环境里被单元测试的前提（计划第 5 节）。
//
// 本文件内容自 system/clipboard.rs 与 system/clipboard_monitor.rs 平移，
// 原语实现与注释逐字未改。

pub mod backend;
pub mod backoff;
pub mod controller;
pub mod health;
pub mod supervisor;
pub mod worker;

pub use controller::MonitorController;
pub use supervisor::start_monitor;

use crate::error::AppError;

// ── 原语层 ────────────────────────────────────────────────────────────────

/// 写入文本到剪贴板
pub fn write_clipboard_text(text: &str) -> Result<(), AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| AppError::ClipboardError(e.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|e| AppError::ClipboardError(e.to_string()))
}

/// 文本规范化处理（**只用于去重/防抖键**，QT-009）：
/// - 结构化多行文本（含段落双换行，或换行数 > 3）：
///   保留段落分隔（\n\n），仅合并 PDF 断行产生的单个换行
/// - 普通短文本：将所有换行替换为空格，合并多余空白
/// - 截断超长文本（> 5000 字符）
///
/// 此前这一个函数同时承担「去重键」与「翻译输入」两个角色 —— 终端日志、
/// Python 缩进、YAML 的空白结构会被不可逆折叠后送进翻译源、写进历史。
/// 角色分离后，翻译输入走 [`build_request_text`]。
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

/// 单行是否带结构信号（QT-009 的保守启发式）。
///
/// 命中任意一条即视为「非散文行」：缩进、列表/引用/提示符/表格标记、
/// 有序列表前缀、常见代码块结尾符。启发式刻意保守 —— 误判为结构化
/// 的代价只是换行保留（译文质量略降），误判为散文的代价是缩进结构
/// 被不可逆折叠（数据保真损失）。
fn looks_structured_line(line: &str) -> bool {
    // 缩进（任何前导空白）= 代码/表格/日志对齐
    if line.starts_with(|c: char| c.is_whitespace()) {
        return true;
    }
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return true;
    }
    // 列表 / 引用 / shell 提示符 / 表格 / markdown 标记
    if trimmed.starts_with(['-', '*', '•', '·', '>', '#', '$', '|', '+', '[']) {
        return true;
    }
    // 有序列表："1." "2)" "12." …
    let head: String = trimmed.chars().take(5).collect();
    if let Some(pos) = head.find(['.', ')']) {
        if pos > 0 && head[..pos].chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    // 常见代码行结尾：语句 / 块 / 赋值
    if trimmed.ends_with([';', '{', '}', '=']) {
        return true;
    }
    false
}

/// 构造送给翻译源的请求文本（QT-009）。
///
/// 三种文本角色自此分离：
///   **原始文本** —— 剪贴板原样（worker 的 last_text 保存的就是它）；
///   **去重键**   —— [`normalize_text`]（跨 CRLF 稳定的折叠比较键）；
///   **请求文本** —— 本函数：保真优先的结构感知整理，同时是历史/缓存
///   与翻译源看到的文本。
///
/// 规则（保守启发式，不做语言/代码检测）：
/// 1. CRLF/CR → LF；去首尾空白；
/// 2. 按 `\n\n+` 分段，段落分隔原样保留；
/// 3. 段内任何一行带结构信号（见 [`looks_structured_line`]）→ 整段
///    原样保留 —— Python 缩进、JSON/YAML、堆栈、表格列、日志因此保真；
/// 4. 无结构信号的段落视为 PDF 硬换行的自然段 → 单换行合并为空格
///    （延续旧 normalize 对散文的意图，但不再波及结构化内容）。
///
/// 已知限制（刻意保守的方向）：完全无缩进、无标记的多行代码会被当作
/// 散文合并换行 —— 这是启发式的边界；反过来「缩进/标记内容 100% 保真」
/// 是硬保证。单行文本不受任何影响。
pub fn build_request_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();

    if !text.contains('\n') {
        return text.to_string();
    }

    // 空行是段落边界；段落分隔（连续空行）原样保留
    let mut parts: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() {
            if !current.is_empty() {
                parts.push(join_paragraph(&current));
                current.clear();
            }
            parts.push(String::new());
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        parts.push(join_paragraph(&current));
    }

    parts.join("\n")
}

/// 段内装配：全部行都无结构信号 → 合并软换行；否则原样保留。
fn join_paragraph(lines: &[&str]) -> String {
    if lines.iter().any(|l| looks_structured_line(l)) {
        return lines.join("\n");
    }
    lines.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")
}

/// 获取当前鼠标光标位置（屏幕坐标）。
///
/// 返回值与 Windows `GetCursorPos` 同语义：**物理像素**、全局坐标。
/// 这是 `compute_popup_position_dpi` 的输入契约 —— 它会把坐标除以
/// 目标显示器的缩放比换算成逻辑像素。
///
/// 读取失败时退化为 `(0, 0)`，但**绝不静默**：发出 `cursor_position_*`
/// 稳定事件日志。退化位置由 `place_popup` 的工作区钳制兜底，至少保证
/// 浮窗完整可见。
#[cfg(target_os = "windows")]
pub fn get_cursor_position() -> (f64, f64) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: 输出指针指向本函数自己的局部变量，无其他副作用
    unsafe {
        GetCursorPos(&mut point);
    }
    (point.x as f64, point.y as f64)
}

/// macOS 光标位置。
///
/// 实现走 CoreGraphics 而不是 AppKit：
/// - `CGEventGetLocation` 直接返回 **CG 全局显示空间**坐标 —— 主屏左上为
///   原点、y 向下、单位为「点」。这与 `CGDisplayBounds` 及 tao/tauri
///   `Monitor::position()`（CG 点 × 该屏缩放比 = 物理像素）是同一坐标系，
///   不需要 Cocoa（NSEvent.mouseLocation）的左下原点翻转；
/// - CG 事件/显示查询是文档化的线程安全 API，可在剪贴板 worker 线程直接
///   调用；`NSScreen` 系列需要 `MainThreadMarker`，worker 线程拿不到；
/// - 每台显示器的缩放比用 `CGDisplayMode pixel_width / CGDisplayBounds
///   width` 推导 —— 与 `NSScreen.backingScaleFactor` 同义，但同样不依赖
///   主线程。这正是 tao 把显示器坐标从点换算成物理像素所用的比例
///   （见 tao `platform_impl/macos/monitor.rs` 的 `position()`），
///   只有同一比例才能保证光标坐标与 Tauri 显示器几何可比。
#[cfg(target_os = "macos")]
pub fn get_cursor_position() -> (f64, f64) {
    use core_graphics::display::CGDisplay;
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // 1. 光标的 CG 点坐标
    let location = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .and_then(CGEvent::new)
        .map(|event| event.location());
    let (mx, my) = match location {
        Ok(pt) => (pt.x, pt.y),
        Err(_) => {
            tracing::warn!(
                event = "cursor_position_unavailable",
                platform = "macos",
                code = "CURSOR_QUERY_FAILED",
                "[cursor] CGEvent 查询失败，浮窗定位退化为工作区原点"
            );
            return (0.0, 0.0);
        }
    };

    // 2. 光标所在显示器的缩放比。找不到时回退主屏缩放比（光标落在
    //    显示器排列的缝隙里，或枚举瞬间显示器变动）
    let displays: Vec<_> = CGDisplay::active_displays()
        .unwrap_or_default()
        .into_iter()
        .map(|id| {
            let d = CGDisplay::new(id);
            let bounds = d.bounds();
            let pixel_width = d.display_mode().map_or(0, |m| m.pixel_width());
            (
                (
                    bounds.origin.x as f64,
                    bounds.origin.y as f64,
                    bounds.size.width as f64,
                    bounds.size.height as f64,
                ),
                pixel_width,
            )
        })
        .collect();
    let fallback = macos_scale_for_display_of(&CGDisplay::main());
    let scale = macos_scale_for_cursor((mx, my), &displays, fallback);

    // 3. 点 → 物理像素：与 tao Monitor::position 的换算一致（乘所在屏缩放比）
    (mx * scale, my * scale)
}

/// 单台显示器的缩放比 = 显示模式像素宽 / CG 显示空间宽。
///
/// 两者的比值就是该屏的 backing scale（Retina 2.0、普通屏 1.0）。
/// bounds 宽为 0（枚举到已断开的显示器）时按 1.0 处理。
#[cfg(target_os = "macos")]
fn macos_scale_for_display_of(display: &core_graphics::display::CGDisplay) -> f64 {
    let bounds = display.bounds();
    let w = bounds.size.width as f64;
    let pixel_width = display.display_mode().map_or(0, |m| m.pixel_width());
    if w <= 0.0 || pixel_width == 0 {
        1.0
    } else {
        pixel_width as f64 / w
    }
}

/// 找出包含 CG 点坐标的显示器并返回其缩放比（物理像素/点）。
///
/// 纯几何函数，与平台 API 解耦 —— 单元测试在所有平台上跑
/// （审计 QT-001 验收：坐标换算必须有单测，实机只验证真实 API）。
///
/// 一台显示器的几何描述：(CG 全局显示空间矩形 (x, y, w, h)，显示模式像素宽)。
#[cfg(any(target_os = "macos", test))]
type DisplayGeometry = ((f64, f64, f64, f64), u64);

/// `displays` 的每项是 [`DisplayGeometry`]；
/// `fallback` 是没有任何显示器包含该点时的回退缩放比。
#[cfg(any(target_os = "macos", test))]
fn macos_scale_for_cursor(cursor: (f64, f64), displays: &[DisplayGeometry], fallback: f64) -> f64 {
    let (cx, cy) = cursor;
    for ((x, y, w, h), pixel_width) in displays {
        if cx >= *x && cx < x + w && cy >= *y && cy < y + h {
            let scale = if *w <= 0.0 || *pixel_width == 0 {
                1.0
            } else {
                *pixel_width as f64 / w
            };
            return scale;
        }
    }
    fallback
}

/// 其余平台无实现：剪贴板监控当前只在 Windows / macOS 启用。
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn get_cursor_position() -> (f64, f64) {
    (0.0, 0.0)
}

/// 剪贴板变更序列号。`Some(n)` = 平台提供权威序列号；`None` = 不可用。
///
/// 用于区分「同一段文本被用户重新复制」（序列号变化）与「文本原地未动」
/// （序列号不变）。仅靠文本内容比较无法区分这两种情况。
///
/// 返回 Option 而非哨兵 0 —— 此前非 Windows 恒返回 0 使判定退化为纯文本
/// 比较，「关闭浮窗后重新复制同一段文本仍能触发」这一承诺静默失效，与
/// `reset_last_text()` 的文档相矛盾（F8）。现在不可用时改由调用方选择
/// 显式的降级策略。
#[cfg(target_os = "windows")]
pub fn clipboard_seq() -> Option<u32> {
    // SAFETY: 无参数、无副作用的 Win32 查询调用
    Some(unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() })
}

/// macOS：NSPasteboard 的 changeCount 就是与 Win32 序列号同语义的
/// 剪贴板变化代数（QT-002）—— 每次写剪贴板（任何进程）单调递增。
///
/// 有了它，worker 的「同文本 + 序列号变化 = 用户重新复制」判定在 macOS
/// 上与 Windows 完全同构，不再需要任何平台特例。
#[cfg(target_os = "macos")]
pub fn clipboard_seq() -> Option<u32> {
    use objc2_app_kit::NSPasteboard;
    let count = NSPasteboard::generalPasteboard().changeCount();
    // changeCount 是单调递增的 NSInteger（isize），负值不可能出现；
    // u32 溢出需要 40 多亿次剪贴板写入，饱和处理即可
    Some(u32::try_from(count.max(0)).unwrap_or(u32::MAX))
}

/// 其余平台无对应原语。
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn clipboard_seq() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_collapses_whitespace() {
        assert_eq!(normalize_text("a\r\nb"), "a b");
        assert_eq!(normalize_text("  a   b  "), "a b");
    }

    #[test]
    fn structured_text_keeps_paragraph_breaks() {
        // PDF 式断行：段内单换行合并，段间双换行保留
        let input = "line one\nline two\n\npara two";
        assert_eq!(normalize_text(input), "line one line two\n\npara two");
    }

    #[test]
    fn more_than_three_newlines_counts_as_structured() {
        let input = "a\nb\nc\nd\ne";
        // 5 个换行 > 3 → 走结构化分支
        assert_eq!(normalize_text(input), "a b c d e");
    }

    // ── QT-001：macOS 光标坐标换算（纯函数部分，全平台可测）────────────

    /// 布景：主屏 Retina（CG 空间 0,0 起 1512×982 点，像素宽 3024 → scale 2.0），
    /// 副屏普通 DPI 在主屏右侧（CG x 1512 起 1920×1080 点，像素宽 1920 → scale 1.0）。
    /// 与 tao Monitor::position 的「CG 点 × 该屏 scale = 物理」模型一致。
    fn two_display_layout() -> Vec<((f64, f64, f64, f64), u64)> {
        vec![
            ((0.0, 0.0, 1512.0, 982.0), 3024),     // 主屏 Retina
            ((1512.0, 0.0, 1920.0, 1080.0), 1920), // 副屏 1x
        ]
    }

    #[test]
    fn scale_is_taken_from_the_display_containing_the_cursor() {
        let displays = two_display_layout();
        let s_main = macos_scale_for_cursor((756.0, 491.0), &displays, 1.0);
        assert!((s_main - 2.0).abs() < 1e-9, "主屏内光标应取 Retina 缩放比");
        let s_side = macos_scale_for_cursor((2000.0, 100.0), &displays, 1.0);
        assert!((s_side - 1.0).abs() < 1e-9, "副屏内光标应取副屏缩放比");
    }

    /// 光标落在主屏四个象限与边缘附近：缩放比必须始终来自主屏，
    /// 边界判定与 `place_popup` 一样是左闭右开（右边缘属于下一台屏）。
    #[test]
    fn quadrant_and_edge_positions_resolve_to_the_right_display() {
        let displays = two_display_layout();
        // 四象限（主屏内）
        for (x, y) in [(10.0, 10.0), (1500.0, 10.0), (10.0, 970.0), (1500.0, 970.0)] {
            let s = macos_scale_for_cursor((x, y), &displays, 1.0);
            assert!((s - 2.0).abs() < 1e-9, "({x},{y}) 应判定为主屏");
        }
        // 右边缘：x=1512 已属于副屏（左闭右开）
        let s = macos_scale_for_cursor((1512.0, 5.0), &displays, 1.0);
        assert!((s - 1.0).abs() < 1e-9, "主屏右边缘之外属于副屏");
        // 主屏最右一点（x=1511.99）仍属主屏
        let s = macos_scale_for_cursor((1511.9, 5.0), &displays, 1.0);
        assert!((s - 2.0).abs() < 1e-9);
    }

    /// 负坐标显示器（副屏在主屏左侧 / 上方是常见摆放）：
    /// CG 空间里它们的 x / y 为负，包含判定与缩放比必须照常工作。
    #[test]
    fn negative_coordinate_displays_are_matched() {
        let displays = vec![
            ((0.0, 0.0, 1512.0, 982.0), 3024),      // 主屏
            ((-1920.0, 0.0, 1920.0, 1080.0), 3840), // 左侧副屏 2x
            ((0.0, -982.0, 1512.0, 982.0), 3024),   // 上方副屏 Retina
        ];
        let s_left = macos_scale_for_cursor((-100.0, 500.0), &displays, 1.0);
        assert!((s_left - 2.0).abs() < 1e-9, "左侧屏内光标取该屏缩放比");
        let s_top = macos_scale_for_cursor((700.0, -50.0), &displays, 1.0);
        assert!((s_top - 2.0).abs() < 1e-9, "上方屏内光标取该屏缩放比");
    }

    /// 光标在所有显示器之外的缝隙：回退到调用方给的主屏缩放比。
    #[test]
    fn cursor_outside_all_displays_falls_back() {
        let displays = two_display_layout();
        // 两台屏之间的垂直缝隙（y = 1100 超出两台屏的高度）
        let s = macos_scale_for_cursor((700.0, 1100.0), &displays, 2.0);
        assert!((s - 2.0).abs() < 1e-9, "缝隙内光标使用回退缩放比");
    }

    /// 缩放比推导：像素宽 / CG 宽。除零（枚举到断开的显示器）按 1.0。
    #[test]
    fn scale_derivation_guards_zero_width() {
        let displays = vec![
            ((0.0, 0.0, 0.0, 0.0), 3024),  // 异常显示器：宽 0
            ((0.0, 0.0, 100.0, 100.0), 0), // 异常显示器：像素宽 0
        ];
        // 都无法推导 → 回退
        let s = macos_scale_for_cursor((50.0, 50.0), &displays, 1.0);
        assert!((s - 1.0).abs() < 1e-9, "退化显示器必须走回退而不是除零/NaN");
    }

    /// 物理换算契约：物理 = CG 点 × 所在屏缩放比（与 tao Monitor::position
    /// 相同的换算）。这是 compute_popup_position_dpi 除以 scale 的逆运算，
    /// 两侧必须互逆，否则高 DPI 屏上浮窗锚点会偏移 scale 倍。
    #[test]
    fn point_times_scale_is_the_physical_contract() {
        // 主屏中心：CG 点 (756, 491) × 2.0 = 物理 (1512, 982)
        let displays = two_display_layout();
        let s = macos_scale_for_cursor((756.0, 491.0), &displays, 1.0);
        assert_eq!((756.0 * s, 491.0 * s), (1512.0, 982.0));
    }
}

#[cfg(test)]
mod request_text_tests {
    use super::*;

    /// 普通散文：单换行按 PDF 软换行合并（与旧 normalize 的意图一致）
    #[test]
    fn prose_soft_wraps_are_joined() {
        assert_eq!(build_request_text("hello\nworld"), "hello world");
        assert_eq!(build_request_text("hello\r\nworld"), "hello world");
        assert_eq!(build_request_text("  trimmed  "), "trimmed");
    }

    /// Python 缩进：任何行带前导空白 → 整段原样保留
    #[test]
    fn indented_code_is_preserved_verbatim() {
        let code = "def f():\n    if x:\n        return 1";
        assert_eq!(build_request_text(code), code);
    }

    /// 终端堆栈：\t 缩进行 → 原样保留（旧实现会折叠成一行空格分隔）
    #[test]
    fn stack_trace_is_preserved_verbatim() {
        let trace = "Exception in thread main\n\tat com.foo.Bar.baz(Bar.java:1)\n\tat com.foo.Qux.run(Qux.java:2)";
        assert_eq!(build_request_text(trace), trace);
    }

    /// YAML / 列表 / 表格 / 引用 / 提示符：结构标记 → 原样保留
    #[test]
    fn structured_markers_preserve_line_breaks() {
        let yaml = "services:\n  web:\n    image: nginx";
        assert_eq!(build_request_text(yaml), yaml);

        let list = "- name: a\n- name: b";
        assert_eq!(build_request_text(list), list);

        let numbered = "1. first\n2. second";
        assert_eq!(build_request_text(numbered), numbered);

        let table = "| a | b |\n| c | d |";
        assert_eq!(build_request_text(table), table);

        let shell = "$ npm run build\n$ echo done";
        assert_eq!(build_request_text(shell), shell);
    }

    /// 代码块结尾符（; { } =）所在段落原样保留
    #[test]
    fn code_terminators_preserve_line_breaks() {
        // 无缩进、无标记的行以块结尾符收尾时同样判为结构化
        let json = "{\"a\":1}";
        assert_eq!(build_request_text(json), json);

        let block = "if (x) {\ndo();\n}";
        assert_eq!(build_request_text(block), block);
    }

    /// 混合段落：散文段合并软换行，结构段原样保留，段间分隔不丢
    #[test]
    fn mixed_paragraphs_get_per_paragraph_treatment() {
        let input = "first prose line\nsecond prose line\n\ndef f():\n    return 1";
        let expected = "first prose line second prose line\n\ndef f():\n    return 1";
        assert_eq!(build_request_text(input), expected);
    }

    /// 连续空行（\n\n\n）作为段落分隔原样保留
    #[test]
    fn paragraph_breaks_are_preserved() {
        let input = "para one\n\n\npara two";
        assert_eq!(build_request_text(input), "para one\n\n\npara two");
    }

    /// 与去重键的角色分离：同一段代码，去重键折叠、请求文本保真 ——
    /// 两个值不再互相冒用。
    #[test]
    fn request_text_differs_from_dedupe_key_for_code() {
        let code = "def f():\n    return 1";
        assert_eq!(build_request_text(code), code, "请求文本保真");
        assert_eq!(
            normalize_text(code),
            "def f(): return 1",
            "去重键仍是折叠键（比较用途）"
        );
    }
}
