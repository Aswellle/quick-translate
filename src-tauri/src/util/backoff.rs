// src-tauri/src/util/backoff.rs
// 退避计算的公共实现。
//
// 剪贴板 supervisor 的重启退避、clipboard worker 的暂态退避、Provider 熔断
// 冷却与请求重试，四处的数学完全一致，因此放在中立位置而不是让 domain
// 反向依赖 system。

use std::time::Duration;

use rand::Rng;

/// 抖动幅度：±20%
pub const JITTER_RATIO: f64 = 0.2;

/// 对时长施加 ±20% 抖动。
///
/// 目的不是「看起来更随机」，而是避免多个后台组件在同一个故障事件后
/// 同步重试形成尖峰。
pub fn with_jitter(base: Duration, rng: &mut impl Rng) -> Duration {
    let factor: f64 = rng.gen_range((1.0 - JITTER_RATIO)..=(1.0 + JITTER_RATIO));
    let ms = (base.as_millis() as f64 * factor).round().max(1.0) as u64;
    Duration::from_millis(ms)
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

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    // ── 抖动 ─────────────────────────────────────────────────────────────

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

    // ── 指数退避 ─────────────────────────────────────────────────────────

    #[test]
    fn exponential_backoff_doubles_until_capped() {
        let initial = Duration::from_millis(200);
        let max = Duration::from_secs(30);
        assert_eq!(exponential_backoff(0, initial, max), initial);
        assert_eq!(
            exponential_backoff(1, initial, max),
            Duration::from_millis(400)
        );
        assert_eq!(
            exponential_backoff(2, initial, max),
            Duration::from_millis(800)
        );
    }

    #[test]
    fn exponential_backoff_caps_at_max() {
        let initial = Duration::from_millis(200);
        let max = Duration::from_secs(30);
        for exp in 20..200 {
            assert_eq!(exponential_backoff(exp, initial, max), max);
        }
    }

    /// 移位溢出防护：exp 很大时不能 panic
    #[test]
    fn exponential_backoff_does_not_overflow() {
        let d = exponential_backoff(u32::MAX, Duration::from_millis(1), Duration::from_secs(30));
        assert_eq!(d, Duration::from_secs(30));
    }
}
