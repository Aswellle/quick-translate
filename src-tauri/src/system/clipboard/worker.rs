// src-tauri/src/system/clipboard/worker.rs
// 剪贴板轮询 worker。
//
// 自 system/clipboard_monitor.rs 的 clipboard_monitor_thread() 平移，轮询、
// 防抖、app 写入吸收、序列号判定的语义**逐字保留** —— 那些行为背后都有
// 已修复的 bug（F4 / F5 / F8 / C6），不是可以顺手重构的东西。
//
// 与重构前的两点差别：
//   1. 启动句柄由 supervisor 传入（backend trait），失败不再就地 return，
//      而是由 supervisor 观察到并重建；
//   2. 暂时性读取失败会退避重试并计入健康快照，而不再是全静默的 `Err(_) => continue`。

use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use super::backend::{ClipboardBackend, ClipboardErrorKind, ClipboardRead};
use super::backoff::transient_backoff;
use super::controller::MonitorController;

/// worker 的可调时序。生产用 Default，测试用毫秒级值以便秒级跑完。
#[derive(Debug, Clone, Copy)]
pub struct WorkerTiming {
    pub poll_interval: Duration,
    pub debounce_delay: Duration,
    pub transient_backoff_initial: Duration,
    pub transient_backoff_max: Duration,
}

impl Default for WorkerTiming {
    fn default() -> Self {
        Self {
            // 与重构前一致：200ms 轮询 / 300ms 防抖
            poll_interval: Duration::from_millis(200),
            debounce_delay: Duration::from_millis(300),
            transient_backoff_initial: Duration::from_millis(200),
            transient_backoff_max: Duration::from_secs(30),
        }
    }
}

/// 触发翻译的出口。
///
/// 抽出 trait 是为了让 worker 不必持有 AppHandle —— 生产实现包 AppHandle
/// 并在其中做 onboarding 检查与异步派发，测试实现只记录调用。没有这一层，
/// worker 的故障路径就无法在没有 Tauri runtime 的情况下被测试（计划第 34 节）。
pub trait TranslationSink: Send {
    fn emit(&self, text: &str);
}

/// worker 的退出原因。仅在收到停机请求或致命错误时返回 —— 暂时性失败不会退出。
#[derive(Debug)]
pub enum WorkerExit {
    /// 收到停机请求（进程退出，或测试场景结束）
    Shutdown,
    /// 致命错误：句柄已不可用，必须由 supervisor 重建后再试
    Fatal(super::backend::ClipboardBackendError),
}

/// 吸收当前剪贴板内容，返回 (要记为 last_text 的内容, 当时的序列号)。
///
/// 语义与 F8 修复后的 hide_popup 路径一致，不要「顺手简化」：
///   序列号可用   → last_text = 当前内容（同文本 + 同序列号不再触发）
///   序列号不可用 → last_text = None（保住 reset_last_text 承诺的
///                  「重复复制仍能触发」，代价是浮窗可能因残留内容再次弹出）
fn absorb_current(backend: &mut dyn ClipboardBackend) -> (Option<String>, Option<u32>) {
    let seq = backend.seq();
    let text = if seq.is_some() {
        match backend.get_text() {
            Ok(ClipboardRead::Text(t)) => Some(t),
            // 读失败或内容为空时置 None：与重构前 `get_text().ok()` 的行为一致
            _ => None,
        }
    } else {
        None
    };
    (text, seq)
}

