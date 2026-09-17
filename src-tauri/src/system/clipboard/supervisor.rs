// src-tauri/src/system/clipboard/supervisor.rs
// ClipboardSupervisor —— 保证「剪贴板句柄坏掉」不会变成「监控永久消失」。
//
// 重构前的故障链（Phase 2 要修的东西）：
//
//     arboard::Clipboard::new() 失败
//              ↓
//     监控线程 return
//              ↓
//     应用仍在运行，但没有任何人观察到线程已经死了
//              ↓
//     用户此后复制任何东西都不会有翻译，日志里只有一行 error
//
// 现在：worker 因致命错误返回 → supervisor 观察到 → 退避（250ms→30s，±20% 抖动）
// → 重建句柄 → 重开 worker。重建次数进入健康快照，Tray / 诊断页可读。

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rand::Rng;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

use super::backend::{ArboardBackend, ClipboardBackend, ClipboardBackendError};
use crate::util::jitter::with_jitter;

use super::backoff::{restart_backoff, RESTART_BACKOFF_SCHEDULE};
use super::controller::MonitorController;
use super::worker::{self, TranslationSink, WorkerExit, WorkerTiming};

/// 生产 sink：把检测到的文本交给 translation_flow。
///
/// 内容与重构前的 `trigger_translation()` 逐字一致 —— onboarding 前置检查、
/// 捕获光标位置、委托 translation_flow，一样都不能少。区别只是它现在挂在
/// worker 的出口 trait 上，使 worker 本身不再需要 AppHandle。
struct TauriSink {
    app: AppHandle,
}

impl TranslationSink for TauriSink {
    fn emit(&self, text: &str) {
        let app = self.app.clone();
        let text = text.to_string();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();

            // 向导未完成时，不弹出翻译浮窗
            if !state.is_onboarding_complete().await {
                tracing::info!(
                    "[clipboard] 翻译被跳过（onboarding 未完成），下次复制相同文本将重新触发"
                );
                return;
            }

            if text.trim().is_empty() {
                return;
            }

            let (cx, cy) = super::get_cursor_position();
            crate::system::translation::execute_at_position(&app, cx, cy, text).await;
        });
    }
}

/// 建后端的工厂。生产是 `ArboardBackend::new`，测试是脚本化 mock。
pub(crate) type BackendFactory =
    Box<dyn Fn() -> Result<Box<dyn ClipboardBackend>, ClipboardBackendError> + Send>;

/// supervisor 的可注入依赖。
///
/// 生产与测试走**同一条代码路径**，区别只在字段实现 —— 这样计划第 34.1 节
/// 那条「初始化失败 → worker 退出 → supervisor 重启 → 恢复」的故障链才能在
/// 单元测试里被真实执行，而不是只能靠在 Windows 机器上人肉复现。
pub(crate) struct SupervisorDeps {
    pub make_backend: BackendFactory,
    pub sink: Box<dyn TranslationSink>,
    pub worker_timing: WorkerTiming,
    /// 重启退避序列。生产用 `RESTART_BACKOFF_SCHEDULE`；测试用毫秒级值，
    /// 否则单条测试要等满 1.25 秒的 250ms+1s。
    pub restart_schedule: Vec<Duration>,
}

