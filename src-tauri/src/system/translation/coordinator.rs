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

use crate::domain::cache::{lookup_cached_result, should_fall_back_to_cache};
use crate::domain::clipboard_filter::{self, FilterDecision, SensitiveRule};
use crate::error::AppError;
use crate::state::AppState;
use crate::system::persistence::{OwnedCacheEntry, PersistJob};
use crate::system::translation_flow;
use crate::types::{ToastPayload, TranslationRecord, TranslationResult};

use super::request::{RequestGeneration, RequestId, TranslationRequest};

struct Inner {
    generation: RequestGeneration,
    /// 在途任务的唯一取消入口。旧代码无条件覆盖它，是上面那条竞态的成因。
    in_flight: Option<JoinHandle<()>>,
    /// 最近一次被敏感过滤拦截的内容及其光标锚点（「仍然翻译」手动覆盖的
    /// 数据源）。
    ///
    /// 只活在内存里：它是用户刚刚复制的东西，生命周期与那次复制意图一致
    /// —— 下一次复制（supersede）或手动覆盖取走后即清空。绝不落盘、
    /// 绝不进日志、绝不出现在任何事件里。
    last_filtered: Option<(String, (f64, f64))>,
    /// 最近一次**通过过滤并实际发起**的翻译原文（快捷键「重新翻译上一次」
    /// 的数据源）。只活在内存里，不落盘、不进日志；被拦截的内容不会记入。
    last_text: Option<String>,
}

