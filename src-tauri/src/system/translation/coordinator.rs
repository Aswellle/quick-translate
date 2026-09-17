// src-tauri/src/system/translation/coordinator.rs
// TranslationCoordinator —— 翻译请求的编排与代际闸门。
//
// 重构前 `translation_flow.rs` 的顺序是：
//
//     cancel_current_translation()   // 取锁、abort 旧任务、放锁
//     show_popup_loading().await     // ← await 点
//     spawn(do_translate())
//     current_translation = Some(task)   // 才安装句柄
//
// `cancel` 与 `install` 之间隔着若干次 await。两个 execute_at_position 交错时：
//
//     A: cancel（无句柄可取消）→ await
//     B: cancel（仍无句柄可取消）→ await
//     A: 安装句柄 A
//     B: 安装句柄 B   ← 覆盖掉 A 的句柄
//
// A 的任务从未被 abort，且句柄已丢失 —— A 的迟到结果可以覆盖 B。
// 计划第 6 节明令禁止的 `B result → A late result → A 覆盖 B` 正是这条路径。
//
// 只靠 abort() 修不掉它（abort 本身也是竞态的：结果可能在 abort 前就已 emit）。
// 本模块的答案是代际闸门：每个结果在回传前都必须证明「我仍是当前代」，
// 而这个证明与回传动作在同一把锁内完成。

use std::sync::Mutex;

use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::state::AppState;
use crate::system::translation_flow;
use crate::types::{now_unix_ms, TranslationRecord, TranslationResult};

use super::request::{RequestGeneration, RequestId, TranslationRequest};

struct Inner {
    generation: RequestGeneration,
    /// 在途任务的唯一取消入口。旧代码无条件覆盖它，是上面那条竞态的成因。
    in_flight: Option<JoinHandle<()>>,
}

/// 翻译请求协调器。
///
/// 所有方法都是**同步**的，锁绝不会跨越 await —— 这一点是刻意的：
/// 持锁跨越 await 会把网络等待变成全局串行点（计划第 58 节原则 2）。
pub struct TranslationCoordinator {
    inner: Mutex<Inner>,
}

impl TranslationCoordinator {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                generation: RequestGeneration::default(),
                in_flight: None,
            }),
        }
    }

    /// 锁中毒时取回内部数据继续用，而不是 unwrap panic。
    /// 一个线程在持锁期间 panic 不该让整条翻译链路永久瘫痪 ——
    /// 这正是本模块要消灭的故障模式。
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 开启一代新请求，并中止上一代的在途任务。返回新请求 id。
    pub fn supersede(&self) -> RequestId {
        let mut inner = self.lock();
        let id = inner.generation.begin();
        if let Some(handle) = inner.in_flight.take() {
            handle.abort();
        }
        id
    }

    /// 安装任务句柄。
    ///
    /// 返回 false 表示该请求在「开新代 → 建窗/定位」这段异步间隙里已被
    /// 更新的请求取代；此时刚 spawn 出来的任务**就地 abort 且不安装**。
    /// 重构前缺的正是这一步：旧代码无条件覆盖句柄。
    pub fn install(&self, id: RequestId, handle: JoinHandle<()>) -> bool {
        let mut inner = self.lock();
        if !inner.generation.is_current(id) {
            handle.abort();
            return false;
        }
        inner.in_flight = Some(handle);
        true
    }

    /// 仅当 `id` 仍是当前代时才执行 `f`（通常是把结果 emit 给 UI）。
    ///
    /// **闸门与 `f` 必须在同一把锁内完成。** 拆成「先 is_stale 判断、
    /// 再 emit」两步的话，另一个线程可以在缝隙里开启新请求，我们照样会把
    /// 已作废的结果推给 UI —— 那等于没修。
    ///
    /// `f` 里不能 await（本方法是同步的）。emit 是同步调用，符合要求。
    pub fn run_if_current(&self, id: RequestId, f: impl FnOnce()) -> bool {
        let _guard = self.lock();
        if !_guard.generation.is_current(id) {
            return false;
        }
        f();
        true
    }

    /// 当前代（供测试与诊断读取）
    pub fn current(&self) -> Option<RequestId> {
        self.lock().generation.current()
    }

    #[cfg(test)]
    pub(crate) fn has_in_flight(&self) -> bool {
        self.lock().in_flight.is_some()
    }
}

