// src-tauri/src/runtime/lifecycle.rs
// 进程生命周期信息。
//
// 与 status.rs 的分工：status 回答「现在好不好」，lifecycle 回答
// 「这台机器上的这个进程活了多久、是怎么起来的」。
//
// 计划第 33 节还要求一组诊断计数器（translation_success_count /
// fallback_count / clipboard_restart_count …）。它们属于诊断面，
// 计划把它排在 Phase 13，此处不预先造出无人消费的字段。

use std::time::Instant;

/// 进程生命周期起点。
///
/// 用 `Instant` 而非 wall-clock：系统时间被调整不该让「已运行时长」
/// 变成负数或者凭空跳变。
pub struct RuntimeLifecycle {
    started_at: Instant,
}

impl RuntimeLifecycle {
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self::start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_starts_near_zero_and_grows() {
        let l = RuntimeLifecycle::start();
        let first = l.uptime_ms();
        assert!(first < 1_000, "刚启动不该报出很大的运行时长");

        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(l.uptime_ms() >= first, "运行时长必须单调不减");
    }
}
