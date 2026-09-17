// src-tauri/src/system/popup_geometry.rs
// 浮窗尺寸与位置的唯一来源（C3）
//
// 此前同一套尺寸散布三处且仅靠注释同步：
//   - 前端 PopupWindow.tsx 常量（400 / 520 / 44 / 160）
//   - 后端 resize_popup 的 clamp 边界（280–520 / 40–480）
//   - translation_flow.rs 的初始创建尺寸（400 / 300）
// 改宽度上限时若漏改一处，resize_popup 会静默裁剪，无任何报错。
//
// 现在后端持有权威值，前端启动时通过 get_popup_geometry 查询。
// 后端是唯一真正强制执行的一方（clamp 会裁剪），因此权威归属于此。
//
// Phase 8a 起，**定位**也归到这里：位置计算的边界保证此前靠两个魔数
// （估算高度 300、底部留白 60），与真实窗口尺寸和工作区都无关。

use std::sync::Mutex;

use serde::Serialize;

/// 标准宽度
pub const WIDTH_NORMAL: f64 = 400.0;
/// 阅读宽度（绿灯切换）
pub const WIDTH_WIDE: f64 = 520.0;
/// 折叠态高度（仅红绿灯标题栏）
pub const HEIGHT_COLLAPSED: f64 = 44.0;
/// 加载态骨架屏高度
pub const HEIGHT_LOADING: f64 = 160.0;
/// 新建窗口时的初始高度（内容到达后由前端量算覆盖）
pub const HEIGHT_INITIAL: f64 = 300.0;

/// clamp 边界：下限刻意略低于 HEIGHT_COLLAPSED，留出边框/缩放余量
pub const MIN_WIDTH: f64 = 280.0;
pub const MAX_WIDTH: f64 = WIDTH_WIDE;
pub const MIN_HEIGHT: f64 = 40.0;
pub const MAX_HEIGHT: f64 = 480.0;

/// 浮窗与光标之间的距离
const OFFSET: f64 = 14.0;
/// 所有方向都放不下时的内缩量
const INSET: f64 = 10.0;

/// 下发给前端的尺寸契约
#[derive(Serialize, Clone, Copy)]
pub struct PopupGeometry {
    pub width_normal: f64,
    pub width_wide: f64,
    pub height_collapsed: f64,
    pub height_loading: f64,
    pub min_width: f64,
    pub max_width: f64,
    pub min_height: f64,
    pub max_height: f64,
}

impl PopupGeometry {
    pub const fn current() -> Self {
        Self {
            width_normal: WIDTH_NORMAL,
            width_wide: WIDTH_WIDE,
            height_collapsed: HEIGHT_COLLAPSED,
            height_loading: HEIGHT_LOADING,
            min_width: MIN_WIDTH,
            max_width: MAX_WIDTH,
            min_height: MIN_HEIGHT,
            max_height: MAX_HEIGHT,
        }
    }
}

/// 将任意请求尺寸钳制到合法范围
pub fn clamp_size(width: f64, height: f64) -> (f64, f64) {
    (
        width.clamp(MIN_WIDTH, MAX_WIDTH),
        height.clamp(MIN_HEIGHT, MAX_HEIGHT),
    )
}

// ── 定位 ────────────────────────────────────────────────────────────────

/// 一个矩形（逻辑像素）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// 把 `(x, y)` 钉进工作区的横轴范围。
///
/// 浮窗比工作区还宽时上界会小于下界 —— 直接 `clamp` 会 panic。
/// 用 `.max(area.x)` 把它钉在工作区左缘，比崩溃合理。
fn clamp_x(x: f64, width: f64, area: Rect) -> f64 {
    x.clamp(area.x, (area.right() - width).max(area.x))
}

fn clamp_y(y: f64, height: f64, area: Rect) -> f64 {
    y.clamp(area.y, (area.bottom() - height).max(area.y))
}

