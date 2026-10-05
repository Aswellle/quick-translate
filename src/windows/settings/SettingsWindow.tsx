// src/windows/settings/SettingsWindow.tsx
// 设置面板 — macOS 简约明亮灵动风格

import { useEffect, useState, useCallback } from "react";
import {
  getConfig,
  setConfigBatch,
  validateProvider,
  getAutostart,
  getStats,
  openUrl,
  getProviderStatus,
  getRuntimeStatus,
  getRuntimeDiagnostics,
  onRuntimeStatusChanged,
  setClipboardMonitorEnabled as invokeSetClipboardMonitor,
  type AppConfig,
  type StatsResult,
  type ProviderStatus,
  type RuntimeStatusSnapshot,
  type RuntimeDiagnostics,
} from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import { SUPPORTED_LANGUAGES, PROVIDERS, ERROR_MESSAGES } from "@/lib/constants";
import { useConfigStore } from "@/stores/configStore";
import { applyTheme } from "@/hooks/useTheme";

// 需要特殊处理的凭证字段：getConfig() 返回 masked 值，不能直接写回
const CREDENTIAL_KEYS = PROVIDERS
  .filter((p) => p.requiresApiKey)
  .flatMap((p) => p.credentialFields.map((f) => f.key));

type TabId = "general" | "provider";

