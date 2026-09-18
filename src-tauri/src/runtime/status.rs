// src-tauri/src/runtime/status.rs
// 统一运行时状态层（计划第 4 节）。
//
// 存在的理由只有一条：**UI 不自己推测状态。**
//
//     ❌ 「因为 Provider 失败，所以认为网络断了」
//     ❌ 「因为历史没写进去，所以认为应用坏了」
//     ✅ RuntimeStatus 给出结论
//
// 在此之前，每个子系统各自持有健康数据，谁都没有一个「应用现在到底怎么样」
// 的答案；界面要么自己拼，要么什么都不显示。
//
// 两个组件是**派生**的，两个是**上报**的：
//
//   clipboard  派生自 MonitorController 的健康快照 —— 那份数据已经存在，
//              再维护一份只会两边不一致
//   network    派生自引擎的翻译源整体健康 —— 由引擎算好结论，运行时层照搬
//   popup      上报。窗口操作分散在 translation_flow，没有既有的健康对象
//   storage    上报。历史/缓存的写失败发生在落盘 worker 内部
//
// 状态与配置严格分离（计划第 49 节）：这里只放运行时状态，
// 用户配置（target_lang / 凭证 / 主题）永远不进来。

use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::domain::translator::health::ProvidersHealth;
use crate::system::clipboard::MonitorController;

/// 状态变化时的广播出口。
///
/// 用回调而不是直接持有 `AppHandle`：运行时层因此完全不依赖 Tauri，
/// 单元测试可以传一个只记录调用的实现（传 AppHandle 的话，没有 Tauri
/// 应用就构造不出这个层，状态逻辑也就无从测试）。
pub type Notifier = Arc<dyn Fn(&RuntimeStatusSnapshot) + Send + Sync>;

/// 什么都不做的广播口：测试与无界面场景用
pub fn no_notifier() -> Notifier {
    Arc::new(|_| {})
}

/// 单个子系统的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    Healthy,
    /// 有故障但功能可能仍可用 —— UI 不该据此报警
    Degraded,
    /// 正在恢复中
    Recovering,
    /// 被用户主动关掉。**不是故障**，不参与整体健康度计算
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentHealth {
    pub state: ComponentState,
    /// 稳定的错误码，**绝不包含用户复制的内容或凭证**（计划第 32 节）
    pub last_error_code: Option<String>,
    pub last_error_at_ms: Option<i64>,
    pub last_recovery_at_ms: Option<i64>,
}

impl Default for ComponentHealth {
    fn default() -> Self {
        Self {
            state: ComponentState::Healthy,
            last_error_code: None,
            last_error_at_ms: None,
            last_recovery_at_ms: None,
        }
    }
}

impl ComponentHealth {
    /// 记录一次状态迁移。恢复（离开故障态）时补记恢复时刻。
    fn transition(&mut self, next: ComponentState, code: Option<&'static str>) {
        let was_faulted = matches!(
            self.state,
            ComponentState::Degraded | ComponentState::Recovering
        );
        let is_faulted = matches!(next, ComponentState::Degraded | ComponentState::Recovering);

        if let Some(c) = code {
            self.last_error_code = Some(c.to_string());
            self.last_error_at_ms = Some(crate::types::now_unix_ms());
        }
        if was_faulted && !is_faulted {
            self.last_recovery_at_ms = Some(crate::types::now_unix_ms());
        }
        self.state = next;
    }
}

/// 应用整体健康度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealth {
    Healthy,
    Degraded,
    Recovering,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatusSnapshot {
    pub overall: RuntimeHealth,
    pub uptime_ms: u64,
    pub clipboard: ComponentHealth,
    pub network: ComponentHealth,
    pub popup: ComponentHealth,
    pub storage: ComponentHealth,
}

/// 整体健康度聚合。
///
/// `Disabled` **不参与**：用户主动关掉剪贴板监控不是应用出了问题，
/// 把它算成 Degraded 会让托盘图标在用户自己关掉功能后一直显示异常。
fn aggregate(states: &[ComponentState]) -> RuntimeHealth {
    if states.contains(&ComponentState::Recovering) {
        RuntimeHealth::Recovering
    } else if states.contains(&ComponentState::Degraded) {
        RuntimeHealth::Degraded
    } else {
        RuntimeHealth::Healthy
    }
}