/// 光标位置 + 工作区 + **真实**浮窗尺寸 → 最终位置（逻辑像素）。
///
/// 纯函数（计划第 54 节要求 `clamp_popup_position` 纯函数化）：不碰 Tauri、
/// 不碰 Win32，因此每条边界都能单独断言 —— 多显示器负坐标、任务栏在四边、
/// 浮窗比工作区还大。
///
/// `work_area` 必须是**工作区**而不是整块屏幕。任务栏占用的区域不该用来
/// 放浮窗，否则浮窗会有一截压在任务栏底下。
///
/// `popup` 必须是真实宽高而不是估算值：前端会按内容量高再 resize，
/// 用固定常数参与定位决策，结果一变高下半截就会跑出屏幕。
pub fn place_popup(cursor: (f64, f64), work_area: Rect, popup: (f64, f64)) -> (f64, f64) {
    let (cx, cy) = cursor;
    let (w, h) = popup;

    let can_right = work_area.right() - (cx + OFFSET) >= w;
    let can_left = cx - OFFSET - work_area.x >= w;
    let can_below = work_area.bottom() - (cy + OFFSET) >= h;
    let can_above = cy - OFFSET - work_area.y >= h;

    // 优先方向：右下 → 右上 → 左下 → 左上 → 下方 → 上方 → 右方 → 左方
    let (lx, ly) = if can_right && can_below {
        (cx + OFFSET, cy + OFFSET)
    } else if can_right && can_above {
        (cx + OFFSET, cy - OFFSET - h)
    } else if can_left && can_below {
        (cx - OFFSET - w, cy + OFFSET)
    } else if can_left && can_above {
        (cx - OFFSET - w, cy - OFFSET - h)
    } else if can_below {
        (clamp_x(cx - w / 2.0, w, work_area), cy + OFFSET)
    } else if can_above {
        (clamp_x(cx - w / 2.0, w, work_area), cy - OFFSET - h)
    } else if can_right {
        (cx + OFFSET, clamp_y(cy - h / 2.0, h, work_area))
    } else if can_left {
        (cx - OFFSET - w, clamp_y(cy - h / 2.0, h, work_area))
    } else {
        // 所有方向都放不下 —— 钉在工作区左上角
        (work_area.x + INSET, work_area.y + INSET)
    };

    (clamp_x(lx, w, work_area), clamp_y(ly, h, work_area))
}

// ── 当前浮窗的实际几何 ──────────────────────────────────────────────────
//
// 定位必须以**真实**尺寸与**真实**锚点为准，所以这两项要记下来。
// 放在这里而不是 AppState：它们就是「当前浮窗几何」本身，
// 与尺寸契约同属一个关注点。

/// 最近一次实际生效的浮窗尺寸（逻辑像素，已过 clamp）
static ACTUAL_SIZE: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// 本次浮窗的定位锚点（屏幕物理像素，与 GetCursorPos 同坐标系）
static ANCHOR: Mutex<Option<(f64, f64)>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 记录 resize_popup 实际应用下去的尺寸。
pub fn record_actual_size(width: f64, height: f64) {
    *lock(&ACTUAL_SIZE) = Some((width, height));
}

/// 当前浮窗尺寸。还没 resize 过就返回新建窗口时的初始尺寸。
pub fn actual_size() -> (f64, f64) {
    lock(&ACTUAL_SIZE).unwrap_or((WIDTH_NORMAL, HEIGHT_INITIAL))
}

/// 记录本次浮窗的定位锚点（触发翻译时的光标位置）。
pub fn remember_anchor(cursor: (f64, f64)) {
    *lock(&ANCHOR) = Some(cursor);
}

/// 本次浮窗的锚点。浮动窗从未显示过则为 None。
pub fn anchor() -> Option<(f64, f64)> {
    *lock(&ANCHOR)
}