export function SettingsWindow({ initialTab }: { initialTab?: "general" | "provider" }) {
  const { setConfig } = useConfigStore();
  // 初始 Tab 可由 URL hash 指定（托盘「配置翻译源…」直达路径）
  const [activeTab, setActiveTab] = useState<TabId>(initialTab ?? "general");
  const [draft, setDraft] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "ok" | "err">("idle");
  const [testingId, setTestingId] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, boolean | null>>({});
  const [stats, setStats] = useState<StatsResult | null>(null);
  const [statsLoading, setStatsLoading] = useState(false);
  // 标记哪些凭证字段在后端已配置（getConfig 返回 masked 非空值）
  // 用于显示"已配置"状态及决定 Save 时是否包含该字段
  const [maskedCredentials, setMaskedCredentials] = useState<Record<string, boolean>>({});
  // 翻译源运行时状态（计划第 28 节）：由后端 Runtime 层给出，前端不自行推测。
  // 挂载时拉一次；运行时状态变化（后端有指纹去重，低频）时刷新。
  const [providerStatus, setProviderStatus] = useState<Record<string, ProviderStatus>>({});
  // 运行时状态快照 + 诊断计数（计划第 33/50 节）：由 Runtime 层给出，面板只做转译。
  // 诊断计数不主动广播，随状态变化时一并刷新。
  const [runtimeSnapshot, setRuntimeSnapshot] = useState<RuntimeStatusSnapshot | null>(null);
  const [runtimeDiag, setRuntimeDiag] = useState<RuntimeDiagnostics | null>(null);

  useEffect(() => {
    let cancelled = false;
    const refresh = () => {
      getProviderStatus()
        .then((list) => {
          if (cancelled) return;
          const map: Record<string, ProviderStatus> = {};
          for (const p of list) map[p.id] = p;
          setProviderStatus(map);
        })
        .catch(() => {
          /* 状态拉取失败不影响设置面板其它功能 */
        });
      getRuntimeStatus()
        .then((snap) => {
          if (!cancelled) setRuntimeSnapshot(snap);
        })
        .catch(() => {});
      getRuntimeDiagnostics()
        .then((d) => {
          if (!cancelled) setRuntimeDiag(d);
        })
        .catch(() => {});
    };
    refresh();
    let unlisten: (() => void) | undefined;
    onRuntimeStatusChanged(() => refresh())
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // 托盘「配置翻译源…」对已打开的设置窗口发事件切换 Tab
  // （新建窗口走 URL hash，见 App.tsx 的 initialTab）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<{ tab: string }>("settings-navigate", (e) => {
          if (e.payload.tab === "provider" || e.payload.tab === "general") {
            setActiveTab(e.payload.tab);
          }
        })
      )
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // 加载配置
  useEffect(() => {
    Promise.all([getConfig(), getAutostart()])
      .then(([cfg, autoStart]) => {
        // 记录哪些凭证字段已在后端配置（非空 masked 值 = 已配置）
        const masked: Record<string, boolean> = {};
        for (const key of CREDENTIAL_KEYS) {
          masked[key] = !!((cfg as unknown as Record<string, string>)[key]);
        }
        setMaskedCredentials(masked);

        // 将凭证字段清空：输入框始终从空白开始，避免 masked 占位符被误写回
        const cleaned = { ...cfg, auto_start: autoStart } as unknown as Record<string, string>;
        for (const key of CREDENTIAL_KEYS) {
          cleaned[key] = "";
        }
        const mergedCfg = cleaned as unknown as AppConfig;
        setConfig(mergedCfg);
        setDraft(mergedCfg);
      })
      .catch((e) => toast("加载配置失败：" + String(e), "error"));
  }, [setConfig]);

  // 加载使用统计
  useEffect(() => {
    if (activeTab !== "general") return;
    setStatsLoading(true);
    getStats()
      .then(setStats)
      .catch(() => setStats(null))
      .finally(() => setStatsLoading(false));
  }, [activeTab]);

  const updateDraft = useCallback(<K extends keyof AppConfig>(key: K, value: AppConfig[K]) => {
    setDraft((prev) => (prev ? { ...prev, [key]: value } : prev));
  }, []);

  // 剪贴板监控开关：立即生效，同时更新 draft 供下次保存持久化
  const handleClipboardToggle = useCallback((enabled: boolean) => {
    updateDraft("clipboard_monitor_enabled", enabled);
    invokeSetClipboardMonitor(enabled).catch((e) =>
      toast("切换剪贴板监控失败：" + String(e), "error")
    );
  }, [updateDraft]);

  // 保存
  const handleSave = useCallback(async () => {
    if (!draft) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      // 非凭证字段：始终包含
      const updates: [string, string][] = [
        ["target_lang",               draft.target_lang],
        ["provider",                  draft.provider],
        ["auto_start",                String(draft.auto_start)],
        ["history_limit",             String(draft.history_limit)],
        ["theme",                     draft.theme],
        ["fallback_enabled",          String(draft.fallback_enabled)],
        ["clipboard_monitor_enabled", String(draft.clipboard_monitor_enabled)],
      ];
      // 凭证字段：仅在用户本次实际输入了新值时才包含，防止 masked 占位符写回破坏原有凭证
      for (const key of CREDENTIAL_KEYS) {
        const newVal = ((draft as unknown) as Record<string, string>)[key]?.trim();
        if (newVal) updates.push([key, newVal]);
      }
      await setConfigBatch(updates);
      setConfig(draft);
      applyTheme(draft.theme);
      setSaveStatus("ok");
      toast("设置已保存", "success");
      setTimeout(() => setSaveStatus("idle"), 2500);
    } catch (err: unknown) {
      setSaveStatus("err");
      // 计划第 52 节：错误码 ≠ 用户文案。后端 AppError 序列化为 {code, message}，
      // 优先查 ERROR_MESSAGES 映射表，查不到才退回原始 message。
      const e = err as { code?: string; message?: string };
      const msg =
        (e?.code && ERROR_MESSAGES[e.code]) ||
        e?.message ||
        (err instanceof Error ? err.message : String(err));
      toast("保存失败：" + msg, "error");
    } finally {
      setSaving(false);
    }
  }, [draft, setConfig]);

  // 测试翻译源
  const handleTest = useCallback(async (providerId: string) => {
    setTestingId(providerId);
    setTestResults((prev) => ({ ...prev, [providerId]: null }));
    try {
      // 测试前只保存该 provider 中用户本次实际填入的凭证（非空）
      // 不发送空值，否则会清除后端已保存的凭证
      const providerMeta = PROVIDERS.find((p) => p.id === providerId);
      if (providerMeta && draft) {
        const credUpdates: [string, string][] = providerMeta.credentialFields
          .filter(f => !!((draft as unknown as Record<string, string>)[f.key]?.trim()))
          .map(f => [f.key, (draft as unknown as Record<string, string>)[f.key]]);
        if (credUpdates.length > 0) await setConfigBatch(credUpdates);
      }
      const ok = await validateProvider(providerId);
      setTestResults((prev) => ({ ...prev, [providerId]: ok }));
      if (!ok) toast(`${PROVIDERS.find((p) => p.id === providerId)?.name} 凭证无效`, "warning");
      else toast("连接测试通过 ✓", "success");
    } catch {
      setTestResults((prev) => ({ ...prev, [providerId]: false }));
      toast("连接测试失败，请检查网络", "error");
    } finally {
      setTestingId(null);
    }
  }, [draft]);

  if (!draft) {
    return (
      <div className="flex items-center justify-center h-full gap-2 text-sm text-[var(--text-secondary)]">
        <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
        加载中…
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen bg-[var(--bg-secondary)] dark:bg-[var(--bg-primary)] text-[var(--text-primary)] select-none">
      {/* ── 标题栏 ── */}
      <div className="px-6 pt-4 pb-3 bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border-b border-[var(--border-secondary)]">
        <div className="flex items-center gap-2">
          <h1 className="text-[15px] font-semibold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>
            设置
          </h1>
        </div>
        <p className="text-[12px] text-[var(--text-secondary)] mt-0.5">
          QuickTranslate
        </p>
      </div>

      {/* ── macOS 分段 Tab ── */}
      <div className="px-6 pt-3 bg-[var(--bg-secondary)]">
        <div className="inline-flex rounded-lg bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] p-0.5 gap-0.5">
          {(["general", "provider"] as TabId[]).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={[
                "px-4 py-1.5 text-[12.5px] font-medium rounded-[7px] transition-all duration-150",
                activeTab === tab
                  ? "bg-[var(--surface-primary)] dark:bg-[var(--surface-tertiary)] text-[var(--text-primary)] shadow-sm"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]",
              ].join(" ")}
            >
              {tab === "general" ? "常规" : "翻译源"}
            </button>
          ))}
        </div>
      </div>

      {/* ── 内容区 ── */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        {activeTab === "general" && (
          <GeneralTab draft={draft} onChange={updateDraft} onClipboardToggle={handleClipboardToggle} stats={stats} statsLoading={statsLoading} runtimeSnapshot={runtimeSnapshot} runtimeDiag={runtimeDiag} providerStatus={providerStatus} onNavigate={setActiveTab} />
        )}
        {activeTab === "provider" && (
          <ProviderTab
            draft={draft}
            onChange={updateDraft}
            onTest={handleTest}
            testingId={testingId}
            testResults={testResults}
            providerStatus={providerStatus}
            maskedCredentials={maskedCredentials}
          />
        )}
      </div>

      {/* ── 底部保存栏 ── */}
      <div className="px-6 py-3.5 bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border-t border-[var(--border-secondary)] flex items-center justify-end gap-3">
        {saveStatus === "ok" && (
          <span className="text-[12px] text-green-500 flex items-center gap-1">
            <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
              <path d="M3 8.5l3.5 3.5 6.5-7" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            已保存
          </span>
        )}
        {saveStatus === "err" && (
          <span className="text-[12px] text-red-500">保存失败</span>
        )}
        <button
          onClick={handleSave}
          disabled={saving}
          className="btn-primary disabled:opacity-40"
        >
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
    </div>
  );
}

