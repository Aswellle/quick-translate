// src-tauri/src/system/popup_geometry.rs
// 浮窗尺寸契约的唯一来源（C3）
//
// 此前同一套尺寸散布三处且仅靠注释同步：
//   - 前端 PopupWindow.tsx 常量（400 / 520 / 44 / 160）
//   - 后端 resize_popup 的 clamp 边界（280–520 / 40–480）
//   - translation_flow.rs 的初始创建尺寸（400 / 300）
// 改宽度上限时若漏改一处，resize_popup 会静默裁剪，无任何报错。
//
// 现在后端持有权威值，前端启动时通过 get_popup_geometry 查询。
// 后端是唯一真正强制执行的一方（clamp 会裁剪），因此权威归属于此。

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约自洽性：前端会用到的每个尺寸都必须能通过 clamp 原样通过，
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
}
