// src-tauri/src/system/clipboard/controller.rs
// MonitorController —— 剪贴板监控的对外控制面。
//
// 公开 API 与重构前逐字一致（suspend / resume / is_suspended / reset_last_text /
// mark_app_write / unmark_app_write），因此 lib.rs、state.rs、commands/system.rs、
// commands/config.rs、system/tray.rs 五处调用方一行都不用改。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::health::{ClipboardHealth, HealthState};

/// app 内写入的期望登记：绑定内容哈希 + 登记时刻（F4）。
///
/// 取代此前的裸 AtomicBool —— 布尔标志不绑定内容，存在三个吸收错文本的
/// 竞态窗口（写入落地前被消费 / 同窗口用户复制被吞 / 暂停期存活吞掉恢复
/// 后首次复制）。哈希匹配才吸收，天然免疫全部三者；EXPECT_TTL 兜底清理
/// 「写入被用户复制覆盖导致期望永远匹配不上」的残留。
#[derive(Clone, Copy)]
struct ExpectedWrite {
    text_hash: u64,
    at: Instant,
}

/// 期望登记的存活时长：超过即视为写入已被覆盖，丢弃
const EXPECT_TTL: Duration = Duration::from_secs(3);

fn hash_text(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// 控制器句柄：可在运行时暂停/恢复监控，并请求重置 last_text
#[derive(Clone)]
pub struct MonitorController {
    /// 暂停标志（true = 暂停中，不执行翻译检测）
    pub suspended: Arc<AtomicBool>,
    /// hide_popup 请求重置标志：下次循环时吸收当前剪贴板内容，
    /// 确保关闭浮窗后再次复制相同文本仍能触发翻译
    pub reset_requested: Arc<AtomicBool>,
    /// app 内写入的期望登记（复制译文/原文按钮）：
    /// 监控读到哈希匹配的内容时静默吸收，不触发翻译
    expected_write: Arc<Mutex<Option<ExpectedWrite>>>,
    /// 恢复监控时请求吸收当前剪贴板内容，避免恢复后立刻翻译暂停期间复制的东西
    absorb_pending: Arc<AtomicBool>,
    /// worker 停机信号（进程退出路径；也让 supervisor 能在测试中被停下）
    shutdown: Arc<AtomicBool>,
    /// 健康快照。worker 线程写、任意线程读，故用 Mutex 而非原子量。
    health: Arc<Mutex<ClipboardHealth>>,
}

impl MonitorController {
    pub fn new() -> Self {
        Self {
            suspended: Arc::new(AtomicBool::new(false)),
            reset_requested: Arc::new(AtomicBool::new(false)),
            expected_write: Arc::new(Mutex::new(None)),
            absorb_pending: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
            health: Arc::new(Mutex::new(ClipboardHealth::default())),
        }
    }

    /// 暂停监控（暂停期间不触发翻译）
    pub fn suspend(&self) {
        let was = self.suspended.load(Ordering::SeqCst);
        self.suspended.store(true, Ordering::SeqCst);
        tracing::info!(
            "[MonitorController] suspend() called: {} -> suspended={}",
            was,
            true
        );
    }

    /// 恢复监控。
    ///
    /// 从暂停态恢复时必须「吸收」当前剪贴板内容：暂停期间用户复制了什么，
    /// 恢复后不该立刻弹窗翻译它（计划第 5 节）。
    ///
    /// 此前只清 pending 而不更新 last_text，于是恢复后第一轮 `is_new` 判为真，
    /// 300ms 后就弹窗。启动路径必然踩到：config 里 clipboard_monitor_enabled=false
    /// 时 lib.rs 会先调 suspend()，用户在设置里一开启就触发。
    pub fn resume(&self) {
        let was = self.suspended.load(Ordering::SeqCst);
        self.suspended.store(false, Ordering::SeqCst);
        if was {
            self.absorb_pending.store(true, Ordering::SeqCst);
        }
        tracing::info!(
            "[MonitorController] resume() called: {} -> suspended={}, absorb_pending={}",
            was,
            false,
            was
        );
    }

    /// 是否处于暂停状态
    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::SeqCst)
    }

    /// 请求监控线程在下次循环时重置 last_text。
    /// 由 hide_popup 调用，确保关闭浮窗后再次复制相同文本能重新触发翻译。
    pub fn reset_last_text(&self) {
        self.reset_requested.store(true, Ordering::SeqCst);
        tracing::info!("[MonitorController] reset_last_text() 已请求");
    }

    /// 请求停机。worker 与 supervisor 都会在下一次循环检查时退出。
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// 登记一次 app 内剪贴板写入（复制译文/原文按钮）。
    /// 哈希取自 normalize_text 后的文本 —— Windows 剪贴板往返可能改写
    /// 换行（LF↔CRLF），原文哈希会永远匹配不上导致吸收失效；两侧统一
    /// 归一化后比较即可免疫。
    pub fn mark_app_write(&self, text: &str) {
        let mut slot = self.expected_write.lock().unwrap();
        *slot = Some(ExpectedWrite {
            text_hash: hash_text(&super::normalize_text(text)),
            at: Instant::now(),
        });
        tracing::info!("[MonitorController] mark_app_write() 已登记期望哈希");
    }

    /// 撤销登记：写剪贴板失败时调用，避免残留期望（F5）。
    pub fn unmark_app_write(&self) {
        *self.expected_write.lock().unwrap() = None;
        tracing::info!("[MonitorController] unmark_app_write() 已撤销（写入失败）");
    }

    /// worker 调用：当前内容（已归一化）是否匹配已登记的 app 写入。
    /// 匹配 → 消费登记并返回 true（调用方应吸收该内容）；
    /// 不匹配 → 保留登记（写入可能尚未落地），但超过 EXPECT_TTL 则丢弃
    /// （写入已被用户复制覆盖，期望永远无法匹配）。
    pub(crate) fn consume_if_expected(&self, current_normalized: &str) -> bool {
        let mut slot = self.expected_write.lock().unwrap();
        match *slot {
            Some(exp) if exp.text_hash == hash_text(current_normalized) => {
                *slot = None;
                true
            }
            Some(exp) if exp.at.elapsed() > EXPECT_TTL => {
                tracing::info!("[MonitorController] 期望登记超时丢弃（写入已被覆盖）");
                *slot = None;
                false
            }
            _ => false,
        }
    }

    /// worker 调用：取出并清除「恢复后吸收」请求。
    pub(crate) fn take_absorb_request(&self) -> bool {
        self.absorb_pending.swap(false, Ordering::SeqCst)
    }

    /// 恢复请求是否待处理但不消费（supervisor 判断重启后是否需要吸收）。仅供测试断言。
    #[cfg(test)]
    pub(crate) fn absorb_is_pending(&self) -> bool {
        self.absorb_pending.load(Ordering::SeqCst)
    }

    // ── 健康快照 ──────────────────────────────────────────────────────────

    pub(crate) fn health_mut(&self) -> std::sync::MutexGuard<'_, ClipboardHealth> {
        self.health.lock().unwrap()
    }

    /// 读取健康快照（供 tray / settings / 诊断页使用，Phase 2 先备好接口）
    pub fn health_snapshot(&self) -> ClipboardHealth {
        self.health.lock().unwrap().clone()
    }

    pub fn health_state(&self) -> HealthState {
        self.health.lock().unwrap().state
    }
}

