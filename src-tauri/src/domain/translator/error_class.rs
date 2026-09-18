// src-tauri/src/domain/translator/error_class.rs
// 翻译源错误的分类与策略表（计划第 8 节）。
//
// 重构前 `TranslationEngine::translate` 对所有错误一视同仁：
//
//     Err(e) => { warn!; errors.push(...); if !fallback { return Err } }
//
// 也就是「任何错误 → 换下一个 provider」。于是：
//   - 认证失败会被反复重试（重试一万次也不会好）
//   - 语种不支持会被当成 provider 故障，把本该健康的翻译源标记成坏的
//   - 429 与 500 用同一种退避节奏（前者该看 Retry-After，后者该快速换源）
//
// 计划第 8 节的原则：**Retry 是同一个 provider 的暂态失败恢复；
// Fallback 是换 provider。两者不能混为一谈。** 这个模块就是那条分界线。

use crate::error::AppError;

/// 一次失败的性质。决定「要不要重试」「要不要换源」「要不要熔断」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// 连接/请求超时、DNS 与传输层失败、HTTP 408、5xx。
    /// 同一个 provider 重试有意义。
    Transient,
    /// HTTP 429。按 Retry-After 退避，重试次数有限。
    RateLimit,
    /// 401 / 403。重试无意义 —— 要用户改凭证才有用。
    Auth,
    /// 额度耗尽。重试无意义 —— 要用户充值或换源。
    Quota,
    /// 不支持的语种、输入过长、本地配置错误。
    /// 换 provider 大概率也没用，因此默认不 fallback。
    Permanent,
    /// 源语言与目标语言相同。**这不是失败**，是一次正常的空操作。
    SameLanguage,
}

impl ErrorClass {
    /// 是否计入熔断器的失败计数。
    ///
    /// Permanent 不计入：语种不支持不是 provider 的健康问题。
    /// 让它把熔断器打开，用户就白白失去一个本来完全正常的翻译源 ——
    /// 而且冷却期内连别的语种也翻不了。
    pub fn counts_toward_circuit(self) -> bool {
        match self {
            Self::Transient | Self::RateLimit | Self::Auth | Self::Quota => true,
            Self::Permanent | Self::SameLanguage => false,
        }
    }

    /// 是否允许换下一个 provider。
    ///
    /// Permanent 不换：同一个请求打到哪家都是同样结果，换源只是让用户
    /// 多等几个超时。SameLanguage 不换：它根本不是失败。
    pub fn allows_fallback(self) -> bool {
        match self {
            Self::Transient | Self::RateLimit | Self::Auth | Self::Quota => true,
            Self::Permanent | Self::SameLanguage => false,
        }
    }

    /// 是否该用长冷却。
    ///
    /// Auth / Quota 在用户动手之前不可能自愈，用常规的 10 秒冷却意味着
    /// 每隔 10 秒就再去撞一次注定失败的请求。给长冷却，同时在用户更新
    /// 凭证时把健康状态整个重置（见 TranslationEngine::update_provider_credentials）。
    pub fn uses_long_cooldown(self) -> bool {
        matches!(self, Self::Auth | Self::Quota)
    }
}