// ──────────── 常规 Tab ────────────

function GeneralTab({
  draft,
  onChange,
  onClipboardToggle,
  stats,
  statsLoading,
  runtimeSnapshot,
  runtimeDiag,
  providerStatus,
  onNavigate,
}: {
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onClipboardToggle: (enabled: boolean) => void;
  stats: StatsResult | null;
  statsLoading: boolean;
  runtimeSnapshot: RuntimeStatusSnapshot | null;
  runtimeDiag: RuntimeDiagnostics | null;
  providerStatus: Record<string, ProviderStatus>;
  onNavigate: (tab: TabId) => void;
}) {
  return (
    <div className="space-y-4">
      {/* 跳过向导的兜底提示：没有任何密钥时给出显眼的配置入口 */}
      <UnconfiguredBanner providerStatus={providerStatus} onNavigate={() => onNavigate("provider")} />

      {/* 外观 */}
      <SettingsSection label="外观">
        <SettingsRow label="目标语言">
          <select
            value={draft.target_lang}
            onChange={(e) => onChange("target_lang", e.target.value as AppConfig["target_lang"])}
            className="input-field w-40"
          >
            {SUPPORTED_LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.flag} {l.name}
              </option>
            ))}
          </select>
        </SettingsRow>
        <SettingsRow label="界面主题">
          <select
            value={draft.theme}
            onChange={(e) => {
              onChange("theme", e.target.value as AppConfig["theme"]);
              applyTheme(e.target.value);
            }}
            className="input-field w-32"
          >
            <option value="system">跟随系统</option>
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </select>
        </SettingsRow>
      </SettingsSection>

      {/* 行为 */}
      <SettingsSection label="行为">
        <SettingsRow label="开机自启动">
          <Toggle value={draft.auto_start} onChange={(v) => onChange("auto_start", v)} />
        </SettingsRow>
        <SettingsRow label="剪贴板监控" hint="选中文本并复制后自动弹出翻译">
          <Toggle value={draft.clipboard_monitor_enabled} onChange={onClipboardToggle} />
        </SettingsRow>
        <SettingsRow label="自动 Fallback" hint="主翻译源失败时自动切换备用">
          <Toggle value={draft.fallback_enabled} onChange={(v) => onChange("fallback_enabled", v)} />
        </SettingsRow>
        <SettingsRow label="历史记录上限" hint="50–1000 条">
          <input
            type="number"
            min={50}
            max={1000}
            value={draft.history_limit}
            onChange={(e) => {
              // 清空或非法输入回退默认 200；越界值钳制到有效区间
              const n = parseInt(e.target.value) || 200;
              onChange("history_limit", Math.min(1000, Math.max(50, n)));
            }}
            className="input-field w-24 text-center"
          />
        </SettingsRow>
      </SettingsSection>

      {/* 运行状态（计划第 33/50 节）：消费 Runtime 层快照与诊断计数 */}
      <RuntimeStatusSection snapshot={runtimeSnapshot} diag={runtimeDiag} />

      {/* 使用统计 */}
      <SettingsSection label="使用统计">
        {statsLoading ? (
          <div className="grid grid-cols-2 gap-2.5">
            {[1, 2, 3, 4].map((i) => (
              <div key={i} className="skeleton h-16 rounded-xl" />
            ))}
          </div>
        ) : stats ? (
          <div className="space-y-3">
            <div className="grid grid-cols-2 gap-2.5">
              <StatCard label="累计翻译" value={String(stats.total_records)} unit="次" />
              <StatCard label="累计字符" value={formatBigNum(stats.total_chars)} unit="字符" />
              <StatCard label="近 7 天" value={String(stats.last_7_days)} unit="次" />
              <StatCard label="近 30 天" value={String(stats.last_30_days)} unit="次" />
            </div>
            {Object.keys(stats.by_provider).length > 0 && (
              <div className="macos-card p-3.5 space-y-2.5">
                <p className="text-[11.5px] font-semibold text-[var(--text-secondary)] tracking-wide">
                  按翻译源
                </p>
                {(() => {
                  const entries = Object.entries(stats.by_provider).sort(([, a], [, b]) => b - a);
                  const max = Math.max(...entries.map(([, c]) => c), 1);
                  return (
                    <div className="space-y-2">
                      {entries.map(([provider, count]) => (
                        <div key={provider}>
                          <div className="flex items-center justify-between mb-1">
                            <span className="text-[12.5px] text-[var(--text-secondary)]">{provider}</span>
                            <span className="text-[12px] font-medium text-[var(--text-primary)] tabular-nums">
                              {count} 次
                            </span>
                          </div>
                          <div className="h-1 rounded-full bg-[var(--surface-tertiary)] overflow-hidden">
                            <div
                              className="h-full rounded-full"
                              style={{
                                width: `${Math.max((count / max) * 100, 4)}%`,
                                backgroundColor: "var(--system-blue)",
                                opacity: 0.7,
                              }}
                            />
                          </div>
                        </div>
                      ))}
                    </div>
                  );
                })()}
              </div>
            )}
          </div>
        ) : (
          <p className="text-[12.5px] text-[var(--text-secondary)]">统计数据加载失败</p>
        )}
      </SettingsSection>
    </div>
  );
}

