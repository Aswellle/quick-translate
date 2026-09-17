// src-tauri/src/domain/translator/health.rs
// Provider 熔断器（计划第 9 节）。
//
// 要解决的问题：
//
//     DeepL 宕机
//       ↓
//     用户复制 A → 撞 DeepL → 等超时 → 失败
//       ↓
//     用户复制 B → 又撞 DeepL → 又等超时 → 失败
//       ↓
//     …每次复制都白等一遍注定失败的请求
//
// 熔断器把「这个 provider 现在不可用」这一事实记住，让后续请求直接跳过它。
//
// 状态机（计划第 9 节）：
//
//     Healthy ──失败达阈值──► Open ──冷却到期──► HalfOpen
//        ▲                                          │
//        └──────────── 探测成功 ────────────────────┤
//                                                    │
//        Open ◄──────────── 探测失败 ────────────────┘
//
// 时间全部由调用方以 `now: Instant` 传入 —— 不用真实时钟、不用 fake clock，
// 冷却到期这类边界因此可以精确断言（计划第 54 节）。
// 用 `Instant` 而非 wall-clock：系统时间被调整不该影响冷却判定。

use std::time::{Duration, Instant};

use rand::Rng;

use crate::util::jitter::with_jitter;

use super::error_class::ErrorClass;

/// 连续多少次「计入熔断」的失败后打开熔断（计划第 9 节）
pub const FAILURE_THRESHOLD: u32 = 3;

/// 熔断冷却表（计划第 9 节）：10s → 30s → 2min，此后封顶 10min
pub const COOLDOWN_SCHEDULE: [Duration; 4] = [
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(120),
    Duration::from_secs(600),
];

/// Auth / Quota 用的长冷却：在用户改凭证或充值之前不可能自愈，
/// 用常规冷却等于每隔 10 秒再去撞一次注定失败的请求。
pub const LONG_COOLDOWN: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHealthState {
    /// 正常
    Healthy,
    /// 有失败但未达阈值。功能可能仍可用 —— UI 不该据此报警。
    Degraded,
    /// 熔断打开：冷却期内不再向该 provider 发起请求
    Open,
    /// 冷却到期，放**一个**探测请求；其余请求继续被挡
    HalfOpen,
}

/// 单个 provider 的健康状态。
///
/// 方法都接收 `now: Instant` 而不是自己读时钟，因此整个状态机是确定性的：
/// 测试可以精确地把时间推到「冷却到期前 1ms」与「到期后 1ms」两侧。
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    state: ProviderHealthState,
    consecutive_failures: u32,
    /// 熔断打开过几次 —— 决定冷却时长走表的第几档
    open_count: u32,
    opened_at: Option<Instant>,
    /// 本次熔断的冷却时长（已含抖动）
    cooldown: Duration,
    /// HalfOpen 时是否已有探测在飞。计划第 41 节：max 1 probe / provider
    probing: bool,
    last_error_class: Option<ErrorClass>,
    last_failure_at: Option<Instant>,
    last_success_at: Option<Instant>,
}

impl ProviderHealth {
    pub fn new() -> Self {
        Self {
            state: ProviderHealthState::Healthy,
            consecutive_failures: 0,
            open_count: 0,
            opened_at: None,
            cooldown: Duration::ZERO,
            probing: false,
            last_error_class: None,
            last_failure_at: None,
            last_success_at: None,
        }
    }