impl Default for MonitorController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_from_suspended_requests_absorb() {
        let c = MonitorController::new();
        c.suspend();
        assert!(!c.absorb_is_pending());

        c.resume();
        assert!(
            c.absorb_is_pending(),
            "从暂停恢复必须请求吸收当前剪贴板内容"
        );
    }

    #[test]
    fn resume_when_not_suspended_does_not_request_absorb() {
        let c = MonitorController::new();
        c.resume();
        assert!(
            !c.absorb_is_pending(),
            "本来就没暂停，恢复不该丢弃当前剪贴板内容"
        );
    }

    #[test]
    fn absorb_request_is_consumed_once() {
        let c = MonitorController::new();
        c.suspend();
        c.resume();
        assert!(c.take_absorb_request());
        assert!(!c.take_absorb_request(), "吸收请求只能被消费一次");
    }

    #[test]
    fn expected_write_matches_only_after_normalization() {
        let c = MonitorController::new();
        // 登记时带 CRLF，监控侧读到的是 LF（Windows 剪贴板往返会改写换行）
        c.mark_app_write("hello\r\nworld");
        assert!(
            c.consume_if_expected("hello world"),
            "两侧都归一化后应当匹配"
        );
    }

    #[test]
    fn expected_write_survives_until_consumed_then_is_gone() {
        let c = MonitorController::new();
        c.mark_app_write("payload");
        assert!(
            !c.consume_if_expected("something else"),
            "不匹配时不得消费登记"
        );
        assert!(c.consume_if_expected("payload"), "匹配时消费登记");
        assert!(
            !c.consume_if_expected("payload"),
            "登记已被消费，不该二次命中"
        );
    }

    #[test]
    fn unmark_cancels_pending_expectation() {
        let c = MonitorController::new();
        c.mark_app_write("payload");
        c.unmark_app_write();
        assert!(
            !c.consume_if_expected("payload"),
            "撤销后不该再吸收用户真实复制的内容"
        );
    }

    #[test]
    fn shutdown_starts_unrequested() {
        let c = MonitorController::new();
        assert!(!c.is_shutdown_requested());
        c.request_shutdown();
        assert!(c.is_shutdown_requested());
    }
}