/// worker 主循环。除非收到停机请求或命中致命错误，否则永不返回。
pub fn run_worker(
    mut backend: Box<dyn ClipboardBackend>,
    controller: &MonitorController,
    sink: &dyn TranslationSink,
    timing: WorkerTiming,
) -> WorkerExit {
    let mut last_text: Option<String> = None;
    // 防抖挂起项：(归一化文本, 入队时刻)。合并为单元组后，
    // 各分支的重置从两行收敛为一次赋值（C6）
    let mut pending: Option<(String, Instant)> = None;
    // 上次判定时的剪贴板序列号，用于识别「同一段文本被重新复制」。
    // None = 平台不提供序列号，走 seq 不可用时的降级策略（见 is_new 判定）
    let mut last_seq: Option<u32> = backend.seq();

    loop {
        if controller.is_shutdown_requested() {
            tracing::info!("[clipboard] worker 收到停机请求，退出");
            return WorkerExit::Shutdown;
        }

        // 暂停时等待，不消耗 CPU。
        // 暂停不杀 worker、不重建句柄 —— 恢复时直接继续用同一个 backend。
        if controller.is_suspended() {
            thread::sleep(timing.poll_interval);
            pending = None;
            continue;
        }

        // 处理 hide_popup 发出的重置请求。
        //
        // 此前这里把 last_text 置为 None，意图是「关闭后再次复制相同文本仍能触发」。
        // 但剪贴板里那段文本并没有消失，下一轮轮询就把它当成全新内容，防抖到期后
        // 立刻重新翻译 —— 浮窗关掉约 1 秒又自己弹回来，红色按钮/空格键看起来失效。
        //
        // 改为「吸收」当前剪贴板内容并记录剪贴板序列号：
        //   同文本 + 同序列号 → 原地未动，不触发（浮窗保持关闭）
        //   同文本 + 序列号变化 → 用户真的重新复制了一次，正常触发
        if controller.reset_requested.swap(false, Ordering::SeqCst) {
            let (absorbed, seq) = absorb_current(backend.as_mut());
            last_seq = seq;
            last_text = absorbed;
            pending = None;
            tracing::info!(
                "[clipboard] hide_popup 触发：seq={:?} absorbed={}",
                last_seq,
                last_text.is_some()
            );
        }

        // 从暂停恢复：吸收当前内容，避免翻译暂停期间用户复制的东西。
        // 与上面的 hide_popup 路径共用吸收语义，只是日志事件不同。
        if controller.take_absorb_request() {
            let (absorbed, seq) = absorb_current(backend.as_mut());
            last_seq = seq;
            last_text = absorbed;
            pending = None;
            tracing::info!(
                event = "clipboard_resume_absorbed",
                "[clipboard] resume 触发：seq={:?} absorbed={}",
                last_seq,
                last_text.is_some()
            );
        }

        thread::sleep(timing.poll_interval);

        // 暂停检测（避免在 sleep 期间被暂停导致丢失一轮检测）
        if controller.is_suspended() {
            pending = None;
            continue;
        }

        let current = match backend.get_text() {
            Ok(ClipboardRead::Text(t)) => {
                if controller.note_success() {
                    tracing::info!(
                        event = "clipboard_worker_recovered",
                        "[clipboard] 剪贴板读取已恢复"
                    );
                }
                t
            }
            // 剪贴板为空，或内容是图片等非文本格式 —— 这是正常状态，不是故障。
            // 不计入失败、不触发退避；pending 保持不动，与重构前
            // `Err(_) => continue` 的语义一致，避免「复制图片把监控打进退避」。
            Ok(ClipboardRead::Empty) => {
                controller.note_success();
                continue;
            }
            Err(e) => {
                let kind = e.kind;
                let code = e.code;
                let failures = controller.note_failure(code);

                if kind == ClipboardErrorKind::Fatal {
                    // 致命错误不在这里空转：退出，把重建句柄的职责交还 supervisor。
                    // 这正是重构前缺失的一环 —— 那时这里直接 return，没人观察。
                    controller.note_worker_exit(code);
                    tracing::error!(
                        event = "clipboard_worker_failed",
                        code,
                        "[clipboard] 剪贴板致命错误，worker 退出交由 supervisor 重建: {}",
                        e.message
                    );
                    return WorkerExit::Fatal(e);
                }

                let backoff = transient_backoff(
                    failures,
                    timing.transient_backoff_initial,
                    timing.transient_backoff_max,
                );
                tracing::warn!(
                    event = "clipboard_worker_failed",
                    code,
                    consecutive = failures,
                    backoff_ms = backoff.as_millis() as u64,
                    "[clipboard] 读取失败，退避后重试: {}",
                    e.message
                );
                thread::sleep(backoff);
                continue;
            }
        };

        let current_normalized = super::normalize_text(&current);

        // app 主动写入剪贴板（复制译文/原文）：内容哈希匹配才吸收（F4）。
        // 不匹配时登记保留 —— 写入可能尚未落地；用户抢先复制的新内容
        // 因哈希不匹配会正常走翻译流程，不再被误吞。
        if controller.consume_if_expected(&current_normalized) {
            tracing::info!(
                "[clipboard] 检测到 app 写入（哈希匹配），已吸收 len={}",
                current_normalized.len()
            );
            last_text = Some(current);
            // 同步序列号：app 的写入本身会让序列号自增，若不同步则下一轮
            // 会因「文本相同但序列号变化」被误判成用户重新复制
            last_seq = backend.seq();
            pending = None;
            continue;
        }

        // 跳过空白或极短内容
        if current_normalized.trim().len() < 2 {
            pending = None;
            continue;
        }

        // 检测是否是新内容。
        // 文本变化 → 新内容；文本相同但剪贴板序列号变化 → 用户重新复制了同一段文本，
        // 同样视为新内容（这是关闭浮窗后重复复制仍能触发翻译的依据）。
        //
        // 序列号不可用时（非 Windows）只能靠文本比较，「重新复制同一段文本」
        // 无法被识别 —— 但此时 hide_popup/resume 分支已清空 last_text，重复复制
        // 仍会因 last_text == None 而触发，承诺得以保住（F8）。
        let current_seq = backend.seq();
        let is_new = match &last_text {
            Some(prev) => {
                let prev_norm = super::normalize_text(prev);
                let text_changed = current_normalized != prev_norm;
                let recopied = match (current_seq, last_seq) {
                    (Some(cur), Some(last)) => cur != last,
                    // 序列号不可用：不做「重新复制」推断，避免恒真/恒假的误判
                    _ => false,
                };
                text_changed || recopied
            }
            None => true,
        };

        if is_new {
            last_text = Some(current);
            last_seq = current_seq;
            pending = Some((current_normalized, Instant::now()));
        } else if pending
            .as_ref()
            .is_some_and(|(_, start)| start.elapsed() >= timing.debounce_delay)
        {
            // 内容未变且防抖到期 → 触发翻译
            if let Some((text_clone, _)) = pending.take() {
                tracing::info!(
                    event = "translation_request_started",
                    len = text_clone.len(),
                    "[clipboard] 防抖到期，触发翻译"
                );
                sink.emit(&text_clone);
            }
        }
    }
}

