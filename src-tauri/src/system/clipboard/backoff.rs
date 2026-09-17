// src-tauri/src/system/clipboard/backoff.rs
// 退避策略的纯函数部分。
//
// 计划第 54 节要求把 next_backoff / should_retry 一类逻辑做成纯函数：
// 纯函数 → 不用启动 Tauri → 不用 Windows → 测试稳定。
// 随机抖动依赖注入的 Rng，因此连抖动路径也是确定可测的。

use std::time::Duration;

use rand::Rng;

/// supervisor 重启退避表（计划第 5 节）：
/// 250ms → 1s → 3s → 10s → 30s，此后封顶在 30s。
///
/// 刻意用显式表而非 `initial * 2^n`：计划给的就是这条曲线
/// （250→1000 是 ×4，1000→3000 是 ×3），写成指数式反而与规格不符。
pub const RESTART_BACKOFF_SCHEDULE: [Duration; 5] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// 第 `attempt` 次重启前的基准退避（attempt 从 0 起），超出表长则取表尾。
/// 纯函数 —— 无需 fake clock 即可验证完整退避曲线。
pub fn restart_backoff(attempt: u32, schedule: &[Duration]) -> Duration {
    debug_assert!(!schedule.is_empty(), "退避表不能为空");
    let idx = (attempt as usize).min(schedule.len() - 1);
    schedule[idx]
}

/// worker 内部暂时性失败的退避：从 `initial` 起按 2 的幂增长，封顶 `max`。
///
/// 与 supervisor 的重启退避分开：这里 worker 还活着、句柄还是好的，
/// 只是这一轮读失败，因此尺度要短得多（毫秒级起步），
/// 否则用户复制一次文本要等半分钟才看到翻译。
pub fn transient_backoff(consecutive_failures: u32, initial: Duration, max: Duration) -> Duration {
    // 失败次数从 1 起，第 1 次失败应当用 initial 而非 2×initial
    let exp = consecutive_failures.saturating_sub(1);
    exponential_backoff(exp, initial, max)
}

/// 指数退避：`initial * 2^exp`，上限 `max`。
///
/// 用整数毫秒做位移而非浮点乘法，避免 Duration 的舍入让
/// 「第 n 次恰好等于上限」这类边界断言变脆。
pub fn exponential_backoff(exp: u32, initial: Duration, max: Duration) -> Duration {
    let initial_ms = initial.as_millis() as u64;
    let max_ms = max.as_millis() as u64;
    // 先夹住指数：`1u64 << 64` 会溢出 panic
    let shift = exp.min(63);
    let scaled = initial_ms.saturating_mul(1u64 << shift);
    Duration::from_millis(scaled.min(max_ms))
}

/// 施加 ±20% 抖动。
///
/// 目的不是「看起来更随机」，而是避免多个后台组件在同一个故障事件后
/// 同步重试形成尖峰（计划第 5 节）。
pub fn with_jitter(base: Duration, rng: &mut impl Rng) -> Duration {
    let factor: f64 = rng.gen_range(0.8_f64..=1.2_f64);
    let ms = (base.as_millis() as f64 * factor).round().max(1.0) as u64;
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    /// 计划第 5 节给出的曲线必须逐项对上
    #[test]
    fn restart_schedule_matches_plan() {
        let s = &RESTART_BACKOFF_SCHEDULE;
        assert_eq!(restart_backoff(0, s), Duration::from_millis(250));
        assert_eq!(restart_backoff(1, s), Duration::from_secs(1));
        assert_eq!(restart_backoff(2, s), Duration::from_secs(3));
        assert_eq!(restart_backoff(3, s), Duration::from_secs(10));
        assert_eq!(restart_backoff(4, s), Duration::from_secs(30));
    }

    #[test]
    fn restart_backoff_is_capped_not_overflowing() {
        let s = &RESTART_BACKOFF_SCHEDULE;
        for attempt in 5..200 {
            assert_eq!(restart_backoff(attempt, s), Duration::from_secs(30));
        }
    }

    /// 退避必须单调不减 —— 否则「连续失败」反而重试得更频繁
    #[test]
    fn restart_backoff_is_monotonic() {
        let s = &RESTART_BACKOFF_SCHEDULE;
        for attempt in 1..50 {
            assert!(
                restart_backoff(attempt, s) >= restart_backoff(attempt - 1, s),
                "第 {} 次退避比上一次短",
                attempt
            );
        }
    }

    #[test]
    fn transient_backoff_starts_at_initial_and_grows() {
        let initial = Duration::from_millis(200);
        let max = Duration::from_secs(30);
        assert_eq!(transient_backoff(1, initial, max), initial);
        assert_eq!(
            transient_backoff(2, initial, max),
            Duration::from_millis(400)
        );
        assert_eq!(
            transient_backoff(3, initial, max),
            Duration::from_millis(800)
        );
    }

    #[test]
    fn transient_backoff_caps_at_max() {
        let initial = Duration::from_millis(200);
        let max = Duration::from_secs(30);
        for failures in 20..200 {
            assert_eq!(transient_backoff(failures, initial, max), max);
        }
    }

    /// 移位溢出防护：exp 很大时不能 panic
    #[test]
    fn exponential_backoff_does_not_overflow() {
        let d = exponential_backoff(u32::MAX, Duration::from_millis(1), Duration::from_secs(30));
        assert_eq!(d, Duration::from_secs(30));
    }

    #[test]
    fn jitter_stays_within_twenty_percent() {
        let base = Duration::from_millis(1_000);
        let mut rng = StdRng::seed_from_u64(42);
        let mut saw_below = false;
        let mut saw_above = false;

        for _ in 0..1_000 {
            let d = with_jitter(base, &mut rng);
            let ms = d.as_millis();
            assert!(
                (800..=1200).contains(&ms),
                "抖动越界: {}ms 不在 800..=1200 内",
                ms
            );
            if ms < 1_000 {
                saw_below = true;
            }
            if ms > 1_000 {
                saw_above = true;
            }
        }

        // 抖动必须双向，否则它只是把退避整体拉长/缩短，起不到去同步的作用
        assert!(saw_below && saw_above, "抖动应当双向分布");
    }

    #[test]
    fn jitter_is_reproducible_for_a_given_seed() {
        let base = Duration::from_millis(500);
        let mut a = StdRng::seed_from_u64(7);
        let mut b = StdRng::seed_from_u64(7);
        for _ in 0..50 {
            assert_eq!(with_jitter(base, &mut a), with_jitter(base, &mut b));
        }
    }

    #[test]
    fn jitter_never_returns_zero() {
        let base = Duration::from_millis(1);
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..500 {
            assert!(with_jitter(base, &mut rng) >= Duration::from_millis(1));
        }
    }
}