/// 上报式组件的集合（剪贴板与网络是派生的，不在这里）
#[derive(Default)]
struct Reported {
    network: ComponentHealth,
    popup: ComponentHealth,
    storage: ComponentHealth,
}

/// 上次对外广播的状态指纹。只在**真的变了**的时候推送（计划第 50 节：
/// 不要每 100ms 推一次）。
type PublishedKey = (
    RuntimeHealth,
    ComponentState,
    ComponentState,
    ComponentState,
    ComponentState,
);

pub struct RuntimeStatus {
    notifier: Notifier,
    clipboard: Arc<MonitorController>,
    started_at: Instant,
    reported: Mutex<Reported>,
    last_published: Mutex<Option<PublishedKey>>,
}

impl RuntimeStatus {
    pub fn new(clipboard: Arc<MonitorController>, notifier: Notifier) -> Arc<Self> {
        Arc::new(Self {
            notifier,
            clipboard,
            started_at: Instant::now(),
            reported: Mutex::new(Reported::default()),
            last_published: Mutex::new(None),
        })
    }

    pub fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    /// 剪贴板组件：从既有健康快照派生，不重复维护。
    fn clipboard_health(&self) -> ComponentHealth {
        // 用户关掉监控 → Disabled，而不是「坏掉了」
        if self.clipboard.is_suspended() {
            return ComponentHealth {
                state: ComponentState::Disabled,
                ..ComponentHealth::default()
            };
        }

        let h = self.clipboard.health_snapshot();
        let state = match h.state {
            crate::system::clipboard::health::HealthState::Healthy => ComponentState::Healthy,
            crate::system::clipboard::health::HealthState::Degraded => ComponentState::Degraded,
            crate::system::clipboard::health::HealthState::Recovering => ComponentState::Recovering,
        };

        // 注意：剪贴板故障时是 Degraded 还是 Recovering 由它自己决定，
        // 这里只做类型映射，不做任何二次判断。
        ComponentHealth {
            state,
            last_error_code: h.last_error_code.map(|c| c.to_string()),
            last_error_at_ms: h.last_error_at_ms,
            last_recovery_at_ms: h.last_recovery_at_ms,
        }
    }

    pub fn snapshot(&self) -> RuntimeStatusSnapshot {
        let reported = self.reported.lock().unwrap_or_else(|e| e.into_inner());
        let clipboard = self.clipboard_health();

        let statuses = [
            clipboard.state,
            reported.network.state,
            reported.popup.state,
            reported.storage.state,
        ];

        RuntimeStatusSnapshot {
            overall: aggregate(&statuses),
            uptime_ms: self.uptime_ms(),
            clipboard,
            network: reported.network.clone(),
            popup: reported.popup.clone(),
            storage: reported.storage.clone(),
        }
    }

    /// 上报网络组件状态。由 coordinator 在每次翻译后调用 —— 它知道整条链的
    /// 结果，而引擎知道单个翻译源的健康，两者都不该由界面去猜。
    pub fn report_network(&self, state: ComponentState, code: Option<&'static str>) {
        self.report(|r| &mut r.network, state, code);
    }

    pub fn report_popup(&self, state: ComponentState, code: Option<&'static str>) {
        self.report(|r| &mut r.popup, state, code);
    }

    pub fn report_storage(&self, state: ComponentState, code: Option<&'static str>) {
        self.report(|r| &mut r.storage, state, code);
    }

    /// 把引擎的翻译源健康结论翻译成网络组件状态。
    ///
    /// 这是「UI 不自己推测」的落点：网络好不好，由翻译源的整体可用性说话。
    pub fn report_providers_health(&self, health: ProvidersHealth) {
        let (state, code) = match health {
            ProvidersHealth::Healthy => (ComponentState::Healthy, None),
            ProvidersHealth::Degraded => (ComponentState::Degraded, Some("PROVIDERS_DEGRADED")),
            ProvidersHealth::Unavailable => {
                (ComponentState::Degraded, Some("PROVIDERS_UNAVAILABLE"))
            }
            // 没配任何凭证不是故障 —— 那是用户还没设置
            ProvidersHealth::Unconfigured => (ComponentState::Disabled, None),
        };
        self.report_network(state, code);
    }