#[cfg(test)]
impl WorkerTiming {
    /// 毫秒级时序，让单条测试在秒级内跑完完整的轮询 / 防抖 / 退避路径。
    pub(crate) fn fast() -> Self {
        Self {
            poll_interval: Duration::from_millis(1),
            debounce_delay: Duration::from_millis(2),
            transient_backoff_initial: Duration::from_millis(1),
            transient_backoff_max: Duration::from_millis(4),
        }
    }
}

/// 记录被触发的文本，供测试断言「到底有没有触发翻译」。
/// 达到 `stop_after` 条后请求停机，让 worker / supervisor 返回。
#[cfg(test)]
pub(crate) struct RecordingSink {
    /// Arc 包一层，让 sink 被装进 Box<dyn TranslationSink> 之后，
    /// 测试仍能持有句柄读回记录（supervisor 测试需要这个）。
    emitted: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    controller: std::sync::Arc<MonitorController>,
    stop_after: usize,
}

#[cfg(test)]
impl RecordingSink {
    pub(crate) fn new(controller: std::sync::Arc<MonitorController>, stop_after: usize) -> Self {
        Self {
            emitted: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            controller,
            stop_after,
        }
    }

    /// 不因触发而停机 —— 由测试自行决定何时结束。
    pub(crate) fn never_stops(controller: std::sync::Arc<MonitorController>) -> Self {
        Self::new(controller, usize::MAX)
    }