impl Default for TranslationCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// 在指定光标位置执行翻译（剪贴板监控的唯一入口）。
///
/// 名字与签名与重构前 `translation_flow::execute_at_position` 一致，
/// 因此调用方只需改模块路径。
pub async fn execute_at_position(app: &AppHandle, cursor_x: f64, cursor_y: f64, text: String) {
    let state = app.state::<AppState>();
    let coordinator = state.coordinator.clone();

    // 先开新一代：上一代的任务被就地 abort，其迟到结果也会因 id 不匹配被丢弃
    let id = coordinator.supersede();

    if text.trim().is_empty() {
        translation_flow::emit_error(app, "EMPTY_TEXT", "未检测到选中文本");
        return;
    }

    let target_lang = state
        .config
        .read()
        .await
        .get("target_lang")
        .unwrap_or_else(|| "zh".to_string());

    let request = TranslationRequest::new(id, text, target_lang, (cursor_x, cursor_y));

    let position = translation_flow::compute_popup_position_dpi(app, cursor_x, cursor_y);
    translation_flow::show_popup_loading(app, &position).await;

    let app_clone = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        run_translation(&app_clone, request).await;
    });

    if !coordinator.install(id, task) {
        tracing::info!(
            event = "translation_request_superseded",
            request_id = %id,
            "[coordinator] 请求在启动期间被更新的请求取代，已中止"
        );
    }
}

/// 执行一次翻译，并在每一次结果回传前过代际闸门。
///
/// 三条出口（成功 / 同语言 / 错误）**全部**要过闸门 —— 重构前三条都不设防。
async fn run_translation(app: &AppHandle, request: TranslationRequest) {
    let state = app.state::<AppState>();
    let coordinator = state.coordinator.clone();
    let id = request.id;
    let start_ms = now_unix_ms();

    tracing::info!(
        event = "translation_request_started",
        request_id = %id,
        len = request.text.len(),
        target_lang = %request.target_lang,
        "[coordinator] 开始翻译"
    );

    match state
        .translator
        .translate(&request.text, &request.target_lang)
        .await
    {
        Ok(mut result) => {
            result.truncated = request.truncated;
            result.duration_ms = (now_unix_ms() - start_ms) as u64;

            let emitted = coordinator.run_if_current(id, || {
                translation_flow::emit_result(app, &result);
            });

            if !emitted {
                log_stale_result(id);
                return;
            }

            tracing::info!(
                event = "translation_request_succeeded",
                request_id = %id,
                provider = %result.provider,
                duration_ms = result.duration_ms,
                "[coordinator] 翻译完成"
            );

            // 历史入队放在 emit 之后：它是 await，会开出新的交错窗口，
            // 不能让它插在闸门与 emit 之间。迟到结果也不会写进历史。
            enqueue_history(&state, &result, &request).await;
        }
        // 源语言与目标语言相同：显示原文而非报错，与重构前保持一致
        Err(AppError::SameLanguage { lang }) => {
            tracing::info!("[coordinator] 源语言与目标语言相同（{}），显示原文", lang);
            let original = TranslationResult {
                source_text: request.text.clone(),
                translated_text: request.text.clone(),
                detected_source_lang: lang,
                target_lang: request.target_lang.clone(),
                provider: "none".to_string(),
                duration_ms: 0,
                truncated: request.truncated,
            };

            if !coordinator.run_if_current(id, || {
                translation_flow::emit_result(app, &original);
            }) {
                log_stale_result(id);
            }
        }
        Err(e) => {
            tracing::error!(
                event = "translation_request_failed",
                request_id = %id,
                code = e.error_code(),
                "[coordinator] 翻译失败: {}",
                e
            );

            if !coordinator.run_if_current(id, || {
                translation_flow::emit_error(app, e.error_code(), &e.to_string());
            }) {
                log_stale_result(id);
            }
        }
    }
}

fn log_stale_result(id: RequestId) {
    tracing::info!(
        event = "translation_request_stale_result",
        request_id = %id,
        "[coordinator] 结果是迟到的（已被更新的请求取代），丢弃且不入历史"
    );
}

/// 历史写入与翻译主链路解耦：写失败只记录，绝不影响已经 emit 出去的结果。
/// （Phase 7 会把它换成有界队列 + 单 worker，此处保持既有行为。）
async fn enqueue_history(
    state: &AppState,
    result: &TranslationResult,
    request: &TranslationRequest,
) {
    let history = state.history.clone();
    let record = TranslationRecord::from_result(result, &request.text, &request.target_lang);
    let limit = state.config.read().await.cache_history_limit();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = history.insert(&record).await {
            tracing::error!("历史记录写入失败: {}", e);
        }
        if let Err(e) = history.enforce_limit(limit).await {
            tracing::error!("历史清理失败: {}", e);
        }
    });
}

