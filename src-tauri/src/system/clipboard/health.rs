// src-tauri/src/system/clipboard/health.rs
// 剪贴板子系统健康快照。
//
// 计划第 4 节要求 UI 不自己推测状态（「因为 Provider 失败所以认为网络断了」），
// 状态必须由 health 层给出结论。全局 RuntimeStatus 是 Phase 1 的范畴；
// Phase 2 只落剪贴板一份，字段命名对齐计划第 4 节的 ComponentHealth，
// 届时直接接上去即可。

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// 正常读取中
    Healthy,
    /// 有失败但仍在重试窗口内 —— 功能可能仍可用，UI 不应报警
    Degraded,
    /// worker 已退出或正在重建句柄，supervisor 正在恢复
    Recovering,
}

/// 剪贴板健康快照。**不得包含用户复制的内容**（计划第 32 节）——
/// 只放错误码、计数与时间戳。
#[derive(Debug, Clone, Serialize)]
pub struct ClipboardHealth {
    pub state: HealthState,
    /// 当前连续失败次数（成功一次即归零）
    pub consecutive_failures: u32,
    /// worker 被 supervisor 重建的次数。
    /// 这是 Phase 2 所修故障的直接指标 —— 修复前该值永远为 0，
    /// 因为线程死了也没人观察。持续增长说明剪贴板初始化在反复失败。
    pub worker_restarts: u32,
    pub last_error_code: Option<&'static str>,
    pub last_error_at_ms: Option<i64>,
    pub last_recovery_at_ms: Option<i64>,
}

impl Default for ClipboardHealth {
    fn default() -> Self {
        Self {
            state: HealthState::Healthy,
            consecutive_failures: 0,
            worker_restarts: 0,
            last_error_code: None,
            last_error_at_ms: None,
            last_recovery_at_ms: None,
        }
    }
}

impl ClipboardHealth {
    /// 一次暂时性失败：降级但不报警（下一次成功即恢复）。
    pub fn note_failure(&mut self, code: &'static str) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.state = HealthState::Degraded;
        self.last_error_code = Some(code);
        self.last_error_at_ms = Some(crate::types::now_unix_ms());
    }

    /// 一次成功读取。此前有失败则记录恢复时刻（计划第 5 节要求可追踪）。
    /// 返回 true 表示这是一次「从失败中恢复」，调用方据此打恢复日志。
    pub fn note_success(&mut self) -> bool {
        let recovered = self.consecutive_failures > 0;
        if recovered {
            self.consecutive_failures = 0;
            self.last_recovery_at_ms = Some(crate::types::now_unix_ms());
        }
        self.state = HealthState::Healthy;
        recovered
    }

    /// worker 因致命错误退出：supervisor 即将重建，进入 Recovering。
    pub fn note_worker_exit(&mut self, code: &'static str) {
        self.state = HealthState::Recovering;
        self.last_error_code = Some(code);
        self.last_error_at_ms = Some(crate::types::now_unix_ms());
    }

    /// supervisor 重建了一次 worker。
    pub fn note_worker_restart(&mut self) {
        self.worker_restarts = self.worker_restarts.saturating_add(1);
        self.state = HealthState::Recovering;
        self.consecutive_failures = 0;
    }

    /// 新 worker 成功建起句柄，恢复读取。
    pub fn note_worker_started(&mut self, is_restart: bool) {
        self.state = HealthState::Healthy;
        if is_restart {
            self.last_recovery_at_ms = Some(crate::types::now_unix_ms());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_after_failures_reports_recovery() {
        let mut h = ClipboardHealth::default();
        assert!(!h.note_success(), "无失败时不应报告恢复");

        h.note_failure("CLIPBOARD_OCCUPIED");
        h.note_failure("CLIPBOARD_OCCUPIED");
        assert_eq!(h.state, HealthState::Degraded);
        assert_eq!(h.consecutive_failures, 2);
        assert_eq!(h.last_error_code, Some("CLIPBOARD_OCCUPIED"));

        assert!(h.note_success(), "从失败中恢复应报告 true");
        assert_eq!(h.state, HealthState::Healthy);
        assert_eq!(h.consecutive_failures, 0);
        assert!(h.last_recovery_at_ms.is_some());
    }

    #[test]
    fn worker_restart_resets_failure_count() {
        let mut h = ClipboardHealth::default();
        h.note_failure("CLIPBOARD_INIT_FAILED");
        h.note_worker_exit("CLIPBOARD_INIT_FAILED");
        assert_eq!(h.state, HealthState::Recovering);

        h.note_worker_restart();
        assert_eq!(h.worker_restarts, 1);
        assert_eq!(h.consecutive_failures, 0);

        h.note_worker_started(true);
        assert_eq!(h.state, HealthState::Healthy);
    }

    /// 错误码是 &'static str，天然不可能把用户复制的内容带进健康快照
    #[test]
    fn health_snapshot_serializes_without_payload() {
        let mut h = ClipboardHealth::default();
        h.note_failure("CLIPBOARD_READ_FAILED");
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("CLIPBOARD_READ_FAILED"));
        assert!(json.contains("degraded"));
    }
}
