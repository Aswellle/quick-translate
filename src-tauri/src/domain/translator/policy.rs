// src-tauri/src/domain/translator/policy.rs
// Provider 请求策略与整体时间预算（计划第 13 节）。
//
// 要解决的问题：
//
//     DeepL 超时(5s) → Tencent 超时(5s) → Baidu 超时(5s)
//       → Youdao 超时(5s) → Google 超时(5s)  =  用户干等 25 秒
//
// 重构前每个 provider 各自跑满全局 5 秒超时，五个源都坏掉时用户要等二十几秒。
// 正确做法是给**整次翻译**一个总预算，每个 provider 只能花掉剩余部分，
// 耗尽即停并返回一个可恢复错误（计划第 13 节末）。

use std::time::{Duration, Instant};

use super::error_class::ErrorClass;

/// 一次翻译的总预算。
///
/// 计划第 13 节给的量级是 8 秒（Provider A 2.5s + retry 0.5s + B 2.0s + C 1.5s）。
pub const TOTAL_BUDGET: Duration = Duration::from_secs(8);

/// 单个 provider 的请求策略（计划第 13 节）。
#[derive(Debug, Clone, Copy)]
pub struct RequestPolicy {
    /// 单次尝试的超时（含连接与读取）。
    /// 由引擎用 `tokio::time::timeout` 施加，实际值还会被剩余预算进一步压低。
    pub attempt_timeout: Duration,
    /// 该 provider 最多尝试几次（含首次）。计划第 30 节：<= 2。
    pub max_attempts: u32,
    /// 首次重试前的退避
    pub backoff_base: Duration,
    /// 退避上限
    pub max_backoff: Duration,
    /// 非官方兜底源标记（目前只有 Google）。
    /// 供诊断使用；Phase 9 的 Tray/设置页会据此把它标注为「备用服务」。
    pub best_effort: bool,
}

impl RequestPolicy {
    /// 官方 API：允许一次重试。
    pub const fn official() -> Self {
        Self {
            attempt_timeout: Duration::from_secs(3),
            max_attempts: 2,
            backoff_base: Duration::from_millis(200),
            max_backoff: Duration::from_secs(1),
            best_effort: false,
        }
    }

    /// 非官方兜底源：**不重试**。
    ///
    /// 它用的是非公开接口，随时可能被限流；重试只会更快撞上 429。
    /// 而它是最后一道防线，快速失败能让用户尽早看到明确结果，
    /// 好过在一次注定失败的请求上耗掉本该留给别处的预算。
    pub const fn best_effort() -> Self {
        Self {
            attempt_timeout: Duration::from_secs(2),
            max_attempts: 1,
            backoff_base: Duration::from_millis(200),
            max_backoff: Duration::from_secs(1),
            best_effort: true,
        }
    }

    /// 该错误类别是否值得立刻再试一次**同一个** provider（计划第 8 节）。
    ///
    /// Retry 与 Fallback 是两件事，这里只回答前者。Auth/Quota 试一万次也
    /// 不会好；Permanent 是请求本身的问题；SameLanguage 根本不是失败。
    ///
    /// **RateLimit 刻意不在此列。** 计划第 8 节允许的是「按 Retry-After
    /// 重试最多一次」，但本仓库尚未解析 `Retry-After`（那是计划第 14 节
    /// HTTP Client V2 的范畴）。缺了这个信息，重试 429 就只是盲目重试：
    /// 服务端刚说了「慢一点」，200ms 后再撞一次只会再拿一个 429，
    /// 白白吃掉本该留给其他源的预算。宁可立刻换源。
    /// 等 Retry-After 解析落地后，再把 RateLimit 加回这个列表。
    pub fn should_retry(self, class: ErrorClass) -> bool {
        matches!(class, ErrorClass::Transient)
    }

    /// 第 `attempt` 次尝试失败后的退避（attempt 从 1 起）。
    ///
    /// 与 `crate::util::backoff::exponential_backoff` 同一条曲线，只是这里
    /// 以「第几次尝试」为参数而非「2 的几次幂」，因此就地展开三行而不是
    /// 绕一层调用。
    pub fn backoff_after(self, attempt: u32) -> Duration {
        let exp = attempt.saturating_sub(1);
        let scaled = self
            .backoff_base
            .saturating_mul(2u32.saturating_pow(exp.min(31)));
        scaled.min(self.max_backoff)
    }
}

/// 按 provider id 取策略。
pub fn policy_for(provider_id: &str) -> RequestPolicy {
    match provider_id {
        "google" => RequestPolicy::best_effort(),
        _ => RequestPolicy::official(),
    }
}

/// 一次翻译的时间预算。
///
/// 用 `Instant` 而非 wall-clock：系统时间被调整不该让预算凭空变长或变短。
pub struct TranslationBudget {
    started_at: Instant,
    total: Duration,
}