/// 把 `AppError` 归类。
///
/// 纯函数 —— 分类规则的正确性不需要任何 I/O、不需要启动 Tauri 就能验证。
///
/// 这里**穷举**所有变体而不写 `_ =>`：将来给 `AppError` 加变体时，
/// 编译器会强制回来决定它属于哪一类，而不是默默落进某个兜底分支。
pub fn classify(err: &AppError) -> ErrorClass {
    match err {
        // 超时与传输层失败：同一 provider 重试有意义
        AppError::Timeout { .. } | AppError::NetworkError(_) => ErrorClass::Transient,

        AppError::RateLimit { .. } => ErrorClass::RateLimit,
        AppError::AuthError { .. } => ErrorClass::Auth,
        AppError::QuotaExhausted { .. } => ErrorClass::Quota,

        // 不是失败，是正常的空操作
        AppError::SameLanguage { .. } => ErrorClass::SameLanguage,

        // 输入的锅，不是 provider 的锅。
        // ProviderRejected 同理：4xx 表示「这个请求被拒绝了」，
        // 换一家还是同样结果，而 provider 本身是健康的。
        AppError::EmptyText | AppError::NonTextContent | AppError::ProviderRejected { .. } => {
            ErrorClass::Permanent
        }

        // 本地配置错误：换源也没用，得先修配置
        AppError::ConfigError(_) => ErrorClass::Permanent,

        // 聚合错误：它只应由引擎自己产生，不该出现在单 provider 的返回值里。
        // 归为 Permanent 会让「不该发生」表现为「不去重试」，比误熔断安全。
        AppError::AllProvidersFailed { .. } => ErrorClass::Permanent,

        // 以下与翻译源无关（剪贴板/数据库/窗口/加密/序列化）。
        // 走到这里说明调用链有问题，同样不做重试与熔断。
        AppError::ClipboardError(_)
        | AppError::DatabaseError(_)
        | AppError::DatabaseMigration { .. }
        | AppError::WindowError(_)
        | AppError::CryptoError(_)
        | AppError::SerdeError(_) => ErrorClass::Permanent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cls(err: AppError) -> ErrorClass {
        classify(&err)
    }

    // ── 计划第 8 节表格逐项核对 ──────────────────────────────────────────

    #[test]
    fn timeout_and_transport_are_transient() {
        assert_eq!(
            cls(AppError::Timeout { timeout_secs: 5 }),
            ErrorClass::Transient
        );
        assert_eq!(
            cls(AppError::NetworkError("dns failure".into())),
            ErrorClass::Transient
        );
    }

    #[test]
    fn rate_limit_is_its_own_class() {
        assert_eq!(
            cls(AppError::RateLimit {
                provider: "deepl".into()
            }),
            ErrorClass::RateLimit
        );
    }

    #[test]
    fn auth_and_quota_are_distinguished() {
        assert_eq!(
            cls(AppError::AuthError {
                provider: "deepl".into()
            }),
            ErrorClass::Auth
        );
        assert_eq!(
            cls(AppError::QuotaExhausted {
                provider: "deepl".into()
            }),
            ErrorClass::Quota
        );
    }

    #[test]
    fn same_language_is_not_a_failure() {
        let c = cls(AppError::SameLanguage { lang: "zh".into() });
        assert_eq!(c, ErrorClass::SameLanguage);
        assert!(!c.allows_fallback(), "同语言不是失败，换源毫无意义");
        assert!(!c.counts_toward_circuit(), "同语言不该影响 provider 健康");
    }

    #[test]
    fn permanent_errors_neither_fallback_nor_open_the_circuit() {
        let c = cls(AppError::NonTextContent);
        assert_eq!(c, ErrorClass::Permanent);
        assert!(
            !c.counts_toward_circuit(),
            "语种/输入问题不是 provider 的健康问题 —— 熔断它会白白损失一个正常的翻译源"
        );
        assert!(!c.allows_fallback(), "换一家也是同样结果");
    }

    #[test]
    fn auth_and_quota_use_long_cooldown() {
        assert!(ErrorClass::Auth.uses_long_cooldown());
        assert!(ErrorClass::Quota.uses_long_cooldown());
        assert!(!ErrorClass::Transient.uses_long_cooldown());
        assert!(!ErrorClass::RateLimit.uses_long_cooldown());
    }

    /// Auth/Quota 重试无意义，但仍然要换源 —— 用户配了别家就该用别家
    #[test]
    fn auth_and_quota_still_allow_fallback() {
        assert!(ErrorClass::Auth.allows_fallback());
        assert!(ErrorClass::Quota.allows_fallback());
        assert!(ErrorClass::Auth.counts_toward_circuit());
        assert!(ErrorClass::Quota.counts_toward_circuit());
    }

    #[test]
    fn transient_failures_count_and_fallback() {
        for c in [ErrorClass::Transient, ErrorClass::RateLimit] {
            assert!(c.counts_toward_circuit());
            assert!(c.allows_fallback());
        }
    }

    /// 与 provider 无关的错误不该被当成翻译源故障
    #[test]
    fn infrastructure_errors_are_permanent() {
        for err in [
            AppError::ClipboardError("x".into()),
            AppError::DatabaseError("x".into()),
            AppError::WindowError("x".into()),
            AppError::SerdeError("x".into()),
        ] {
            let c = classify(&err);
            assert_eq!(c, ErrorClass::Permanent);
            assert!(!c.counts_toward_circuit());
        }
    }
}