    pub fn state(&self) -> ProviderHealthState {
        self.state
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn open_count(&self) -> u32 {
        self.open_count
    }

    pub fn last_error_class(&self) -> Option<ErrorClass> {
        self.last_error_class
    }

    /// 本次请求是否可以使用该 provider。
    ///
    /// Open 且冷却到期时会**就地**转入 HalfOpen 并占住唯一的探测名额，
    /// 因此并发请求里只有一个能通过 —— 计划第 9 节「只允许一个探测请求，
    /// 其他请求不撞击该 Provider」。
    pub fn try_acquire(&mut self, now: Instant) -> bool {
        match self.state {
            ProviderHealthState::Healthy | ProviderHealthState::Degraded => true,
            ProviderHealthState::Open => {
                let expired = self
                    .opened_at
                    .is_some_and(|t| now.duration_since(t) >= self.cooldown);
                if expired {
                    self.state = ProviderHealthState::HalfOpen;
                    self.probing = true;
                    tracing::info!(
                        event = "provider_circuit_half_open",
                        "[provider] 冷却到期，放行一个探测请求"
                    );
                    true
                } else {
                    false
                }
            }
            // 探测已在飞：其余请求继续被挡
            ProviderHealthState::HalfOpen => false,
        }
    }

    /// 一次成功。无论此前处于哪个状态，都回到 Healthy 并把退避阶梯清零。
    pub fn record_success(&mut self, now: Instant) {
        if self.state != ProviderHealthState::Healthy {
            tracing::info!(
                event = "provider_circuit_closed",
                previous = ?self.state,
                "[provider] 恢复健康"
            );
        }
        self.close(now);
    }

    /// 一次失败。按 `class` 决定是否影响熔断。
    pub fn record_failure(&mut self, class: ErrorClass, now: Instant, rng: &mut impl Rng) {
        self.last_error_class = Some(class);
        self.last_failure_at = Some(now);

        let was_probing = self.probing;
        self.probing = false;

        if !class.counts_toward_circuit() {
            // Permanent / SameLanguage：不是 provider 的健康问题，状态机不动。
            //
            // 但如果刚才是在探测，必须收尾 —— 探测名额已归还而状态仍停在
            // HalfOpen 的话，`try_acquire` 会对它永久返回 false，
            // 该 provider 就此被彻底挡住。而探测能拿到回应本身就说明它是可达的，
            // 熔断器衡量的正是可达性，所以这次探测算通过。
            if was_probing {
                self.close(now);
            }
            return;
        }

        self.consecutive_failures = self.consecutive_failures.saturating_add(1);

        // 探测失败立刻重新熔断（不看阈值）；常规失败要攒够阈值
        if was_probing || self.consecutive_failures >= FAILURE_THRESHOLD {
            self.open(now, class, rng);
        } else {
            self.state = ProviderHealthState::Degraded;
        }
    }

    /// 凭证被替换后调用：整个健康状态归零。
    ///
    /// 没有这一步的话，用户把 Key 改对了仍然要在冷却里干等 10 分钟 ——
    /// 而用户刚修完配置时的预期是「现在应该能用了」。
    pub fn reset(&mut self) {
        self.close(Instant::now());
        self.last_error_class = None;
        self.last_failure_at = None;
        tracing::info!(
            event = "provider_health_reset",
            "[provider] 凭证已更新，健康状态重置"
        );
    }

    fn close(&mut self, now: Instant) {
        self.state = ProviderHealthState::Healthy;
        self.consecutive_failures = 0;
        self.open_count = 0;
        self.probing = false;
        self.opened_at = None;
        self.cooldown = Duration::ZERO;
        self.last_success_at = Some(now);
    }

    fn open(&mut self, now: Instant, class: ErrorClass, rng: &mut impl Rng) {
        self.open_count = self.open_count.saturating_add(1);
        self.cooldown = cooldown_for(class, self.open_count, rng);
        self.opened_at = Some(now);
        self.state = ProviderHealthState::Open;
        self.probing = false;

        tracing::warn!(
            event = "provider_circuit_open",
            class = ?class,
            open_count = self.open_count,
            cooldown_ms = self.cooldown.as_millis() as u64,
            "[provider] 熔断打开，冷却 {:?}",
            self.cooldown
        );
    }
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// 第 `open_count` 次熔断的冷却时长（已含 ±20% 抖动）。
///
/// 纯函数（抖动源注入），因此冷却曲线本身可被逐项断言。
pub fn cooldown_for(class: ErrorClass, open_count: u32, rng: &mut impl Rng) -> Duration {
    let base = if class.uses_long_cooldown() {
        LONG_COOLDOWN
    } else {
        let idx = open_count
            .saturating_sub(1)
            .min(COOLDOWN_SCHEDULE.len() as u32 - 1) as usize;
        COOLDOWN_SCHEDULE[idx]
    };
    with_jitter(base, rng)
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    /// 便于在测试里把时间推来推去。用 checked_add 而非 `+`，
    /// 免得「推到 10 分钟后」意外溢出 Instant 的表示范围。
    fn at(base: Instant, d: Duration) -> Instant {
        base.checked_add(d).expect("测试时间不应溢出")
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(2024)
    }

    // ── 计划第 35 节测试 2：连续失败 → 熔断打开 → 后续请求跳过 ──────────

    #[test]
    fn consecutive_failures_open_the_circuit() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        // 阈值前仍放行
        for i in 1..FAILURE_THRESHOLD {
            assert!(h.try_acquire(t0), "第 {} 次失败前应仍放行", i);
            h.record_failure(ErrorClass::Transient, t0, &mut r);
            assert_eq!(h.state(), ProviderHealthState::Degraded);
        }

        assert!(h.try_acquire(t0));
        h.record_failure(ErrorClass::Transient, t0, &mut r);
        assert_eq!(
            h.state(),
            ProviderHealthState::Open,
            "连续 {} 次失败后必须熔断",
            FAILURE_THRESHOLD
        );

        // 冷却期内不再放行
        assert!(!h.try_acquire(t0), "冷却期内不得再请求该 provider");
        assert!(!h.try_acquire(at(t0, Duration::from_secs(1))));
    }

    // ── 计划第 35 节测试 4：冷却到期 → 半开探测 → 恢复 ──────────────────

    #[test]
    fn cooldown_expiry_allows_exactly_one_probe() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        h.record_failure(ErrorClass::Transient, t0, &mut r);
        h.record_failure(ErrorClass::Transient, t0, &mut r);
        h.record_failure(ErrorClass::Transient, t0, &mut r);
        assert_eq!(h.state(), ProviderHealthState::Open);

        // 冷却到期前 1ms：仍不放行
        assert!(!h.try_acquire(at(t0, h.cooldown - Duration::from_millis(1))));

        // 到期：放行，且只放行一个
        let after = at(t0, h.cooldown + Duration::from_millis(1));
        assert!(h.try_acquire(after), "冷却到期应放行探测请求");
        assert_eq!(h.state(), ProviderHealthState::HalfOpen);
        assert!(
            !h.try_acquire(after),
            "半开态只允许一个探测，其余请求必须继续被挡（计划第 9 节）"
        );
        assert!(!h.try_acquire(at(after, Duration::from_secs(5))));
    }

