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

/// app 内写入的期望登记：内容哈希 + 基线变化代数 + 登记时刻。
///
/// 演进（QT-013）：F4 时代的「哈希 + 时刻」只能判断「读到的内容是否相同」，
/// 无法证明这次内容变化是**应用写入**而非用户动作 —— 用户点「复制原文」
/// 时剪贴板里躺着的就是同一段原文：worker 在写入落地前先读到相同文本
/// （哈希命中）把期望消费掉，随后真正的写入让变化代数前进，同文本被
/// 误判成「用户重新复制」，同一段内容被再次翻译弹出。
///
/// 绑定基线后语义闭环：登记时刻记下剪贴板变化代数，**只有代数前进且
/// 文本匹配**才认定写入落地并消费。落地前的同文本轮询（代数 == 基线）
/// 不消费也不触发；平台无变化代数时退化为纯哈希匹配（显式降级，
/// 与剪贴板监控其余路径的降级策略一致）。
#[derive(Clone, Copy)]
struct ExpectedWrite {
    text_hash: u64,
    baseline_seq: Option<u32>,
    at: Instant,
}

/// 期望登记的存活时长：超过即视为写入已被覆盖，丢弃
const EXPECT_TTL: Duration = Duration::from_secs(3);

fn hash_text(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// 登记的写入是否已经落地：代数从基线前进即落地。
///
/// - (基线 None, 任意) → 平台无变化代数，退化为纯哈希匹配（显式降级）；
/// - (基线 Some, 代数 None) → 读不到代数，保守视为未落地；
/// - (Some(b), Some(c)) → c != b 即落地（写入必然推进代数）。
fn write_has_landed(baseline_seq: Option<u32>, current_seq: Option<u32>) -> bool {
    match (baseline_seq, current_seq) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(b), Some(c)) => c != b,
    }
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
    ///
    /// `baseline_seq` 是登记时刻的剪贴板变化代数（调用方在写入**之前**
    /// 读取平台序列号传入；平台不提供时传 None）。哈希取自 normalize_text
    /// 后的文本 —— Windows 剪贴板往返可能改写换行（LF↔CRLF），原文哈希
    /// 会永远匹配不上导致吸收失效；两侧统一归一化后比较即可免疫。
    pub fn mark_app_write(&self, text: &str, baseline_seq: Option<u32>) {
        let mut slot = self
            .expected_write
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *slot = Some(ExpectedWrite {
            text_hash: hash_text(&super::normalize_text(text)),
            baseline_seq,
            at: Instant::now(),
        });
        tracing::info!(
            "[MonitorController] mark_app_write() 已登记期望（baseline={:?}）",
            baseline_seq
        );
    }

    /// 撤销登记：写剪贴板失败时调用，避免残留期望（F5）。
    pub fn unmark_app_write(&self) {
        *self
            .expected_write
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        tracing::info!("[MonitorController] unmark_app_write() 已撤销（写入失败）");
    }

    /// worker 读不到剪贴板内容时保活期望登记。
    ///
    /// EXPECT_TTL 的前提是「worker 每轮都有机会读到内容去匹配」；读取本身
    /// 失败（剪贴板被其他进程锁住）时这个前提不成立 —— 读取退避最长可到
    /// 30s，远超 3s TTL，期望会在 worker 有机会匹配之前就被过期清理，
    /// app 自己写入的内容随后被当成用户新复制送去翻译。读取失败期间
    /// 由 worker 调用本方法刷新登记时刻，TTL 恢复为「自上次可匹配的轮次起算」。
    pub(crate) fn touch_expected_write(&self) {
        let mut slot = self
            .expected_write
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(exp) = slot.as_mut() {
            exp.at = Instant::now();
        }
    }

    /// worker 调用：当前内容（已归一化）是否匹配已登记的 app 写入。
    ///
    /// `current_seq` 是本轮读取的剪贴板变化代数。匹配 = 文本哈希一致
    /// **且**代数已从基线前进（写入真正落地）→ 消费登记并返回 true；
    /// 文本相同但代数未前进 → 写入尚未落地，保留登记、返回 false
    /// （此时同文本同代数也不该触发翻译，由 is_new_content 判定）。
    /// 超过 EXPECT_TTL 丢弃（写入已被用户复制覆盖，期望永远无法匹配）。
    pub(crate) fn consume_if_expected(
        &self,
        current_normalized: &str,
        current_seq: Option<u32>,
    ) -> bool {
        let mut slot = self
            .expected_write
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match *slot {
            Some(exp)
                if exp.text_hash == hash_text(current_normalized)
                    && write_has_landed(exp.baseline_seq, current_seq) =>
            {
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

    /// 请求 worker 在下一轮循环吸收当前剪贴板内容（不触发翻译）。
    ///
    /// 两个触发方，语义相同：
    /// - `resume()`：暂停不是用户动作，暂停期间复制的旧内容恢复后不该弹窗；
    /// - supervisor 重建 worker：句柄重建也不是用户动作，剪贴板里躺着的
    ///   旧内容不该因为重启而被当成「新复制」重新弹窗（计划第 5 节）。
    pub fn request_absorb(&self) {
        self.absorb_pending.store(true, Ordering::SeqCst);
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

    /// 期望登记是否仍在（未被消费或超时丢弃）。仅供测试断言（QT-013 时序）。
    #[cfg(test)]
    pub(crate) fn expectation_pending_for_test(&self) -> bool {
        self.expected_write
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
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
        c.mark_app_write("hello\r\nworld", None);
        assert!(
            c.consume_if_expected("hello world", None),
            "两侧都归一化后应当匹配"
        );
    }

    #[test]
    fn expected_write_survives_until_consumed_then_is_gone() {
        let c = MonitorController::new();
        c.mark_app_write("payload", None);
        assert!(
            !c.consume_if_expected("something else", None),
            "不匹配时不得消费登记"
        );
        assert!(c.consume_if_expected("payload", None), "匹配时消费登记");
        assert!(
            !c.consume_if_expected("payload", None),
            "登记已被消费，不该二次命中"
        );
    }

    // ── QT-013：期望登记必须绑定变化代数 ─────────────────────────────────

    /// 核心回归：文本相同但代数未从基线前进 = 写入尚未落地，不得消费。
    /// 旧实现只看哈希 —— 用户点「复制原文」而剪贴板已是同一段原文时，
    /// worker 在写入落地前就把期望消费掉，随后的真实写入被误判成
    /// 「用户重新复制」而再次翻译。
    #[test]
    fn expectation_with_unchanged_seq_is_not_consumed() {
        let c = MonitorController::new();
        c.mark_app_write("same text", Some(5));

        // 写入落地前：同文本、同代数 —— 不消费
        assert!(
            !c.consume_if_expected("same text", Some(5)),
            "代数未前进不得认定写入落地"
        );
        // 落地后：代数前进 → 消费
        assert!(c.consume_if_expected("same text", Some(6)));
    }

    /// 有基线但读不到代数：保守不消费（宁可多等一轮，不可吞掉用户复制）。
    #[test]
    fn expectation_with_unreadable_seq_is_not_consumed() {
        let c = MonitorController::new();
        c.mark_app_write("payload", Some(5));
        assert!(!c.consume_if_expected("payload", None));
    }

    /// 平台无变化代数（baseline None）：退化为纯哈希匹配（显式降级语义）。
    #[test]
    fn expectation_without_platform_seq_falls_back_to_hash() {
        let c = MonitorController::new();
        c.mark_app_write("payload", None);
        assert!(c.consume_if_expected("payload", None));
    }

    /// 代数回退/异常（current < baseline）同样视为落地：代数比较用不等而非
    /// 大于 —— u32 回绕在真实时间尺度上不可达，不等于基线即变化。
    #[test]
    fn expectation_consumes_on_any_seq_change() {
        let c = MonitorController::new();
        c.mark_app_write("payload", Some(u32::MAX));
        assert!(
            c.consume_if_expected("payload", Some(0)),
            "代数回绕视为变化"
        );
    }

    /// touch_expected_write：保活不得清掉登记，空槽调用不得 panic。
    /// （TTL 真实过期依赖 3s 墙钟，这里锁定「touch 是无害刷新」这一契约；
    /// 「长退避下期望存活更久」由 worker 侧在读取失败时调用它保证。）
    #[test]
    fn touch_keeps_pending_expectation_and_is_safe_when_empty() {
        let c = MonitorController::new();
        c.touch_expected_write();

        c.mark_app_write("payload", None);
        c.touch_expected_write();
        assert!(c.expectation_pending_for_test(), "保活刷新不得清掉期望登记");
        assert!(!c.consume_if_expected("other", None));
        assert!(c.consume_if_expected("payload", None), "保活后仍可正常匹配");
    }

    #[test]
    fn unmark_cancels_pending_expectation() {
        let c = MonitorController::new();
        c.mark_app_write("payload", None);
        c.unmark_app_write();
        assert!(
            !c.consume_if_expected("payload", None),
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