// ──────────── 未配置提示横幅 ────────────

/**
 * 跳过首次向导的兜底提示（计划第 5 节的后续路径）：
 * 所有需要密钥的翻译源都未配置时，明确告诉用户当前在用 Google 兜底，
 * 并给出直达「翻译源」Tab 的入口 —— 不催促、不阻塞，但入口必须显眼。
 */
function UnconfiguredBanner({
  providerStatus,
  onNavigate,
}: {
  providerStatus: Record<string, ProviderStatus>;
  onNavigate: () => void;
}) {
  const statuses = Object.values(providerStatus);
  if (statuses.length === 0) return null; // 状态未就绪时不显示

  const anyConfigured = statuses.some((p) => p.requires_api_key && p.is_available);
  if (anyConfigured) return null;

  return (
    <div className="macos-card p-3.5 flex items-center justify-between gap-3 border-[var(--system-blue)]/30">
      <div className="flex items-start gap-2.5 min-w-0">
        <svg className="w-4 h-4 text-[var(--system-blue)] shrink-0 mt-0.5" viewBox="0 0 16 16" fill="none">
          <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.4" />
          <path d="M8 5v3.5M8 10.5v.01" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
        <div className="min-w-0">
          <p className="text-[12.5px] font-medium text-[var(--text-primary)]">
            还没有配置翻译源密钥
          </p>
          <p className="text-[11.5px] text-[var(--text-secondary)] mt-0.5 leading-snug">
            当前使用免配置的 Google 兜底源。配置一个密钥可获得更佳的翻译质量与更高的免费额度。
          </p>
        </div>
      </div>
      <button
        onClick={onNavigate}
        className="shrink-0 text-[12px] font-medium px-3 py-1.5 rounded-lg bg-[var(--system-blue)] text-white hover:bg-[#0071E3] active:scale-95 transition-all"
      >
        前往配置
      </button>
    </div>
  );
}