    #[test]
    fn successful_probe_closes_the_circuit() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Transient, t0, &mut r);
        }
        assert_eq!(h.state(), ProviderHealthState::Open);

        let after = at(t0, h.cooldown + Duration::from_millis(1));
        assert!(h.try_acquire(after));
        h.record_success(after);

        assert_eq!(h.state(), ProviderHealthState::Healthy);
        assert_eq!(h.consecutive_failures(), 0);
        assert_eq!(h.open_count(), 0, "恢复后退避阶梯应清零");
        assert!(h.try_acquire(after), "恢复后应正常放行");
    }

    #[test]
    fn failed_probe_reopens_with_a_longer_cooldown() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Transient, t0, &mut r);
        }
        let first_cooldown = h.cooldown;

        let after = at(t0, first_cooldown + Duration::from_millis(1));
        assert!(h.try_acquire(after));
        h.record_failure(ErrorClass::Transient, after, &mut r);

        assert_eq!(h.state(), ProviderHealthState::Open);
        assert_eq!(h.open_count(), 2);
        assert!(
            h.cooldown > first_cooldown,
            "第二次熔断的冷却应更长：{:?} vs {:?}",
            h.cooldown,
            first_cooldown
        );
    }

    /// 探测失败必须立刻重新熔断，而不是重新攒阈值
    #[test]
    fn failed_probe_reopens_immediately_without_reaching_threshold() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Transient, t0, &mut r);
        }
        let after = at(t0, h.cooldown + Duration::from_millis(1));
        h.try_acquire(after);

        // 只失败一次
        h.record_failure(ErrorClass::Transient, after, &mut r);
        assert_eq!(
            h.state(),
            ProviderHealthState::Open,
            "探测失败应立即重新熔断，不该重新攒 3 次"
        );
    }

    // ── 计划第 35 节测试 5：AuthError 无盲目重试 + 长冷却 ────────────────

    #[test]
    fn auth_failure_uses_a_long_cooldown() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Auth, t0, &mut r);
        }
        assert_eq!(h.state(), ProviderHealthState::Open);
        assert!(
            h.cooldown >= Duration::from_secs(600) * 8 / 10,
            "认证失败的冷却应接近 10 分钟，实际 {:?}",
            h.cooldown
        );
    }

    #[test]
    fn quota_failure_uses_a_long_cooldown() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();
        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Quota, t0, &mut r);
        }
        assert!(h.cooldown >= Duration::from_secs(480));
    }

    // ── 计划第 35 节测试 6：SameLanguage 不算失败 ────────────────────────

    #[test]
    fn same_language_never_opens_the_circuit() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..50 {
            h.record_failure(ErrorClass::SameLanguage, t0, &mut r);
        }
        assert_eq!(
            h.state(),
            ProviderHealthState::Healthy,
            "同语言不是故障，绝不能熔断一个完全正常的 provider"
        );
        assert!(h.try_acquire(t0));
    }

    #[test]
    fn permanent_errors_never_open_the_circuit() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..50 {
            h.record_failure(ErrorClass::Permanent, t0, &mut r);
        }
        assert_eq!(h.state(), ProviderHealthState::Healthy);
        assert_eq!(h.consecutive_failures(), 0);
    }

    /// 探测期间收到 Permanent：provider 明明可达，不能被永久卡在 HalfOpen
    #[test]
    fn permanent_failure_during_probe_releases_the_probe_slot() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Transient, t0, &mut r);
        }
        let after = at(t0, h.cooldown + Duration::from_millis(1));
        assert!(h.try_acquire(after));
        assert_eq!(h.state(), ProviderHealthState::HalfOpen);

        // 探测拿到了回应，只是这次输入 provider 不收
        h.record_failure(ErrorClass::Permanent, after, &mut r);

        assert_eq!(
            h.state(),
            ProviderHealthState::Healthy,
            "有回应即证明可达，探测应算通过"
        );
        assert!(h.try_acquire(after), "该 provider 不能被永久卡在 HalfOpen");
    }

    // ── 凭证更新后的重置 ─────────────────────────────────────────────────

    #[test]
    fn reset_clears_a_stuck_circuit() {
        let t0 = Instant::now();
        let mut h = ProviderHealth::new();
        let mut r = rng();

        for _ in 0..FAILURE_THRESHOLD {
            h.record_failure(ErrorClass::Auth, t0, &mut r);
        }
        assert_eq!(h.state(), ProviderHealthState::Open);
        assert!(!h.try_acquire(t0));

        h.reset();

        assert_eq!(h.state(), ProviderHealthState::Healthy);
        assert!(
            h.try_acquire(t0),
            "改对凭证后应立刻可用，而不是干等 10 分钟冷却"
        );
        assert_eq!(h.last_error_class(), None);
        assert_eq!(h.open_count(), 0);
    }

    // ── 冷却曲线本身 ─────────────────────────────────────────────────────

    #[test]
    fn cooldown_schedule_matches_the_plan() {
        let mut r = rng();
        // 抖动 ±20%，用区间断言
        let cases = [
            (1u32, Duration::from_secs(10)),
            (2, Duration::from_secs(30)),
            (3, Duration::from_secs(120)),
            (4, Duration::from_secs(600)),
            (9, Duration::from_secs(600)),
        ];
        for (n, expected) in cases {
            let got = cooldown_for(ErrorClass::Transient, n, &mut r);
            let lo = expected.mul_f64(0.8) - Duration::from_millis(1);
            let hi = expected.mul_f64(1.2) + Duration::from_millis(1);
            assert!(
                got >= lo && got <= hi,
                "第 {} 次冷却应在 {:?} 附近，实际 {:?}",
                n,
                expected,
                got
            );
        }
    }

    #[test]
    fn cooldown_is_monotonic_in_open_count() {
        let mut r = rng();
        let mut prev = Duration::ZERO;
        for n in 1..=COOLDOWN_SCHEDULE.len() as u32 {
            // 取抖动下界比较，避免抖动掩盖单调性
            let got = cooldown_for(ErrorClass::Transient, n, &mut r);
            assert!(got.mul_f64(0.8) >= prev, "第 {} 次冷却不该比上一次短", n);
            prev = got.mul_f64(1.2);
        }
    }
}