impl TranslationBudget {
    /// 以当前时刻为起点开一个新预算。
    pub fn new(total: Duration) -> Self {
        Self {
            started_at: Instant::now(),
            total,
        }
    }

    /// 直接指定起点（供测试构造「已耗尽」或「只剩一点」的预算）
    pub fn starting_at(started_at: Instant, total: Duration) -> Self {
        Self { started_at, total }
    }

    /// 剩余预算。已耗尽时为 0，不会 panic 也不会回绕。
    pub fn remaining(&self) -> Duration {
        self.total.saturating_sub(self.started_at.elapsed())
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining().is_zero()
    }

    /// 本次尝试实际可用的超时：策略值与剩余预算取小。
    ///
    /// 这是「总预算」真正生效的地方 —— 最后一个 provider 不会拿到完整的
    /// attempt_timeout，而是只剩多少用多少。
    pub fn attempt_timeout(&self, policy: RequestPolicy) -> Duration {
        policy.attempt_timeout.min(self.remaining())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_policy_allows_one_retry() {
        let p = RequestPolicy::official();
        assert_eq!(
            p.max_attempts, 2,
            "计划第 30 节：per-provider attempts <= 2"
        );
        assert!(!p.best_effort);
    }

    #[test]
    fn best_effort_policy_never_retries() {
        let p = RequestPolicy::best_effort();
        assert_eq!(p.max_attempts, 1);
        assert!(p.best_effort);
    }

    #[test]
    fn google_is_the_only_best_effort_provider() {
        assert!(policy_for("google").best_effort);
        for id in ["deepl", "tencent", "baidu", "youdao"] {
            assert!(!policy_for(id).best_effort, "{} 是官方 API", id);
        }
    }

    // ── 重试判定（计划第 8 节表格）───────────────────────────────────────

    #[test]
    fn only_transient_failures_are_retried_in_place() {
        let p = RequestPolicy::official();
        assert!(p.should_retry(ErrorClass::Transient));

        for class in [
            ErrorClass::Auth,
            ErrorClass::Quota,
            ErrorClass::Permanent,
            ErrorClass::SameLanguage,
            // 未解析 Retry-After 之前重试 429 只是盲目重试，见 should_retry 的说明
            ErrorClass::RateLimit,
        ] {
            assert!(!p.should_retry(class), "{:?} 不该在原地重试", class);
        }
    }

    // ── 重试退避 ─────────────────────────────────────────────────────────

    #[test]
    fn retry_backoff_starts_at_base_then_caps() {
        let p = RequestPolicy {
            backoff_base: Duration::from_millis(200),
            max_backoff: Duration::from_millis(500),
            ..RequestPolicy::official()
        };
        assert_eq!(p.backoff_after(1), Duration::from_millis(200));
        assert_eq!(p.backoff_after(2), Duration::from_millis(400));
        assert_eq!(p.backoff_after(3), Duration::from_millis(500), "应封顶");
        assert_eq!(p.backoff_after(50), Duration::from_millis(500), "不应溢出");
    }

    // ── 预算 ─────────────────────────────────────────────────────────────

    #[test]
    fn fresh_budget_is_not_exhausted() {
        let b = TranslationBudget::new(TOTAL_BUDGET);
        assert!(!b.is_exhausted());
        assert!(b.remaining() <= TOTAL_BUDGET);
    }

    #[test]
    fn expired_budget_reports_zero_remaining() {
        // 起点在预算长度之前 → 已耗尽
        let b = TranslationBudget::starting_at(
            Instant::now() - TOTAL_BUDGET - Duration::from_secs(1),
            TOTAL_BUDGET,
        );
        assert!(b.is_exhausted());
        assert_eq!(b.remaining(), Duration::ZERO);
    }

    /// 总预算真正生效的地方：最后一个 provider 只能拿到剩余部分
    #[test]
    fn attempt_timeout_is_clamped_by_remaining_budget() {
        let policy = RequestPolicy::official(); // attempt_timeout = 3s

        let plenty = TranslationBudget::new(TOTAL_BUDGET);
        assert_eq!(
            plenty.attempt_timeout(policy),
            Duration::from_secs(3),
            "预算充足时应使用策略值"
        );

        // 只剩 500ms
        let tight = TranslationBudget::starting_at(
            Instant::now() - (TOTAL_BUDGET - Duration::from_millis(500)),
            TOTAL_BUDGET,
        );
        let t = tight.attempt_timeout(policy);
        assert!(
            t <= Duration::from_millis(500) && t > Duration::from_millis(300),
            "剩余预算应压低单次超时，实际 {:?}",
            t
        );
    }

    #[test]
    fn exhausted_budget_yields_zero_attempt_timeout() {
        let b = TranslationBudget::starting_at(Instant::now() - TOTAL_BUDGET * 2, TOTAL_BUDGET);
        assert_eq!(b.attempt_timeout(RequestPolicy::official()), Duration::ZERO);
    }
}