/// supervisor 主循环：建句柄 → 跑 worker → 观察退出 → 退避 → 重建。
/// 只有收到停机请求才返回。
pub(crate) fn run_supervisor<R: Rng>(
    controller: Arc<MonitorController>,
    deps: SupervisorDeps,
    rng: &mut R,
) {
    let mut attempt: u32 = 0;

    loop {
        if controller.is_shutdown_requested() {
            tracing::info!("[clipboard] supervisor 收到停机请求，退出");
            return;
        }

        // ── 建句柄。失败在这里就被接住 —— 这正是重构前缺失的一环 ──
        let backend = match (deps.make_backend)() {
            Ok(backend) => {
                let is_restart = attempt > 0;
                controller.health_mut().note_worker_started(is_restart);
                if is_restart {
                    tracing::info!(
                        event = "clipboard_worker_recovered",
                        restarts = attempt,
                        "[clipboard] worker 已重建，恢复读取"
                    );
                } else {
                    tracing::info!(
                        event = "clipboard_worker_started",
                        "[clipboard] worker 已启动"
                    );
                }
                attempt = 0;
                backend
            }
            Err(e) => {
                let delay = announce_restart(&controller, &e, attempt, &deps.restart_schedule, rng);
                if !sleep_interruptible(&controller, delay) {
                    return;
                }
                attempt = attempt.saturating_add(1);
                continue;
            }
        };

        // worker 是阻塞轮询循环，直接在本线程跑；它返回即代表需要重建。
        //
        // 刻意**不用** catch_unwind 兜 panic：Cargo.toml 的 release profile 设了
        // `panic = "abort"`，发布版本里 panic 会直接终止进程，catch_unwind 根本
        // 拦不住 —— 留着它只会制造「已经兜住了」的错觉。真正的防线是 worker
        // 不对可失败的 I/O 做 unwrap/expect。
        match worker::run_worker(backend, &controller, deps.sink.as_ref(), deps.worker_timing) {
            WorkerExit::Shutdown => {
                tracing::info!("[clipboard] worker 正常停机，supervisor 退出");
                return;
            }
            WorkerExit::Fatal(e) => {
                let delay = announce_restart(&controller, &e, attempt, &deps.restart_schedule, rng);
                if !sleep_interruptible(&controller, delay) {
                    return;
                }
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

/// 记录一次重启（健康快照 + 结构化日志），返回**实际将要休眠**的时长。
///
/// 时长在这里只抽样一次并同时用于日志与休眠 —— 分别抽样会让日志里写的
/// 延迟和真正 sleep 的对不上，排查退避问题时直接被误导。
fn announce_restart(
    controller: &MonitorController,
    error: &ClipboardBackendError,
    attempt: u32,
    schedule: &[Duration],
    rng: &mut impl Rng,
) -> Duration {
    let delay = with_jitter(restart_backoff(attempt, schedule), rng);
    {
        let mut h = controller.health_mut();
        h.note_worker_exit(error.code);
        h.note_worker_restart();
    }
    tracing::error!(
        event = "clipboard_worker_restart_scheduled",
        code = error.code,
        attempt,
        delay_ms = delay.as_millis() as u64,
        "[clipboard] worker 需要重建，{:?} 后重试: {}",
        delay,
        error.message
    );
    delay
}

/// 可被打断的休眠。
///
/// 分片睡的理由：退避上限是 30s，进程退出不该等满一整个退避窗口。
/// 返回 false 表示期间收到了停机请求。
fn sleep_interruptible(controller: &MonitorController, total: Duration) -> bool {
    const SLICE: Duration = Duration::from_millis(50);
    let mut left = total;
    while !left.is_zero() {
        if controller.is_shutdown_requested() {
            return false;
        }
        let slice = left.min(SLICE);
        thread::sleep(slice);
        left -= slice;
    }
    !controller.is_shutdown_requested()
}

/// 启动剪贴板监控（在 lib.rs setup 中调用）。
///
/// 返回的 controller 与内部 supervisor 共享同一份状态（字段全为 Arc），
/// 因此外部调用 suspend/resume/reset_last_text 会立刻生效。
/// 签名与重构前一致，调用方无需改动。
pub fn start_monitor(app: AppHandle) -> MonitorController {
    let controller = Arc::new(MonitorController::new());
    let supervisor_controller = controller.clone();

    std::thread::spawn(move || {
        let mut rng = rand::thread_rng();
        let deps = SupervisorDeps {
            make_backend: Box::new(|| {
                ArboardBackend::new().map(|b| Box::new(b) as Box<dyn ClipboardBackend>)
            }),
            sink: Box::new(TauriSink { app }),
            worker_timing: WorkerTiming::default(),
            restart_schedule: RESTART_BACKOFF_SCHEDULE.to_vec(),
        };
        run_supervisor(supervisor_controller, deps, &mut rng);
    });

    tracing::info!("[start_monitor] supervisor 已启动，initial suspended=false");
    (*controller).clone()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;
    use crate::system::clipboard::backend::mock::{MockClipboardBackend, Step};
    use crate::system::clipboard::health::HealthState;
    use crate::system::clipboard::worker::RecordingSink;

    fn fast_schedule() -> Vec<Duration> {
        vec![Duration::from_millis(1); 5]
    }

    /// 计划 34.1：模拟 `Clipboard::new()` 失败，验证 supervisor 会重启并恢复。
    /// 这是 Phase 2 的核心验收项 —— 重构前这条链的终点是「监控永久死亡」。
    #[test]
    fn supervisor_rebuilds_after_init_failure_and_recovers() {
        let controller = Arc::new(MonitorController::new());
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();

        let deps = SupervisorDeps {
            make_backend: Box::new(move || {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(ClipboardBackendError::fatal(
                        "CLIPBOARD_INIT_FAILED",
                        "mock: 模拟剪贴板句柄创建失败",
                    ))
                } else {
                    Ok(
                        Box::new(MockClipboardBackend::new(vec![Step::Text("hello")]))
                            as Box<dyn ClipboardBackend>,
                    )
                }
            }),
            sink: Box::new(RecordingSink::new(controller.clone(), 1)),
            worker_timing: WorkerTiming::fast(),
            restart_schedule: fast_schedule(),
        };

        let mut rng = StdRng::seed_from_u64(9);
        run_supervisor(controller.clone(), deps, &mut rng);

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3,
            "应当先失败两次、第三次成功建起句柄"
        );

        let h = controller.health_snapshot();
        assert_eq!(h.worker_restarts, 2, "两次初始化失败必须被记成两次重建");
        assert_eq!(h.state, HealthState::Healthy, "重建成功后必须回到 Healthy");
        assert_eq!(h.last_error_code, Some("CLIPBOARD_INIT_FAILED"));
        assert!(h.last_recovery_at_ms.is_some(), "重建成功必须记录恢复时刻");
    }

    /// worker 在运行中因致命错误退出 → supervisor 同样重建（不是只有启动路径受保护）
    #[test]
    fn supervisor_rebuilds_after_worker_fatal_exit() {
        let controller = Arc::new(MonitorController::new());
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();

        let sink = RecordingSink::new(controller.clone(), 1);
        let log = sink.log();

        let deps = SupervisorDeps {
            make_backend: Box::new(move || {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    // 第一次建得起来，但一读就致命失败
                    Ok(Box::new(MockClipboardBackend::new(vec![Step::Fatal(
                        "CLIPBOARD_NOT_SUPPORTED",
                    )])) as Box<dyn ClipboardBackend>)
                } else {
                    Ok(
                        Box::new(MockClipboardBackend::new(vec![Step::Text("recovered")]))
                            as Box<dyn ClipboardBackend>,
                    )
                }
            }),
            sink: Box::new(sink),
            worker_timing: WorkerTiming::fast(),
            restart_schedule: fast_schedule(),
        };

        let mut rng = StdRng::seed_from_u64(3);
        run_supervisor(controller.clone(), deps, &mut rng);

        assert_eq!(attempts.load(Ordering::SeqCst), 2, "应当重建一次");
        assert_eq!(controller.health_snapshot().worker_restarts, 1);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["recovered"],
            "重建后必须真正恢复翻译能力"
        );
    }

    /// 停机请求必须能让 supervisor 退出，哪怕正卡在退避里
    #[test]
    fn supervisor_exits_promptly_on_shutdown() {
        let controller = Arc::new(MonitorController::new());
        let deps = SupervisorDeps {
            make_backend: Box::new(|| {
                Err(ClipboardBackendError::fatal(
                    "CLIPBOARD_INIT_FAILED",
                    "mock: 永远失败",
                ))
            }),
            sink: Box::new(RecordingSink::never_stops(controller.clone())),
            worker_timing: WorkerTiming::fast(),
            // 故意给一个很长的退避，验证可被打断而不是傻等
            restart_schedule: vec![Duration::from_secs(10)],
        };

        let c2 = controller.clone();
        let handle = thread::spawn(move || {
            let mut rng = StdRng::seed_from_u64(1);
            run_supervisor(c2, deps, &mut rng);
        });

        // 等它进入退避，再请求停机
        thread::sleep(Duration::from_millis(30));
        let started = std::time::Instant::now();
        controller.request_shutdown();
        handle.join().unwrap();

        assert!(
            started.elapsed() < Duration::from_secs(2),
            "停机不该等满 10s 的退避窗口，实际耗时 {:?}",
            started.elapsed()
        );
    }

    /// 持续失败时重建次数要收敛在退避曲线上，而不是忙等刷爆日志
    #[test]
    fn repeated_failures_back_off_instead_of_spinning() {
        let controller = Arc::new(MonitorController::new());
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = attempts.clone();

        let deps = SupervisorDeps {
            make_backend: Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(ClipboardBackendError::fatal(
                    "CLIPBOARD_INIT_FAILED",
                    "常失败",
                ))
            }),
            sink: Box::new(RecordingSink::never_stops(controller.clone())),
            worker_timing: WorkerTiming::fast(),
            restart_schedule: vec![
                Duration::from_millis(5),
                Duration::from_millis(20),
                Duration::from_millis(60),
            ],
        };

        let c2 = controller.clone();
        let handle = thread::spawn(move || {
            let mut rng = StdRng::seed_from_u64(2);
            run_supervisor(c2, deps, &mut rng);
        });

        thread::sleep(Duration::from_millis(200));
        controller.request_shutdown();
        handle.join().unwrap();

        let n = attempts.load(Ordering::SeqCst);
        // 200ms 内，5+20+60+60+60… 的节奏最多重建个位数次。
        // 若退避失效会变成成百上千次 —— 那正是「无限 retry」这条红线。
        assert!(n <= 12, "退避未生效，200ms 内重建了 {} 次（疑似忙等）", n);
        assert!(n >= 2, "至少应尝试重建一次，实际 {}", n);
    }
}