    /// 剪贴板健康发生变化后由 MonitorController 调用，触发一次对外广播。
    ///
    /// 剪贴板状态是派生的，所以没有「上报值」可改 —— 只需要重新评估并
    /// 在必要时通知。
    pub(crate) fn publish(&self) {
        self.publish_if_changed();
    }

    fn report(
        &self,
        pick: impl FnOnce(&mut Reported) -> &mut ComponentHealth,
        state: ComponentState,
        code: Option<&'static str>,
    ) {
        {
            let mut reported = self.reported.lock().unwrap_or_else(|e| e.into_inner());
            pick(&mut reported).transition(state, code);
        }
        self.publish_if_changed();
    }

    fn publish_if_changed(&self) {
        let snapshot = self.snapshot();
        let key: PublishedKey = (
            snapshot.overall,
            snapshot.clipboard.state,
            snapshot.network.state,
            snapshot.popup.state,
            snapshot.storage.state,
        );

        {
            let mut last = self
                .last_published
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if *last == Some(key) {
                return;
            }
            *last = Some(key);
        }

        tracing::info!(
            event = "runtime_status_changed",
            overall = ?snapshot.overall,
            clipboard = ?snapshot.clipboard.state,
            network = ?snapshot.network.state,
            "[runtime] 状态变化: {:?}",
            snapshot.overall
        );
        (self.notifier)(&snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_components_do_not_degrade_the_whole_app() {
        // 用户主动关掉剪贴板监控，不该让托盘图标显示「有问题」
        assert_eq!(
            aggregate(&[ComponentState::Disabled, ComponentState::Healthy]),
            RuntimeHealth::Healthy
        );
        assert_eq!(
            aggregate(&[ComponentState::Disabled, ComponentState::Disabled]),
            RuntimeHealth::Healthy
        );
    }

    #[test]
    fn recovering_outranks_degraded() {
        assert_eq!(
            aggregate(&[ComponentState::Healthy, ComponentState::Degraded]),
            RuntimeHealth::Degraded
        );
        assert_eq!(
            aggregate(&[
                ComponentState::Degraded,
                ComponentState::Recovering,
                ComponentState::Healthy
            ]),
            RuntimeHealth::Recovering
        );
    }

    #[test]
    fn all_healthy_is_healthy() {
        assert_eq!(
            aggregate(&[ComponentState::Healthy; 4]),
            RuntimeHealth::Healthy
        );
    }

    // ── 状态迁移的时间戳记账 ─────────────────────────────────────────────

    #[test]
    fn failure_records_the_error_code_and_time() {
        let mut h = ComponentHealth::default();
        h.transition(ComponentState::Degraded, Some("CLIPBOARD_OCCUPIED"));

        assert_eq!(h.state, ComponentState::Degraded);
        assert_eq!(h.last_error_code.as_deref(), Some("CLIPBOARD_OCCUPIED"));
        assert!(h.last_error_at_ms.is_some());
        assert!(h.last_recovery_at_ms.is_none());
    }

    #[test]
    fn recovery_from_a_fault_is_timestamped() {
        let mut h = ComponentHealth::default();
        h.transition(ComponentState::Degraded, Some("X"));
        h.transition(ComponentState::Healthy, None);

        assert_eq!(h.state, ComponentState::Healthy);
        assert!(
            h.last_recovery_at_ms.is_some(),
            "离开故障态必须补记恢复时刻，否则「什么时候好的」无从回答"
        );
    }

    /// 健康 → 健康 不该被记成一次「恢复」
    #[test]
    fn staying_healthy_does_not_register_a_recovery() {
        let mut h = ComponentHealth::default();
        h.transition(ComponentState::Healthy, None);
        assert!(h.last_recovery_at_ms.is_none());
    }

    /// Disabled 不是故障态：关掉再打开不该被记成「恢复」
    #[test]
    fn disabled_is_not_a_fault_state() {
        let mut h = ComponentHealth::default();
        h.transition(ComponentState::Disabled, None);
        h.transition(ComponentState::Healthy, None);
        assert!(h.last_recovery_at_ms.is_none());
    }

    // ── 翻译源健康 → 网络组件状态 ────────────────────────────────────────

    fn mapped(health: ProvidersHealth) -> ComponentState {
        match health {
            ProvidersHealth::Healthy => ComponentState::Healthy,
            ProvidersHealth::Degraded | ProvidersHealth::Unavailable => ComponentState::Degraded,
            ProvidersHealth::Unconfigured => ComponentState::Disabled,
        }
    }

    #[test]
    fn unconfigured_providers_are_disabled_not_broken() {
        assert_eq!(
            mapped(ProvidersHealth::Unconfigured),
            ComponentState::Disabled,
            "用户还没配凭证不是应用故障"
        );
    }

    #[test]
    fn unavailable_providers_degrade_the_network_component() {
        assert_eq!(
            mapped(ProvidersHealth::Unavailable),
            ComponentState::Degraded
        );
    }

    // ── 运行时层本体（不依赖 Tauri，因此可测）────────────────────────────

    fn test_runtime(notifier: Notifier) -> Arc<RuntimeStatus> {
        RuntimeStatus::new(Arc::new(MonitorController::new()), notifier)
    }

    #[test]
    fn fresh_runtime_is_healthy() {
        let s = test_runtime(no_notifier()).snapshot();
        assert_eq!(s.overall, RuntimeHealth::Healthy);
        assert_eq!(s.clipboard.state, ComponentState::Healthy);
        assert_eq!(s.network.state, ComponentState::Healthy);
    }

    /// 用户主动关掉剪贴板监控 → Disabled，而不是「应用坏了」
    #[test]
    fn suspending_the_monitor_reports_disabled_not_broken() {
        let ctrl = Arc::new(MonitorController::new());
        let rt = RuntimeStatus::new(ctrl.clone(), no_notifier());

        ctrl.suspend();

        assert_eq!(rt.snapshot().clipboard.state, ComponentState::Disabled);
        assert_eq!(
            rt.snapshot().overall,
            RuntimeHealth::Healthy,
            "用户自己关掉的功能不该让整体健康度下降"
        );
    }

    /// 计划第 50 节：只在状态**变化**时广播，不要每次都推
    #[test]
    fn notifier_fires_only_when_the_state_actually_changes() {
        let calls = Arc::new(Mutex::new(0usize));
        let counter = calls.clone();
        let rt = test_runtime(Arc::new(move |_| {
            *counter.lock().unwrap() += 1;
        }));

        rt.report_network(ComponentState::Degraded, Some("PROVIDERS_UNAVAILABLE"));
        assert_eq!(*calls.lock().unwrap(), 1);

        rt.report_network(ComponentState::Degraded, Some("PROVIDERS_UNAVAILABLE"));
        assert_eq!(*calls.lock().unwrap(), 1, "状态没变就不该再广播一次");

        rt.report_network(ComponentState::Healthy, None);
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    /// 剪贴板是派生组件：健康变化经 controller 流入快照
    #[test]
    fn clipboard_failures_flow_into_the_snapshot() {
        let ctrl = Arc::new(MonitorController::new());
        let rt = RuntimeStatus::new(ctrl.clone(), no_notifier());
        ctrl.attach_runtime(rt.clone());

        ctrl.note_failure("CLIPBOARD_OCCUPIED");
        let s = rt.snapshot();
        assert_eq!(s.clipboard.state, ComponentState::Degraded);
        assert_eq!(s.overall, RuntimeHealth::Degraded);
        assert_eq!(
            s.clipboard.last_error_code.as_deref(),
            Some("CLIPBOARD_OCCUPIED")
        );

        ctrl.note_success();
        assert_eq!(rt.snapshot().clipboard.state, ComponentState::Healthy);
        assert_eq!(rt.snapshot().overall, RuntimeHealth::Healthy);
    }

    /// 快照绝不能带出凭证或用户内容（计划第 32 节）
    #[test]
    fn snapshot_carries_no_credentials_or_user_content() {
        let ctrl = Arc::new(MonitorController::new());
        let rt = RuntimeStatus::new(ctrl.clone(), no_notifier());
        ctrl.note_failure("CLIPBOARD_READ_FAILED");

        let json = serde_json::to_string(&rt.snapshot()).unwrap();
        for forbidden in ["api_key", "secret", "password", "token"] {
            assert!(
                !json.to_lowercase().contains(forbidden),
                "状态快照里不该出现 {}",
                forbidden
            );
        }
    }
}
