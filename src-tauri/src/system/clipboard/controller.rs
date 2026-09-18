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
    /// 运行时层句柄。构造顺序上 runtime 需要 controller 才能建，
    /// 因此这里是先建后 `attach_runtime()` 接上，用 OnceLock 表达「只设一次」。
    /// 包一层 Arc 是为了让 MonitorController 仍然可 Clone。
    runtime: Arc<std::sync::OnceLock<Arc<crate::runtime::RuntimeStatus>>>,
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
            runtime: Arc::new(std::sync::OnceLock::new()),
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

    // ── 健康状态 ──────────────────────────────────────────────────────────
    //
    // 健康变更**只能**经由下面这些方法。刻意不暴露 `health_mut()`：
    // 一旦外部能直接改状态，就会有人绕过运行时层，导致
    // `runtime-status-changed` 少发一次、界面看到的状态与实际不符。
    // 每个方法改完都调 `publish_runtime()`。

    fn health(&self) -> std::sync::MutexGuard<'_, ClipboardHealth> {
        self.health.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 一次暂时性失败。返回累计的连续失败次数（调用方据此算退避）。
    pub(crate) fn note_failure(&self, code: &'static str) -> u32 {
        let failures = {
            let mut h = self.health();
            h.note_failure(code);
            h.consecutive_failures
        };
        self.publish_runtime();
        failures
    }

    /// 一次成功读取。返回 true 表示这是一次「从失败中恢复」。
    pub(crate) fn note_success(&self) -> bool {
        let recovered = self.health().note_success();
        if recovered {
            self.publish_runtime();
        }
        recovered
    }

    /// worker 因致命错误退出，supervisor 即将重建
    pub(crate) fn note_worker_exit(&self, code: &'static str) {
        self.health().note_worker_exit(code);
        self.publish_runtime();
    }

    pub(crate) fn note_worker_restart(&self) {
        self.health().note_worker_restart();
        self.publish_runtime();
    }

    pub(crate) fn note_worker_started(&self, is_restart: bool) {
        self.health().note_worker_started(is_restart);
        self.publish_runtime();
    }

    /// 把健康快照交给运行时层重新评估并广播（若状态确有变化）。
    fn publish_runtime(&self) {
        if let Some(rt) = self.runtime.get() {
            rt.publish();
        }
    }

    /// 绑定运行时层。由 `start_monitor` 在构造完 RuntimeStatus 之后调用 ——
    /// 二者互相引用，只能分两步接上。
    pub fn attach_runtime(&self, runtime: Arc<crate::runtime::RuntimeStatus>) {
        let _ = self.runtime.set(runtime);
    }

    /// 读取健康快照（供 tray / settings / 诊断页与运行时层使用）
    pub fn health_snapshot(&self) -> ClipboardHealth {
        self.health().clone()
    }

    pub fn health_state(&self) -> HealthState {
        self.health().state
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