/// 供 `run_translation` 读取历史条数上限
trait ConfigExt {
    fn cache_history_limit(&self) -> i64;
}

impl ConfigExt for crate::domain::config::ConfigService {
    fn cache_history_limit(&self) -> i64 {
        self.get("history_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(200)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    /// 计划第 36 节的核心验收：A 起 → B 起 → A 迟到 → 只有 B 可见。
    ///
    /// 在 coordinator 层面直接跑通，不需要 Tauri runtime —— 这正是把
    /// 代际判断与 emit 收进 `run_if_current` 的收益。
    #[test]
    fn late_result_from_a_superseded_request_is_discarded() {
        let c = TranslationCoordinator::new();
        let visible: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let a = c.supersede();
        let b = c.supersede();

        // A 的结果现在才回来
        let v = visible.clone();
        let ran_a = c.run_if_current(a, || v.lock().unwrap().push("A"));

        // B 的结果回来
        let v2 = visible.clone();
        let ran_b = c.run_if_current(b, || v2.lock().unwrap().push("B"));

        assert!(!ran_a, "已作废请求的结果必须被丢弃");
        assert!(ran_b, "当前请求的结果必须回传");
        assert_eq!(
            *visible.lock().unwrap(),
            vec!["B"],
            "最终可见的只能有 B —— 旧结果绝不能覆盖新结果"
        );
    }

    #[test]
    fn run_if_current_runs_for_the_live_request() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();
        let mut ran = false;
        assert!(c.run_if_current(a, || ran = true));
        assert!(ran);
    }

    #[test]
    fn supersede_aborts_the_previous_task() {
        let c = TranslationCoordinator::new();
        let finished = Arc::new(AtomicBool::new(false));

        let a = c.supersede();
        let flag = finished.clone();
        let handle = tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
        });
        assert!(c.install(a, handle));
        assert!(c.has_in_flight());

        let _b = c.supersede();

        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !finished.load(Ordering::SeqCst),
            "被取代的请求必须真的被中止，而不是继续跑完"
        );
    }

    /// 重构前那条竞态的直接回归测试：请求在「开新代 → 安装句柄」的
    /// 异步间隙里被取代时，句柄不得覆盖新一代，且刚 spawn 的任务必须被中止。
    #[test]
    fn install_rejects_and_aborts_a_superseded_request() {
        let c = TranslationCoordinator::new();
        let finished = Arc::new(AtomicBool::new(false));

        let a = c.supersede();
        let b = c.supersede(); // A 在建窗期间被取代

        let flag = finished.clone();
        let handle = tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
        });

        assert!(!c.install(a, handle), "已作废的请求不该被安装");
        assert!(!c.has_in_flight(), "被拒绝的句柄不该留在 in_flight 里");
        assert_eq!(c.current(), Some(b), "当前代不应该被旧请求的安装动作改动");

        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !finished.load(Ordering::SeqCst),
            "被拒绝的句柄必须已就地 abort"
        );
    }

    /// 首次请求没有上一代可取消，不应 panic
    #[test]
    fn supersede_on_empty_coordinator_is_safe() {
        let c = TranslationCoordinator::new();
        assert_eq!(c.current(), None);
        let a = c.supersede();
        assert_eq!(c.current(), Some(a));
        assert!(!c.has_in_flight());
    }

    /// 连续突发：只有最后一代能回传，前面的全部丢弃
    #[test]
    fn burst_of_requests_leaves_only_the_last_visible() {
        let c = TranslationCoordinator::new();
        let visible: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

        let ids: Vec<_> = (0..8).map(|_| c.supersede()).collect();

        for (idx, id) in ids.iter().enumerate() {
            let v = visible.clone();
            c.run_if_current(*id, || v.lock().unwrap().push(idx));
        }

        assert_eq!(
            *visible.lock().unwrap(),
            vec![7],
            "8 次突发复制后只应显示最后一次的结果"
        );
    }

    /// 闸门为 false 时 f 绝不能被执行（副作用不能泄漏）
    #[test]
    fn stale_gate_does_not_run_the_side_effect() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();
        let _b = c.supersede();

        let side_effect = Arc::new(AtomicBool::new(false));
        let flag = side_effect.clone();
        let ran = c.run_if_current(a, || flag.store(true, Ordering::SeqCst));

        assert!(!ran);
        assert!(
            !side_effect.load(Ordering::SeqCst),
            "过期请求绝不能产生副作用"
        );
    }
}