    /// 记录缓冲区句柄，可在 sink 被装箱后继续读取。
    pub(crate) fn log(&self) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
        self.emitted.clone()
    }

    pub(crate) fn emitted(&self) -> Vec<String> {
        self.emitted.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl TranslationSink for RecordingSink {
    fn emit(&self, text: &str) {
        let mut v = self.emitted.lock().unwrap();
        v.push(text.to_string());
        let done = v.len() >= self.stop_after;
        drop(v);
        if done {
            self.controller.request_shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::system::clipboard::backend::mock::{MockClipboardBackend, Step};
    use crate::system::clipboard::health::HealthState;

    /// 在后台线程里跑 worker，返回共享的 sink 以便事后断言。
    /// sink 留在外部，测试才能在 worker 结束后检查「到底有没有触发翻译」。
    fn spawn(
        backend: MockClipboardBackend,
        controller: &Arc<MonitorController>,
        stop_after: usize,
    ) -> (Arc<RecordingSink>, thread::JoinHandle<WorkerExit>) {
        let sink = Arc::new(RecordingSink::new(controller.clone(), stop_after));
        let sink_in_thread = sink.clone();
        let controller_in_thread = controller.clone();
        let handle = thread::spawn(move || {
            run_worker(
                Box::new(backend),
                &controller_in_thread,
                sink_in_thread.as_ref(),
                WorkerTiming::fast(),
            )
        });
        (sink, handle)
    }

    /// 在 `timeout` 内轮询等待条件成立，避免用固定 sleep 造成偶发失败。
    fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(2));
        }
        cond()
    }

    /// 跑 worker 直到 sink 收到 `stop_after` 条文本后自行停机。
    fn run(
        backend: MockClipboardBackend,
        stop_after: usize,
    ) -> (Vec<String>, Arc<MonitorController>) {
        let controller = Arc::new(MonitorController::new());
        let sink = RecordingSink::new(controller.clone(), stop_after);
        let exit = run_worker(Box::new(backend), &controller, &sink, WorkerTiming::fast());
        assert!(
            matches!(exit, WorkerExit::Shutdown),
            "测试应通过停机请求结束"
        );
        (sink.emitted(), controller)
    }

    // ── 计划 34.2：连续读取失败后自动恢复 ────────────────────────────────

    #[test]
    fn worker_survives_consecutive_transient_failures() {
        let backend = MockClipboardBackend::new(vec![
            Step::Transient("CLIPBOARD_OCCUPIED"),
            Step::Transient("CLIPBOARD_OCCUPIED"),
            Step::Transient("CLIPBOARD_OCCUPIED"),
            Step::Text("hello"),
        ]);

        let (emitted, controller) = run(backend, 1);

        assert_eq!(emitted, vec!["hello"], "连续失败后仍应恢复并翻译");
        let h = controller.health_snapshot();
        assert_eq!(h.state, HealthState::Healthy, "恢复后必须回到 Healthy");
        assert_eq!(h.consecutive_failures, 0);
        assert_eq!(h.last_error_code, Some("CLIPBOARD_OCCUPIED"));
        assert!(h.last_recovery_at_ms.is_some(), "必须记录恢复时刻");
    }

    #[test]
    fn worker_goes_degraded_while_failing() {
        let controller = Arc::new(MonitorController::new());
        // 持续失败（单步脚本会重复），健康状态应稳定停在 Degraded
        let backend = MockClipboardBackend::new(vec![Step::Transient("CLIPBOARD_OCCUPIED")]);
        let (sink, handle) = spawn(backend, &controller, usize::MAX);

        assert!(
            wait_until(Duration::from_secs(2), || controller
                .health_snapshot()
                .consecutive_failures
                >= 2),
            "worker 未在期限内记录失败"
        );
        assert_eq!(controller.health_state(), HealthState::Degraded);

        controller.request_shutdown();
        let _ = handle.join().unwrap();
        assert!(
            sink.emitted().is_empty(),
            "持续读失败期间绝不能产生翻译事件"
        );
    }

    // ── 致命错误必须退出，交给 supervisor 重建 ───────────────────────────

    #[test]
    fn fatal_error_exits_worker_with_reason() {
        let controller = Arc::new(MonitorController::new());
        let backend = MockClipboardBackend::new(vec![Step::Fatal("CLIPBOARD_NOT_SUPPORTED")]);
        let sink = RecordingSink::new(controller.clone(), usize::MAX);

        let exit = run_worker(Box::new(backend), &controller, &sink, WorkerTiming::fast());

        match exit {
            WorkerExit::Fatal(e) => assert_eq!(e.code, "CLIPBOARD_NOT_SUPPORTED"),
            other => panic!("致命错误应让 worker 退出，实际: {:?}", other),
        }
        assert_eq!(controller.health_state(), HealthState::Recovering);
    }

    // ── 空剪贴板不是故障 ─────────────────────────────────────────────────

    #[test]
    fn empty_clipboard_does_not_degrade_health() {
        let backend = MockClipboardBackend::new(vec![Step::Empty, Step::Empty, Step::Text("hi")]);
        let (emitted, controller) = run(backend, 1);

        assert_eq!(emitted, vec!["hi"]);
        let h = controller.health_snapshot();
        assert_eq!(h.state, HealthState::Healthy);
        assert_eq!(h.last_error_code, None, "空剪贴板不该被记成错误");
    }

    // ── 防抖：同一段文本只触发一次 ───────────────────────────────────────

    #[test]
    fn same_text_triggers_exactly_once() {
        let backend = MockClipboardBackend::new(vec![Step::Text("stable")]);
        let (emitted, _) = run(backend, 1);
        assert_eq!(emitted, vec!["stable"]);
    }

    #[test]
    fn empty_and_short_text_never_triggers() {
        // 单个字符被 trim().len() < 2 挡掉
        let controller = Arc::new(MonitorController::new());
        let backend = MockClipboardBackend::new(vec![Step::Text("x")]);
        let (sink, handle) = spawn(backend, &controller, usize::MAX);

        thread::sleep(Duration::from_millis(50));
        controller.request_shutdown();
        let _ = handle.join().unwrap();

        assert!(sink.emitted().is_empty(), "过短内容不该触发翻译");
    }

    // ── app 写入吸收（F4 / F5）───────────────────────────────────────────

    #[test]
    fn app_write_is_absorbed_and_does_not_trigger() {
        let controller = Arc::new(MonitorController::new());
        // 模拟「复制译文」按钮：app 先登记期望，再把内容写进剪贴板
        controller.mark_app_write("translated");
        let backend = MockClipboardBackend::new(vec![Step::Text("translated")]);
        let (sink, handle) = spawn(backend, &controller, usize::MAX);

        thread::sleep(Duration::from_millis(50));
        controller.request_shutdown();
        let _ = handle.join().unwrap();

        assert!(
            sink.emitted().is_empty(),
            "app 自己写入剪贴板绝不能触发二次翻译（F4）"
        );
    }

    // ── 暂停期间不产生翻译事件 ───────────────────────────────────────────

    #[test]
    fn suspended_worker_emits_nothing() {
        let controller = Arc::new(MonitorController::new());
        controller.suspend();
        let backend = MockClipboardBackend::new(vec![Step::Text("while paused")]);
        let (sink, handle) = spawn(backend, &controller, usize::MAX);

        thread::sleep(Duration::from_millis(50));
        controller.request_shutdown();
        let _ = handle.join().unwrap();

        assert!(sink.emitted().is_empty(), "暂停期间不得触发翻译");
    }

    // ── 恢复时吸收（Phase 2 新增：修「恢复后立刻弹窗」）──────────────────

    /// 覆盖启动路径：config 里 clipboard_monitor_enabled=false 时 lib.rs 会先
    /// 调 suspend()，用户在设置里一开启就 resume()。此时剪贴板里的旧内容
    /// 不该被立刻翻译。
    #[test]
    fn resume_absorbs_preexisting_clipboard_content() {
        let controller = Arc::new(MonitorController::new());
        controller.suspend();
        // 暂停期间剪贴板里已经躺着一段文字
        let backend = MockClipboardBackend::new(vec![Step::Text("copied while paused")]);
        let (sink, handle) = spawn(backend, &controller, usize::MAX);

        thread::sleep(Duration::from_millis(20));
        controller.resume();

        // 等到 worker 确实消费掉吸收请求为止，再给它足够时间「犯错」
        assert!(
            wait_until(Duration::from_secs(2), || !controller.absorb_is_pending()),
            "恢复请求应被 worker 消费"
        );
        thread::sleep(Duration::from_millis(60));

        controller.request_shutdown();
        let _ = handle.join().unwrap();

        assert!(
            sink.emitted().is_empty(),
            "恢复后不得因暂停期间已有的剪贴板内容弹窗翻译"
        );
    }

    /// 恢复之后，用户真正的新复制必须照常触发 —— 别把「吸收」做成永久静音。
    ///
    /// 注意本用例与 `resume_absorbs_preexisting_clipboard_content` 覆盖的是不同侧面：
    /// 那条断言「恢复后不弹旧内容」（负向），这条断言「恢复后仍能工作」（正向）。
    /// 两条都需要，因为只做吸收不做恢复检测的实现能骗过其中任意一条。
    #[test]
    fn real_copy_after_resume_still_triggers() {
        let controller = Arc::new(MonitorController::new());
        controller.suspend();
        let backend = MockClipboardBackend::new(vec![
            Step::Text("copied while paused"),
            Step::Text("brand new copy"),
        ]);
        let (sink, handle) = spawn(backend, &controller, 1);

        thread::sleep(Duration::from_millis(20));
        controller.resume();

        assert!(
            wait_until(Duration::from_secs(2), || !sink.emitted().is_empty()),
            "恢复后的新复制必须触发翻译"
        );
        let _ = handle.join().unwrap();

        assert_eq!(sink.emitted(), vec!["brand new copy"]);
    }
}