/// 浮窗 show 流程的跨请求互斥锁。
///
/// 两个并发 `execute_at_position` 的窗口操作（set_position / show）若不串行，
/// `B.pos → A.pos → B.show → A.show` 这样的交错会把窗口永久留在**过期请求**的
/// 位置上。锁必须横跨「代际检查 → 窗口操作 → loading 提交」整段 —— 它是
/// async 锁，窗口操作要 await；但代际的判定与提交仍由同步的
/// `RequestGeneration` 闸门完成，互斥锁只负责把窗口操作的顺序定下来。
static POPUP_SHOW_OPS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 浮窗 show 流程的提交结果（QT-003 / QT-007 的契约核心）。
#[derive(Debug, PartialEq, Eq)]
pub enum ShowCommit {
    /// 窗口已显示，loading 事件已按当前代原子提交。
    Committed,
    /// 请求已过期：未发任何事件。窗口操作是否已执行取决于过期发生在
    /// 哪个阶段 —— 窗锁的串行保证**当前代请求的 show 随后必然执行**，
    /// 并把窗口位置带回当前代自己的锚点。
    Superseded,
    /// 窗口创建或显示失败（WebView 异常等）。
    Failed,
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
                last_filtered: None,
                last_text: None,
            }),
        }
    }

    /// 锁中毒时取回内部数据继续用，而不是 unwrap panic。
    /// 一个线程在持锁期间 panic 不该让整条翻译链路永久瘫痪 ——
    /// 这正是本模块要消灭的故障模式。
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 记住最近一次实际发起的翻译原文（只用于「重新翻译上一次」）。
    /// 被敏感过滤拦截的内容不会调用本方法。
    pub fn remember_text(&self, text: &str) {
        self.lock().last_text = Some(text.to_string());
    }

    /// 取最近一次翻译原文；None = 本次进程还没有成功发起过翻译。
    pub fn last_text(&self) -> Option<String> {
        self.lock().last_text.clone()
    }

    /// 开启一代新请求，并中止上一代的在途任务。返回新请求 id。
    ///
    /// 同时清空被拦截内容的暂存：用户复制了新东西，上一条被拦截内容的
    /// 「仍然翻译」入口就该消失 —— 手动覆盖只对「刚刚被拦的那条」有效。
    pub fn supersede(&self) -> RequestId {
        let mut inner = self.lock();
        let id = inner.generation.begin();
        inner.last_filtered = None;
        if let Some(handle) = inner.in_flight.take() {
            handle.abort();
        }
        id
    }

    /// 记录一次被过滤拦截的内容。
    ///
    /// 与 [`Self::install`] 同样的竞态约束：拦截判定发生在锁外，记录时刻
    /// 该请求可能已被更新的复制取代 —— 只有仍是当前代才记录。
    pub fn store_filtered(&self, id: RequestId, text: String, cursor: (f64, f64)) -> bool {
        let mut inner = self.lock();
        if !inner.generation.is_current(id) {
            return false;
        }
        inner.last_filtered = Some((text, cursor));
        true
    }

    /// 取走被拦截内容（读后即清）。手动覆盖路径的唯一数据源。
    pub fn take_filtered(&self) -> Option<(String, (f64, f64))> {
        self.lock().last_filtered.take()
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

    /// 该请求是否仍是当前代。
    ///
    /// 供 show 流程在触碰窗口**之前**做快速出局判断。注意单独使用它不构成
    /// 提交 —— 「检查」与「提交」之间隔着 await，必须配合 [`Self::run_if_current`]
    /// 的原子提交（QT-003 的核心约束）。
    pub fn is_current(&self, id: RequestId) -> bool {
        self.lock().generation.is_current(id)
    }

    /// 浮窗 show 流程的统一提交协议（QT-003）。
    ///
    /// 旧实现（translation_flow::show_popup_loading）的顺序是「窗口操作 →
    /// 无条件 emit loading」。loading 事件会把前端的 request identity 改成
    /// 发送者 —— 过期请求 A 在 B 已显示之后恢复，仍能把 UI 身份改成 A，
    /// 导致 B 的结果抵达时被前端丢弃，浮窗永远停在 loading。
    ///
    /// 协议三段，缺一不可：
    ///
    /// 1. **窗锁内代际检查**：过期请求不得触碰窗口（移位/显示），否则
    ///    过期 A 会把窗口从 B 的锚点挪走；
    /// 2. **窗口操作在窗锁内执行**：跨请求串行，杜绝 set_position/show
    ///    的交错；
    /// 3. **loading 事件经 `run_if_current` 原子提交**：判定与 emit 在
    ///    同一把同步锁内完成，中间没有 await —— 窗锁只串行化窗口操作，
    ///    真正的「提交与否」由代际闸门单独裁决。
    ///
    /// `window_ops` 返回 false 表示窗口创建/显示失败（`Failed`）。
    pub async fn commit_popup_show(
        &self,
        id: RequestId,
        window_ops: impl std::future::Future<Output = bool>,
        emit_loading: impl FnOnce(),
    ) -> ShowCommit {
        let _ops_guard = POPUP_SHOW_OPS.lock().await;

        if !self.is_current(id) {
            return ShowCommit::Superseded;
        }

        if !window_ops.await {
            return ShowCommit::Failed;
        }

        if self.run_if_current(id, emit_loading) {
            ShowCommit::Committed
        } else {
            // 窗口操作期间被取代：不发事件。窗锁保证当前代的 show 随后
            // 执行并夺回窗口位置（见 ShowCommit::Superseded 的文档）。
            ShowCommit::Superseded
        }
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

/// 自动翻译入口的过滤门控：开关开启且命中规则时返回拦截原因。
///
/// 抽成纯函数是为了让「过滤发生在任何网络请求之前」这条产品约束可测
/// —— 它没有任何 I/O，`execute_at_position` 在 supersede / 建窗 /
/// 启动翻译任务之前调用它，早退路径上不存在能发出请求的代码。
fn filter_gate(text: &str, enabled: bool) -> Option<SensitiveRule> {
    if !enabled {
        return None;
    }
    match clipboard_filter::classify(text) {
        FilterDecision::BlockSensitive { rule } => Some(rule),
        FilterDecision::Allow => None,
    }
}

/// 在指定光标位置执行翻译（剪贴板监控的唯一入口）。
///
/// 名字与签名与重构前 `translation_flow::execute_at_position` 一致，
/// 因此调用方只需改模块路径。
pub async fn execute_at_position(app: &AppHandle, cursor_x: f64, cursor_y: f64, text: String) {
    let state = app.state::<AppState>();
    let coordinator = state.coordinator.clone();

    // ── 敏感内容过滤：必须发生在任何网络请求 / UI 动作之前 ─────────────
    //
    // 放在 supersede 之前的语义：被拦截的内容不打断进行中的翻译、
    // 不显示浮窗、不产生任何请求。拦截反馈用 toast —— 一句不含原文的
    // 解释（计划 3.2 第 9 条「可解释反馈」）；原文不出现在事件、日志、
    // 历史里的任何地方。
    //
    // 拦截时仍要开一代「占位代」：手动覆盖接口按「当前代」校验，且
    // 被拦内容要暂存到 coordinator 供 take_filtered() 取走。这一代不
    // 安装任务、不发事件，仅承载拦截结果。
    let filter_enabled = state
        .config
        .read()
        .await
        .get("sensitive_filter_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);
    if let Some(rule) = filter_gate(&text, filter_enabled) {
        let len = text.len();
        let id = coordinator.supersede();
        coordinator.store_filtered(id, text, (cursor_x, cursor_y));
        state.runtime.record_clipboard_filtered();
        crate::commands::system::emit_toast(
            app,
            ToastPayload::warning_with_action(rule.reason(), "translate_filtered", "仍然翻译"),
        );
        tracing::info!(
            event = "clipboard_filtered_sensitive",
            rule = rule.code(),
            len = len,
            "[coordinator] 命中敏感内容过滤，已拦截（不翻译、不记录原文）"
        );
        return;
    }

    // 通过过滤、即将实际发起 —— 记为「上一次翻译」，供快捷键重试。
    coordinator.remember_text(&text);

    // 先开新一代：上一代的任务被就地 abort，其迟到结果也会因 id 不匹配被丢弃
    let id = coordinator.supersede();
    dispatch_at_position(app, &state, id, cursor_x, cursor_y, text).await;
}

/// 手动覆盖：翻译最近一条被敏感过滤拦截的内容。
///
/// 唯一的调用方是用户在前端的显式点击（`translate_filtered_text` 命令）
/// —— 这是「手动覆盖必须由用户主动触发」的产品约束在代码里的落点：
/// 没有任何自动路径会调用本函数。
///
/// 被拦截内容取走即清：覆盖只给一次机会，重复调用返回 `None`。
pub async fn translate_filtered_text(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let coordinator = state.coordinator.clone();

    let Some((text, (cursor_x, cursor_y))) = coordinator.take_filtered() else {
        return false;
    };

    // 手动覆盖就是一次正常翻译：开新代、走同一套 show/loading/结果闸门。
    // 过滤开关在此**不生效** —— 用户已显式授权发送这段内容。
    let id = coordinator.supersede();
    tracing::info!(
        event = "clipboard_filter_overridden",
        request_id = %id,
        len = text.len(),
        "[coordinator] 用户手动覆盖敏感过滤，继续翻译（原文不记录）"
    );
    dispatch_at_position(app, &state, id, cursor_x, cursor_y, text).await;
    true
}

/// 一次翻译的完整派发：建窗 → loading → 启动任务 → 安装句柄。
///
/// 自动路径（execute_at_position）与手动覆盖共用本函数 —— 两者只在
/// 「是否过了过滤器」上不同，之后的编排必须完全一致，否则覆盖路径会
/// 悄悄绕过代际闸门或 show 协议。
async fn dispatch_at_position(
    app: &AppHandle,
    state: &AppState,
    id: RequestId,
    cursor_x: f64,
    cursor_y: f64,
    text: String,
) {
    let coordinator = state.coordinator.clone();

    if text.trim().is_empty() {
        translation_flow::emit_error(app, "EMPTY_TEXT", "未检测到选中文本", &id.full());
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

    // 浮窗 show + loading 走统一提交协议（QT-003）：过期请求在这里出局，
    // 不建窗、不移位、不发 loading、不启动翻译任务 —— 它们没有任何一条
    // 路径能再改变 UI 的当前请求身份。
    //
    // 浮窗建不起来（WebView2 异常等）是真实的故障模式，上报给运行时层，
    // 让界面能如实显示，而不是让用户面对一个「什么都没发生」的复制操作。
    let show = coordinator
        .commit_popup_show(
            id,
            translation_flow::show_popup_window(app, &position),
            || translation_flow::emit_loading(app, &position, &id.full()),
        )
        .await;

    match show {
        ShowCommit::Committed => {
            // 浮窗已显示才记录基线：被动态的关闭看守要拿「浮窗出现时用户
            // 正在用谁」当参照物（Phase 8b）
            state.popup_watch.begin();
            state
                .runtime
                .report_popup(crate::runtime::ComponentState::Healthy, None);
            // 计划第 32 节：被动显示（未抢焦点）也要可观测 —— 排查「浮窗没
            // 弹出来」时，这条事件与 popup_watch 的关闭事件是同一条时间线的两端。
            tracing::info!(
                event = "popup_show_passive",
                request_id = %id,
                "[popup] 浮窗以被动态显示"
            );
        }
        ShowCommit::Superseded => {
            tracing::info!(
                event = "translation_request_superseded",
                request_id = %id,
                "[coordinator] 请求在显示阶段被更新的请求取代，已退出"
            );
            return;
        }
        ShowCommit::Failed => {
            state.runtime.report_popup(
                crate::runtime::ComponentState::Degraded,
                Some("POPUP_SHOW_FAILED"),
            );
            // 与旧行为一致：显示失败不阻断翻译任务 —— 结果事件仍会发出，
            // 窗口恢复后的下一轮操作还有机会看到状态
        }
    }

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
    let started = std::time::Instant::now();

    tracing::info!(
        event = "translation_request_started",
        request_id = %id,
        len = request.text.len(),
        target_lang = %request.target_lang,
        "[coordinator] 开始翻译"
    );

    // 控制台术语库引用（百度/有道）：配置随请求读取，改配置立即生效，
    // 不需要重建 provider。剪贴板路径没有显式源语言 → 不应用 DeepL glossary。
    let options = {
        let cfg = state.config.read().await;
        crate::domain::translator::TranslateOptions {
            baidu_use_term_base: cfg
                .get("baidu_use_term_base")
                .map(|v| v == "true")
                .unwrap_or(false),
            youdao_vocab_id: cfg.get("youdao_vocab_id").filter(|v| !v.is_empty()),
            tencent_term_repo_ids:
                crate::domain::translator::tencent::TencentProvider::parse_term_repo_ids(
                    &cfg.get("tencent_term_repo_ids").unwrap_or_default(),
                ),
            ..Default::default()
        }
    };

    let outcome = state
        .translator
        .translate_with_options(&request.text, &request.target_lang, &options)
        .await;

    // 网络组件的结论由**翻译源的整体健康**给出，而不是让界面从「刚才翻译
    // 失败了」去推断网络断了（计划第 4 节）。引擎知道每个源的真实状态，
    // 界面不知道。
    state
        .runtime
        .report_providers_health(state.translator.providers_health().await);

    // 诊断计数（计划第 33 节）：纯记账，不触发状态广播
    state.runtime.record_translation_result(outcome.is_ok());

    match outcome {
        Ok(mut result) => {
            result.truncated = request.truncated;
            // Instant 计时：墙钟回拨时 (now - start) 为负，as u64 会绕成
            // 巨大值。provider 内部早已用 Instant，只有这层聚合此前漏了
            result.duration_ms = started.elapsed().as_millis() as u64;

            let emitted = coordinator.run_if_current(id, || {
                translation_flow::emit_result(app, &result, &id.full());
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

            // 落盘放在 emit 之后：它是 await，会开出新的交错窗口，
            // 不能让它插在闸门与 emit 之间。迟到结果也不会落盘。
            persist_success(&state, &result, &request).await;
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
                translation_flow::emit_result(app, &original, &id.full());
            }) {
                log_stale_result(id);
            }
        }
        Err(e) => {
            // ── 本地缓存回退 ──────────────────────────────────────────────
            //
            // 计划第 15 节划的产品边界：本项目依赖云端翻译源，「离线也能翻译
            // 任何新文本」是做不到的承诺。能做的是——译过的内容仍然显示得出来。
            // 这是整条链路里唯一读缓存的地方，因此在线时结果永远是新鲜的。
            if should_fall_back_to_cache(&e) {
                if let Some(cached) = lookup_cached_result(
                    &state.cache,
                    &request.text,
                    &request.target_lang,
                    "",
                    request.truncated,
                )
                .await
                {
                    if coordinator.run_if_current(id, || {
                        translation_flow::emit_result(app, &cached, &id.full());
                    }) {
                        tracing::info!(
                            event = "translation_served_from_cache",
                            request_id = %id,
                            "[coordinator] 翻译源均不可用，改用本地缓存"
                        );
                    } else {
                        log_stale_result(id);
                    }
                    return;
                }
            }

            tracing::error!(
                event = "translation_request_failed",
                request_id = %id,
                code = e.error_code(),
                "[coordinator] 翻译失败: {}",
                e
            );

            if !coordinator.run_if_current(id, || {
                translation_flow::emit_error(app, e.error_code(), &e.to_string(), &id.full());
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

// 缓存回退（should_fall_back_to_cache / lookup_cached_result / CACHE_PROVIDER）
// 移到 domain/cache.rs：手动翻译路径（commands/translate.rs）与 coordinator
// 共用同一套「全部翻译源失败 → 查本地缓存」的语义。

/// 成功的翻译落盘：交给有界队列 + 单 worker（计划第 24 节）。
///
/// 这里只做入队 —— 不 spawn、不等数据库。调用方此刻已经把结果 emit 给
/// 用户了，落盘快慢、甚至队列溢出，都不影响用户已经看到的东西。
async fn persist_success(
    state: &AppState,
    result: &TranslationResult,
    request: &TranslationRequest,
) {
    let history_limit = state.config.read().await.cache_history_limit();

    let job = PersistJob {
        record: TranslationRecord::from_result(
            result,
            &request.text,
            &request.target_lang,
            crate::types::RECORD_SOURCE_CLIPBOARD,
        ),
        history_limit,
        cache_entry: OwnedCacheEntry {
            source_text: request.text.clone(),
            target_lang: request.target_lang.clone(),
            translated_text: result.translated_text.clone(),
            source_lang: result.detected_source_lang.clone(),
            provider: result.provider.clone(),
            // 剪贴板路径没有显式源语言 → 不应用术语表（DeepL 限制）
            glossary_fingerprint: String::new(),
        },
    };

    if !state.persistence.enqueue(job) {
        tracing::warn!(
            event = "persistence_queue_full",
            "[persistence] 落盘队列已满，本次历史与缓存写入被丢弃（翻译结果不受影响）"
        );
    }
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

    // ── 缓存回退的判定（计划第 15/16 节）────────────────────────────────

    /// 只有「所有翻译源都用不了」才回退缓存 —— 那才是离线的信号。
    #[test]
    fn remember_text_keeps_the_latest_and_survives_supersede() {
        let c = TranslationCoordinator::new();
        assert_eq!(c.last_text(), None);

        c.remember_text("first");
        c.remember_text("second");
        assert_eq!(c.last_text().as_deref(), Some("second"));

        // 新请求不该清掉「上一次翻译」—— 关闭浮窗后仍可重试
        let _ = c.supersede();
        assert_eq!(c.last_text().as_deref(), Some("second"));
    }

    /// 其他失败把真实原因告诉用户，比递一个来源不明的旧译文有用。
    #[test]
    fn cache_fallback_is_only_for_total_provider_failure() {
        assert!(should_fall_back_to_cache(&AppError::AllProvidersFailed {
            errors: vec![("deepl".into(), "timeout".into())],
        }));

        for err in [
            AppError::SameLanguage { lang: "zh".into() },
            AppError::NonTextContent,
            AppError::ProviderRejected {
                provider: "deepl".into(),
                status: 400,
            },
            AppError::AuthError {
                provider: "deepl".into(),
            },
        ] {
            assert!(!should_fall_back_to_cache(&err), "{:?} 不该回退缓存", err);
        }
    }

    // ── 敏感过滤门控（顺序契约：早于任何 I/O）──────────────────────────

    /// 合成的 AWS Key ID 形态（AKIA + 16 位大写字母数字）。
    fn aws_key_sample() -> String {
        format!("AKIA{}", "X7".repeat(8))
    }

    /// 开关关闭时一律放行 —— 用户明确选择承担风险。
    #[test]
    fn filter_gate_is_bypassed_when_disabled() {
        assert_eq!(
            filter_gate(aws_key_sample().as_str(), false),
            None,
            "关闭过滤后敏感内容也必须放行"
        );
    }

    /// 开启时命中高置信度凭证形态。
    #[test]
    fn filter_gate_blocks_sensitive_text_when_enabled() {
        assert_eq!(
            filter_gate(aws_key_sample().as_str(), true),
            Some(SensitiveRule::AwsAccessKeyId)
        );
    }

    /// 普通文本不受影响 —— 过滤不能伤害主链路的日常使用。
    #[test]
    fn filter_gate_allows_normal_text() {
        assert_eq!(filter_gate("Hello, world. 一段普通文本。", true), None);
        assert_eq!(filter_gate("", true), None);
    }

    // ── 敏感过滤暂存（手动覆盖的数据源）────────────────────────────────

    /// 拦截内容按请求代暂存：当前代可以写入，过期代不行。
    #[test]
    fn filtered_text_is_stored_only_for_the_current_generation() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();

        assert!(c.store_filtered(a, "secret".to_string(), (10.0, 20.0)));
        assert_eq!(c.take_filtered().map(|(t, _)| t).as_deref(), Some("secret"));

        let b = c.supersede();
        assert!(
            !c.store_filtered(a, "stale".to_string(), (0.0, 0.0)),
            "过期请求不得写入暂存"
        );
        assert!(c.take_filtered().is_none());
        assert!(c.store_filtered(b, "fresh".to_string(), (1.0, 2.0)));
    }

    /// 取走即清：手动覆盖只给一次机会，重复调用返回 None。
    #[test]
    fn filtered_text_is_consumed_once() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();
        c.store_filtered(a, "secret".to_string(), (0.0, 0.0));

        assert_eq!(c.take_filtered(), Some(("secret".to_string(), (0.0, 0.0))));
        assert!(c.take_filtered().is_none(), "第二次取走必须是空");
    }

    /// 新的复制（supersede）让上一条被拦截内容失效 —— 覆盖只对「刚刚
    /// 被拦的那条」有效，不能翻译用户早已放弃的内容。
    #[test]
    fn new_request_clears_the_pending_filtered_text() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();
        c.store_filtered(a, "secret".to_string(), (0.0, 0.0));

        let _b = c.supersede();
        assert!(c.take_filtered().is_none(), "新复制后旧拦截内容必须失效");
    }

    /// 被拦截的暂存不参与「在途任务」语义：拦截不该显示成有一个任务在跑。
    #[test]
    fn storing_filtered_text_does_not_install_an_in_flight_task() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();
        c.store_filtered(a, "secret".to_string(), (0.0, 0.0));
        assert!(!c.has_in_flight());
        assert_eq!(c.current(), Some(a));
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

    // ── QT-003：show + loading 提交协议（commit_popup_show）──────────────
    //
    // 旧实现的病理：show_popup_loading 在窗口操作后**无条件** emit loading。
    // A 在配置读取处暂停 → B 取代并显示 loading(B) → A 恢复，重新定位窗口
    // 并 emit loading(A) → 前端 request identity 变成 A → B 的结果抵达时被
    // 丢弃，浮窗永远停在 loading。下面的测试用受控交错把这条时序锁死：
    // 协议若失去任何一段闸门，对应的断言就会失败。

    /// 等待条件成立（单测内轮询，超时 2 秒）。
    fn wait_until(timeout: std::time::Duration, mut cond: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        cond()
    }

    /// 核心交错：A 通过窗锁内的代际检查、进入窗口操作；B 在 A 的窗口操作
    /// 期间取代之；A 完成后提交 loading 必须被拒。
    ///
    /// 旧协议（无条件 emit）在这条时序下会发出 loading(A) —— 前端身份被
    /// 污染、B 的结果被丢弃。新协议断言 A 什么都不能提交。
    #[test]
    fn stale_show_cannot_commit_loading_after_window_ops() {
        let c = Arc::new(TranslationCoordinator::new());
        let a = c.supersede();

        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let entered = Arc::new(AtomicBool::new(false));
        let entered_in_ops = entered.clone();
        let committed_a = Arc::new(AtomicBool::new(false));
        let committed_flag = committed_a.clone();

        let c_in_task = c.clone();
        let handle = tauri::async_runtime::spawn(async move {
            c_in_task
                .commit_popup_show(
                    a,
                    async move {
                        // 走到这里说明已通过窗锁内的代际检查（此刻 A 仍是当前代）
                        entered_in_ops.store(true, Ordering::SeqCst);
                        let _ = release_rx.await;
                        true // 窗口操作「成功」
                    },
                    || {
                        committed_flag.store(true, Ordering::SeqCst);
                    },
                )
                .await
        });

        assert!(
            wait_until(Duration::from_secs(2), || entered.load(Ordering::SeqCst)),
            "A 应通过代际检查并进入窗口操作"
        );

        // A 卡在窗口操作期间，B 取代之
        let _b = c.supersede();
        let _ = release_tx.send(());

        let outcome = tauri::async_runtime::block_on(handle).unwrap();
        assert_eq!(
            outcome,
            ShowCommit::Superseded,
            "窗口操作期间被取代的请求必须以 Superseded 结束"
        );
        assert!(
            !committed_a.load(Ordering::SeqCst),
            "旧行为的病灶：过期请求的 loading 事件绝不能发出"
        );
    }

    /// 已过期的请求在窗锁内的第一道检查就必须出局：不触碰窗口（不移位、
    /// 不显示），更不发事件。
    #[test]
    fn superseded_request_never_touches_the_window() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();
        let _b = c.supersede(); // A 在进入流程前就已被取代

        let touched = Arc::new(AtomicBool::new(false));
        let touched_flag = touched.clone();
        let committed = Arc::new(AtomicBool::new(false));
        let committed_flag = committed.clone();

        let outcome = tauri::async_runtime::block_on(c.commit_popup_show(
            a,
            async {
                touched_flag.store(true, Ordering::SeqCst);
                true
            },
            || committed_flag.store(true, Ordering::SeqCst),
        ));

        assert_eq!(outcome, ShowCommit::Superseded);
        assert!(
            !touched.load(Ordering::SeqCst),
            "过期请求不得执行任何窗口操作（B 的锚点不能被挪走）"
        );
        assert!(!committed.load(Ordering::SeqCst));
    }

    /// 窗口创建/显示失败必须如实上报 Failed，而不是伪装成已显示。
    #[test]
    fn failed_window_ops_report_failed() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();

        let outcome = tauri::async_runtime::block_on(c.commit_popup_show(
            a,
            async { false }, // 建窗失败
            || {},
        ));

        assert_eq!(outcome, ShowCommit::Failed);
    }

    /// 当前代请求的完整提交：窗口操作成功 + loading 事件发出 = Committed。
    #[test]
    fn current_show_commits_loading() {
        let c = TranslationCoordinator::new();
        let a = c.supersede();

        let committed = Arc::new(AtomicBool::new(false));
        let committed_flag = committed.clone();

        let outcome =
            tauri::async_runtime::block_on(c.commit_popup_show(a, async { true }, || {
                committed_flag.store(true, Ordering::SeqCst)
            }));

        assert_eq!(outcome, ShowCommit::Committed);
        assert!(
            committed.load(Ordering::SeqCst),
            "当前代的 loading 必须发出"
        );
    }

    /// 完整 A/B 交错（对齐 TEST_PLAN §4 的推荐序列）：
    /// A 通过检查并暂停在窗口操作里 → B 取代并完整走完 show → A 恢复。
    /// 断言：A 不发事件；B 的 loading 是唯一被提交的；后续 B 的结果仍能过闸门。
    #[test]
    fn interleave_a_paused_b_completes_then_a_resumes() {
        let c = Arc::new(TranslationCoordinator::new());
        let a = c.supersede();

        let (release_a_tx, release_a_rx) = tokio::sync::oneshot::channel::<()>();
        let entered = Arc::new(AtomicBool::new(false));
        let entered_in_ops = entered.clone();
        let loadings: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let c_in_task = c.clone();
        let loadings_in_task = loadings.clone();
        let a_handle = tauri::async_runtime::spawn(async move {
            c_in_task
                .commit_popup_show(
                    a,
                    async move {
                        entered_in_ops.store(true, Ordering::SeqCst);
                        let _ = release_a_rx.await;
                        true
                    },
                    move || loadings_in_task.lock().unwrap().push("A"),
                )
                .await
        });

        assert!(wait_until(Duration::from_secs(2), || entered.load(Ordering::SeqCst)));

        // B 取代 A，并启动自己的 show（此刻窗锁仍被 A 持有，B 只能等待）
        let b = c.supersede();
        let c_in_b = c.clone();
        let loadings_in_b = loadings.clone();
        let b_handle = tauri::async_runtime::spawn(async move {
            c_in_b
                .commit_popup_show(b, async { true }, move || {
                    loadings_in_b.lock().unwrap().push("B")
                })
                .await
        });

        // 释放 A：A 的 loading 提交被代际闸门拒绝（不发事件），窗锁随之释放
        let _ = release_a_tx.send(());
        let a_outcome = tauri::async_runtime::block_on(a_handle).unwrap();
        let b_outcome = tauri::async_runtime::block_on(b_handle).unwrap();

        assert_eq!(a_outcome, ShowCommit::Superseded);
        assert_eq!(
            b_outcome,
            ShowCommit::Committed,
            "B 在 A 释放窗锁后必须能完整提交 —— 串行不得变成永久阻塞"
        );
        assert_eq!(
            *loadings.lock().unwrap(),
            vec!["B"],
            "被提交的 loading 只能有 B —— 过期请求不得改变 UI 的当前请求身份"
        );

        // B 的结果仍能通过结果闸门（前端不会收到身份不匹配的丢弃）
        let result_emitted = Arc::new(AtomicBool::new(false));
        let flag = result_emitted.clone();
        assert!(c.run_if_current(b, || flag.store(true, Ordering::SeqCst)));
        assert!(result_emitted.load(Ordering::SeqCst));
    }

    /// 窗锁串行：A 持锁期间 B 的 show 必须等待，B 在 A 之后完成并把窗口
    /// 带回 B 的位置 —— 最终窗口状态属于当前代。
    #[test]
    fn window_ops_are_serialized_and_current_generation_wins_the_position() {
        let c = Arc::new(TranslationCoordinator::new());
        let a = c.supersede();

        let (release_a_tx, release_a_rx) = tokio::sync::oneshot::channel::<()>();
        let entered = Arc::new(AtomicBool::new(false));
        let entered_in_ops = entered.clone();
        let positions: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let c_in_task = c.clone();
        let a_handle = tauri::async_runtime::spawn(async move {
            c_in_task
                .commit_popup_show(
                    a,
                    async move {
                        entered_in_ops.store(true, Ordering::SeqCst);
                        let _ = release_a_rx.await;
                        true
                    },
                    || {},
                )
                .await
        });

        assert!(wait_until(Duration::from_secs(2), || entered.load(Ordering::SeqCst)));

        let b = c.supersede();
        let c_in_b = c.clone();
        let positions_in_b = positions.clone();
        let b_handle = tauri::async_runtime::spawn(async move {
            c_in_b
                .commit_popup_show(
                    b,
                    async move {
                        positions_in_b.lock().unwrap().push("B");
                        true
                    },
                    || {},
                )
                .await
        });

        // B 此刻必须被窗锁挡住（A 还没释放）
        std::thread::sleep(Duration::from_millis(30));
        assert!(
            positions.lock().unwrap().is_empty(),
            "B 的窗口操作必须等待 A 释放窗锁，不得交错"
        );

        let _ = release_a_tx.send(());
        let a_outcome = tauri::async_runtime::block_on(a_handle).unwrap();
        let b_outcome = tauri::async_runtime::block_on(b_handle).unwrap();

        assert_eq!(a_outcome, ShowCommit::Superseded);
        assert_eq!(b_outcome, ShowCommit::Committed);
        assert_eq!(
            *positions.lock().unwrap(),
            vec!["B"],
            "最终的窗口位置必须由当前代决定"
        );
    }
}