/// 测试用：清掉跨用例残留的全局状态。
///
/// 这两个 static 是进程级的，测试并行跑时互相污染 —— 每个涉及它们的用例
/// 开头都要清一次。
#[cfg(test)]
pub fn reset_state_for_test() {
    *lock(&ACTUAL_SIZE) = None;
    *lock(&ANCHOR) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一块 1920×1080、无任务栏遮挡的工作区
    fn hd() -> Rect {
        Rect::new(0.0, 0.0, 1920.0, 1080.0)
    }

    fn inside(pos: (f64, f64), area: Rect, popup: (f64, f64)) -> bool {
        pos.0 >= area.x - 0.001
            && pos.1 >= area.y - 0.001
            && pos.0 + popup.0 <= area.right() + 0.001
            && pos.1 + popup.1 <= area.bottom() + 0.001
    }

    // ── 契约自洽性（原有）────────────────────────────────────────────────

    /// 前端会用到的每个尺寸都必须能通过 clamp 原样通过，
    /// 否则窗口会被静默裁剪成与前端布局不符的尺寸（C3 要防的正是这个）
    #[test]
    fn frontend_sizes_survive_clamp() {
        for w in [WIDTH_NORMAL, WIDTH_WIDE] {
            for h in [HEIGHT_COLLAPSED, HEIGHT_LOADING, HEIGHT_INITIAL] {
                assert_eq!(
                    clamp_size(w, h),
                    (w, h),
                    "尺寸 {}x{} 被 clamp 改写，前后端契约不一致",
                    w,
                    h
                );
            }
        }
    }

    #[test]
    fn clamp_bounds_are_ordered() {
        assert!(MIN_WIDTH < MAX_WIDTH);
        assert!(MIN_HEIGHT < MAX_HEIGHT);
        assert!(MIN_HEIGHT <= HEIGHT_COLLAPSED);
    }

    #[test]
    fn clamp_rejects_out_of_range() {
        assert_eq!(clamp_size(100.0, 10.0), (MIN_WIDTH, MIN_HEIGHT));
        assert_eq!(clamp_size(9999.0, 9999.0), (MAX_WIDTH, MAX_HEIGHT));
    }

    // ── 定位：优选方向 ───────────────────────────────────────────────────

    #[test]
    fn room_below_right_places_it_below_right() {
        let pos = place_popup((500.0, 500.0), hd(), (400.0, 300.0));
        assert_eq!(pos, (500.0 + OFFSET, 500.0 + OFFSET));
    }

    /// 右下都放不下 → 翻到左上
    #[test]
    fn bottom_right_corner_flips_to_above_left() {
        let a = hd();
        let popup = (400.0, 300.0);
        let pos = place_popup((a.right() - 5.0, a.bottom() - 5.0), a, popup);
        assert!(inside(pos, a, popup), "{:?} 跑出工作区", pos);
    }

    #[test]
    fn near_right_edge_flips_left() {
        let a = hd();
        let popup = (400.0, 200.0);
        let pos = place_popup((a.right() - 50.0, 400.0), a, popup);
        assert!(
            pos.0 + popup.0 <= a.right() + 0.001,
            "右侧放不下时应翻到左侧，实际 x={}",
            pos.0
        );
    }

    #[test]
    fn near_bottom_edge_flips_above() {
        let a = hd();
        let popup = (400.0, 300.0);
        let pos = place_popup((500.0, a.bottom() - 50.0), a, popup);
        assert!(
            pos.1 + popup.1 <= a.bottom() + 0.001,
            "下方放不下时应翻到上方，实际 y={}",
            pos.1
        );
    }

    // ── 定位：工作区而非整块屏幕 ─────────────────────────────────────────

    /// 任务栏在下方：工作区底边低于屏幕底边，浮窗绝不能压到任务栏上
    #[test]
    fn never_overlaps_a_bottom_taskbar() {
        // 1920×1080 屏幕，底部 48px 任务栏 → 工作区高 1032
        let a = Rect::new(0.0, 0.0, 1920.0, 1032.0);
        let popup = (400.0, 480.0); // 浮窗顶到最大高度

        for cursor_y in [900.0, 1000.0, 1030.0] {
            let pos = place_popup((800.0, cursor_y), a, popup);
            assert!(
                pos.1 + popup.1 <= a.bottom() + 0.001,
                "cursor_y={} 时浮窗压到了任务栏：y={} h={}",
                cursor_y,
                pos.1,
                popup.1
            );
        }
    }

    /// 任务栏在左侧：工作区左缘 x=80，浮窗不能跑到负 x 去
    #[test]
    fn never_overlaps_a_left_taskbar() {
        let a = Rect::new(80.0, 0.0, 1840.0, 1080.0);
        let popup = (400.0, 300.0);
        let pos = place_popup((100.0, 500.0), a, popup);
        assert!(pos.0 >= a.x, "浮窗压到了左侧任务栏：x={}", pos.0);
        assert!(inside(pos, a, popup));
    }

    /// 任务栏在上方：工作区顶边 y=40
    #[test]
    fn never_overlaps_a_top_taskbar() {
        let a = Rect::new(0.0, 40.0, 1920.0, 1040.0);
        let popup = (400.0, 300.0);
        let pos = place_popup((500.0, 45.0), a, popup);
        assert!(pos.1 >= a.y, "浮窗压到了上方任务栏：y={}", pos.1);
        assert!(inside(pos, a, popup));
    }

    // ── 定位：多显示器负坐标 ─────────────────────────────────────────────

    /// 左侧副屏：工作区整体位于负坐标。此前的实现假设 `0 <= x < screen_width`，
    /// 在这种布局下会把浮窗强制拉回主屏。
    #[test]
    fn works_on_a_negative_coordinate_monitor() {
        let a = Rect::new(-1920.0, 0.0, 1920.0, 1080.0);
        let popup = (400.0, 300.0);

        let pos = place_popup((-1000.0, 500.0), a, popup);

        assert!(inside(pos, a, popup), "{:?} 跑出了负坐标工作区", pos);
        assert!(
            pos.0 < 0.0,
            "浮窗应留在副屏上，实际 x={}（被拉回主屏了）",
            pos.0
        );
    }

    /// 副屏在主屏右侧
    #[test]
    fn works_on_a_monitor_to_the_right() {
        let a = Rect::new(1920.0, 0.0, 2560.0, 1440.0);
        let popup = (400.0, 300.0);
        let pos = place_popup((2400.0, 700.0), a, popup);
        assert!(inside(pos, a, popup));
        assert!(pos.0 >= 1920.0);
    }

    // ── 定位：退化情形 ───────────────────────────────────────────────────

    /// 浮窗比工作区还大：必须钉在左上角，绝不能 panic 也不能跑到外面
    #[test]
    fn popup_larger_than_the_work_area_is_pinned_not_panicking() {
        let tiny = Rect::new(0.0, 0.0, 200.0, 100.0);
        let popup = (400.0, 300.0);

        let pos = place_popup((100.0, 50.0), tiny, popup);

        assert_eq!(pos, (tiny.x, tiny.y), "放不下时应钉在工作区左上角");
        // 注意：此时浮窗必然超出工作区 —— 那是工作区太小的客观事实，
        // 能保证的是位置不越界、且不 panic
        assert!(pos.0 >= tiny.x && pos.1 >= tiny.y);
    }

    /// 极端情形：光标在工作区外的负坐标（多屏交界的瞬间）
    #[test]
    fn cursor_outside_the_work_area_still_yields_a_valid_position() {
        let a = hd();
        let popup = (400.0, 300.0);
        for cursor in [(-500.0, -500.0), (5000.0, 5000.0), (-1.0, 500.0)] {
            let pos = place_popup(cursor, a, popup);
            assert!(
                inside(pos, a, popup),
                "cursor={:?} → {:?} 越界",
                cursor,
                pos
            );
        }
    }

    /// 穷举一批光标位置，任何情况下都不能越界
    #[test]
    fn position_is_always_inside_the_work_area() {
        let areas = [
            hd(),
            Rect::new(-1920.0, 0.0, 1920.0, 1080.0),
            Rect::new(0.0, 0.0, 1920.0, 1032.0),
            Rect::new(80.0, 40.0, 1840.0, 1000.0),
        ];
        let popups = [
            (WIDTH_NORMAL, HEIGHT_COLLAPSED),
            (WIDTH_NORMAL, HEIGHT_INITIAL),
            (WIDTH_NORMAL, MAX_HEIGHT),
            (WIDTH_WIDE, MAX_HEIGHT),
        ];

        for area in areas {
            for popup in popups {
                for cx in [
                    -2000.0,
                    area.x,
                    area.x + 1.0,
                    area.right() / 2.0,
                    area.right(),
                    3000.0,
                ] {
                    for cy in [
                        area.y,
                        area.y + 1.0,
                        area.bottom() / 2.0,
                        area.bottom(),
                        3000.0,
                    ] {
                        let pos = place_popup((cx, cy), area, popup);
                        assert!(
                            inside(pos, area, popup),
                            "area={:?} popup={:?} cursor=({},{}) → {:?} 越界",
                            area,
                            popup,
                            cx,
                            cy,
                            pos
                        );
                    }
                }
            }
        }
    }

    // ── 实际尺寸记账 ─────────────────────────────────────────────────────

    #[test]
    fn actual_size_defaults_to_the_initial_window_size() {
        reset_state_for_test();
        assert_eq!(actual_size(), (WIDTH_NORMAL, HEIGHT_INITIAL));
    }

    #[test]
    fn recorded_size_is_returned() {
        reset_state_for_test();
        record_actual_size(400.0, 236.0);
        assert_eq!(actual_size(), (400.0, 236.0));
        reset_state_for_test();
    }

    #[test]
    fn anchor_is_none_until_recorded() {
        reset_state_for_test();
        assert_eq!(anchor(), None);
        remember_anchor((123.0, 456.0));
        assert_eq!(anchor(), Some((123.0, 456.0)));
        reset_state_for_test();
    }
}