// ──────────── 运行状态面板 ────────────

const COMPONENT_LABELS: Record<"clipboard" | "network" | "popup" | "storage", string> = {
  clipboard: "剪贴板监控",
  network: "翻译服务",
  popup: "翻译浮窗",
  storage: "本地存储",
};

/** 组件状态 → 展示文案与色调（UI 不推测状态，只转译 Runtime 层给出的结论） */
function componentStateLabel(state: string): { text: string; tone: "ok" | "warn" | "muted" } {
  switch (state) {
    case "healthy":
      return { text: "正常", tone: "ok" };
    case "recovering":
      return { text: "正在恢复", tone: "warn" };
    case "degraded":
      return { text: "异常，自动处理中", tone: "warn" };
    case "disabled":
      return { text: "已停用", tone: "muted" };
    default:
      return { text: state, tone: "muted" };
  }
}

const RUNTIME_TONE_CLASS: Record<string, string> = {
  ok: "text-green-600 dark:text-green-500",
  warn: "text-amber-600 dark:text-amber-500",
  muted: "text-[var(--text-secondary)]",
};

function formatUptime(ms: number): string {
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟`;
  return `${(minutes / 60).toFixed(1)} 小时`;
}

function RuntimeStatusSection({
  snapshot,
  diag,
}: {
  snapshot: RuntimeStatusSnapshot | null;
  diag: RuntimeDiagnostics | null;
}) {
  const overall = snapshot
    ? snapshot.overall === "healthy"
      ? { text: "正常运行", tone: "ok" as const }
      : snapshot.overall === "recovering"
        ? { text: "正在恢复", tone: "warn" as const }
        : { text: "部分功能异常", tone: "warn" as const }
    : null;

  return (
    <SettingsSection label="运行状态">
      {/* 总体状态 + 诊断摘要 */}
      <div className="px-4 py-3 flex items-center justify-between gap-4">
        <div className="flex-1 min-w-0">
          <p className="text-[13px] font-medium text-[var(--text-primary)]">QuickTranslate</p>
          {diag && (
            <p className="text-[11.5px] text-[var(--text-secondary)] mt-0.5 tabular-nums leading-snug">
              已运行 {formatUptime(diag.uptime_ms)} · 翻译成功 {diag.translation_success_count} 次
              {diag.translation_failure_count > 0 && `（失败 ${diag.translation_failure_count}）`}
              {diag.fallback_count > 0 && ` · 兜底切换 ${diag.fallback_count} 次`}
              {diag.clipboard_restart_count > 0 && ` · 剪贴板自愈 ${diag.clipboard_restart_count} 次`}
            </p>
          )}
        </div>
        {overall && (
          <span
            className={["text-[12px] font-medium shrink-0", RUNTIME_TONE_CLASS[overall.tone]].join(" ")}
          >
            ● {overall.text}
          </span>
        )}
      </div>
      {/* 各组件状态 */}
      {snapshot &&
        (["clipboard", "network", "popup", "storage"] as const).map((key) => {
          const comp = snapshot[key];
          const label = componentStateLabel(comp.state);
          const isDisabled = comp.state === "disabled";
          return (
            <div
              key={key}
              className="px-4 py-2.5 flex items-center justify-between gap-4 border-t border-[var(--border-secondary)]"
            >
              <p
                className={[
                  "text-[12.5px]",
                  isDisabled ? "text-[var(--text-secondary)]" : "text-[var(--text-primary)]",
                ].join(" ")}
              >
                {COMPONENT_LABELS[key]}
              </p>
              <span
                className={["text-[11.5px] font-medium shrink-0", RUNTIME_TONE_CLASS[label.tone]].join(" ")}
              >
                {label.text}
                {comp.state !== "healthy" && comp.state !== "disabled" && comp.last_error_code
                  ? `（${comp.last_error_code}）`
                  : ""}
              </span>
            </div>
          );
        })}
    </SettingsSection>
  );
}

function StatCard({ label, value, unit }: { label: string; value: string; unit: string }) {
  return (
    <div className="macos-card p-3.5">
      <p className="text-[11.5px] font-medium text-[var(--text-secondary)] tracking-wide">{label}</p>
      <p className="text-[22px] font-semibold text-[var(--text-primary)] mt-0.5 tabular-nums leading-none" style={{ fontFamily: "var(--font-display)", letterSpacing: "-0.02em" }}>
        {value}
        <span className="text-[12px] font-normal text-[var(--text-secondary)] ml-1">{unit}</span>
      </p>
    </div>
  );
}

function formatBigNum(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return String(n);
}

// ──────────── 翻译源 Tab ────────────

function ProviderTab({
  draft,
  onChange,
  onTest,
  testingId,
  testResults,
  maskedCredentials,
  providerStatus,
}: {
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onTest: (id: string) => void;
  testingId: string | null;
  testResults: Record<string, boolean | null>;
  maskedCredentials: Record<string, boolean>;
  providerStatus: Record<string, ProviderStatus>;
}) {
  return (
    <div className="space-y-3">
      {/* 默认翻译源 */}
      <SettingsRow label="默认翻译源" hint="托盘菜单可快速切换">
        <select
          value={draft.provider}
          onChange={(e) => onChange("provider", e.target.value as AppConfig["provider"])}
          className="input-field w-40"
        >
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </SettingsRow>

      <div className="h-px bg-[var(--border-secondary)]" />

      <p className="text-[12px] text-[var(--text-secondary)] -mb-1">
        凭证配置（AES-256 本地加密存储）
      </p>

      {PROVIDERS.filter((p) => p.requiresApiKey).map((provider) => (
        <ProviderCard
          key={provider.id}
          provider={provider}
          draft={draft}
          onChange={onChange}
          onTest={() => onTest(provider.id)}
          isTesting={testingId === provider.id}
          testResult={testResults[provider.id]}
          maskedCredentials={maskedCredentials}
          runtimeStatus={providerStatus[provider.id]}
        />
      ))}

      {/* Google 无需配置 */}
      <div className="macos-card p-3.5">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">Google Translate</span>
            <span className="text-[11px] bg-[var(--surface-tertiary)] text-[var(--text-secondary)] px-1.5 py-0.5 rounded-full font-medium">
              无需配置
            </span>
          </div>
          <span className="text-[12px] text-green-600 dark:text-green-500 flex items-center gap-1">
            <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
              <path d="M3 8.5l3.5 3.5 6.5-7" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            始终可用
          </span>
        </div>
        <p className="text-[12px] text-[var(--text-secondary)] mt-1">用于 Fallback 兜底，无需任何配置</p>
      </div>
    </div>
  );
}

/**
 * 运行时状态的展示词表（计划第 28/51 节）。
 * 用户不需要看到开发者的 Open/HalfOpen 术语 ——
 * 「暂时不可用」「认证失败」这类话配上「将自动恢复」就够了。
 */
function providerStateLabel(
  status: ProviderStatus | undefined,
  hasCredentials: boolean
): { text: string; tone: "ok" | "warn" | "err" | "muted" } | null {
  if (!status || !hasCredentials) {
    return hasCredentials ? null : { text: "未配置", tone: "muted" };
  }
  switch (status.health_state) {
    case "healthy":
      return { text: "正常", tone: "ok" };
    case "half_open":
      return { text: "正在恢复…", tone: "warn" };
    case "open":
    case "degraded": {
      // 用最近一次错误的类别细分「暂时不可用」的原因
      switch (status.last_error_class) {
        case "auth":
          return { text: "认证失败，请检查凭证", tone: "err" };
        case "quota":
          return { text: "额度已用尽", tone: "err" };
        case "rate_limit":
          return { text: "请求过于频繁，稍后自动恢复", tone: "warn" };
        default:
          return { text: "暂时不可用，将自动恢复", tone: "warn" };
      }
    }
    default:
      return null;
  }
}

const STATE_TONE_CLASS: Record<string, string> = {
  ok: "text-green-500",
  warn: "text-amber-500",
  err: "text-red-500",
  muted: "text-[var(--text-secondary)]",
};

function ProviderCard({
  provider,
  draft,
  onChange,
  onTest,
  isTesting,
  testResult,
  maskedCredentials,
  runtimeStatus,
}: {
  provider: (typeof PROVIDERS)[number];
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onTest: () => void;
  isTesting: boolean;
  testResult?: boolean | null;
  maskedCredentials: Record<string, boolean>;
  runtimeStatus?: ProviderStatus;
}) {
  const [expanded, setExpanded] = useState(false);
  // 已配置 = 后端有记录（masked 非空）或本次输入框有值
  const hasCredentials = provider.credentialFields.every(
    (f) =>
      !!maskedCredentials[f.key] ||
      !!((draft as unknown as Record<string, string>)[f.key]?.trim())
  );

  return (
    <div className="macos-card overflow-hidden">
      {/* 折叠头 */}
      <button
        onClick={() => setExpanded((e) => !e)}
        className="w-full px-4 py-3 flex items-center justify-between hover:bg-[var(--hover-bg)] transition-colors text-left"
      >
        <div className="flex items-center gap-2.5">
          <span className="text-[13px] font-medium text-[var(--text-primary)]">{provider.name}</span>
          <span
            className="text-[11px] text-white px-1.5 py-px rounded-full font-semibold"
            style={{ backgroundColor: provider.dotColor }}
          >
            {provider.badge}
          </span>
          {testResult === undefined &&
            (() => {
              const label = providerStateLabel(runtimeStatus, hasCredentials);
              if (!label) return null;
              return (
                <span className={["text-[11.5px] font-medium", STATE_TONE_CLASS[label.tone]].join(" ")}>
                  {label.text}
                </span>
              );
            })()}
          {testResult === true && (
            <span className="text-[11.5px] font-medium text-green-600 dark:text-green-500 flex items-center gap-0.5">
              <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none">
                <path d="M3 8.5l3.5 3.5 6.5-7" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
              验证通过
            </span>
          )}
          {testResult === false && (
            <span className="text-[11.5px] font-medium text-red-500">验证失败</span>
          )}
        </div>
        <div className="flex items-center gap-2.5">
          <span className="text-[12px] text-[var(--text-secondary)]">{provider.freeQuota}</span>
          <svg
            className={["w-3.5 h-3.5 text-[var(--text-tertiary)] transition-transform duration-200", expanded ? "rotate-180" : ""].join(" ")}
            viewBox="0 0 16 16"
            fill="none"
          >
            <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </div>
      </button>

      {/* 展开内容 */}
      {expanded && (
        <div className="border-t border-[var(--border-secondary)] px-4 py-3.5 space-y-3 bg-[var(--surface-secondary)]/50">
          {/* 配置步骤 */}
          {provider.setupSteps.length > 0 && (
            <div className="bg-[var(--surface-tertiary)] rounded-lg p-3 space-y-1.5 border border-[var(--border-secondary)]">
              <p className="text-[11.5px] font-semibold text-[var(--text-secondary)] flex items-center gap-1">
                <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none">
                  <rect x="2" y="2" width="12" height="12" rx="2" stroke="currentColor" strokeWidth="1.3" />
                  <path d="M5 8h6M5 5.5h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
                </svg>
                获取步骤
              </p>
              {provider.setupSteps.map((s, i) => (
                <p key={i} className="text-[12px] text-[var(--text-secondary)] flex gap-1.5 leading-relaxed">
                  <span className="text-[var(--system-blue)] font-bold shrink-0 mt-0.5">{i + 1}.</span>
                  {s}
                </p>
              ))}
              {provider.setupUrl && (
                <button
                  type="button"
                  onClick={() => openUrl(provider.setupUrl).catch(console.error)}
                  className="text-[12px] text-[var(--system-blue)] hover:underline inline-flex items-center gap-0.5 mt-0.5 font-medium"
                >
                  前往控制台
                  <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
                    <path d="M4 2H2v8h8V8M6 2h4v4M8 2L4 6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                </button>
              )}
            </div>
          )}

          {/* 凭证输入 */}
          {provider.credentialFields.map((field) => (
            <div key={field.key}>
              <label className="text-[12px] font-medium text-[var(--text-secondary)] mb-1 block">
                {field.label}
              </label>
              <input
                type={field.type}
                value={((draft as unknown) as Record<string, string>)[field.key] ?? ""}
                onChange={(e) => onChange(field.key as keyof AppConfig, e.target.value as AppConfig[keyof AppConfig])}
                placeholder={
                  maskedCredentials[field.key]
                    ? "已配置（留空保持不变，输入新值覆盖）"
                    : field.placeholder
                }
                className="input-field w-full font-mono text-xs"
                autoComplete="off"
                spellCheck={false}
              />
            </div>
          ))}

          {/* 测试按钮 */}
          <button
            onClick={onTest}
            disabled={isTesting || !hasCredentials}
            className="text-[12.5px] font-medium px-3.5 py-1.5 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors text-[var(--text-primary)] flex items-center gap-1.5"
          >
            {isTesting ? (
              <>
                <div className="w-3.5 h-3.5 border border-current border-t-transparent rounded-full animate-spin" />
                测试中…
              </>
            ) : (
              <>
                <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
                  <path d="M13 3L3 8l10 5M13 3l-3 5H3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                测试连接
              </>
            )}
          </button>
        </div>
      )}
    </div>
  );
}

// ──────────── 通用子组件 ────────────

function SettingsSection({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-2">
      <p className="text-[12px] font-semibold text-[var(--text-secondary)] tracking-wide px-1">
        {label}
      </p>
      <div className="macos-card divide-y divide-[var(--border-secondary)] overflow-hidden">
        {children}
      </div>
    </div>
  );
}

function SettingsRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="px-4 py-3 flex items-center justify-between gap-4">
      <div className="flex-1 min-w-0">
        <p className="text-[13px] font-medium text-[var(--text-primary)]">{label}</p>
        {hint && <p className="text-[12px] text-[var(--text-secondary)] mt-0.5 leading-snug">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

// macOS 风格的 Toggle 开关
function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!value)}
      role="switch"
      aria-checked={value}
      className={[
        "relative w-9 h-5 rounded-full transition-colors duration-200",
        value ? "bg-[var(--system-blue)]" : "bg-[var(--surface-tertiary)]",
      ].join(" ")}
      style={{
        boxShadow: value
          ? "0 0 0 0 rgba(0, 122, 255, 0)"
          : "inset 0 0 0 1px rgba(0,0,0,0.08)",
      }}
    >
      <span
        className={[
          "absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full",
          "transition-transform duration-200",
          "shadow-[0_1px_3px_rgba(0,0,0,0.2)]",
          value ? "translate-x-4" : "translate-x-0",
        ].join(" ")}
      />
    </button>
  );
}
