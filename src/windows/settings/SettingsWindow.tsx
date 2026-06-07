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
  setClipboardMonitorEnabled as invokeSetClipboardMonitor,
  type AppConfig,
  type StatsResult,
} from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import { SUPPORTED_LANGUAGES, PROVIDERS } from "@/lib/constants";
import { useConfigStore } from "@/stores/configStore";

// 需要特殊处理的凭证字段：getConfig() 返回 masked 值，不能直接写回
const CREDENTIAL_KEYS = PROVIDERS
  .filter((p) => p.requiresApiKey)
  .flatMap((p) => p.credentialFields.map((f) => f.key));

type TabId = "general" | "provider";

function applyThemeNow(theme: string) {
  const root = document.documentElement;
  if (theme === "dark") {
    root.classList.add("dark");
  } else if (theme === "light") {
    root.classList.remove("dark");
  } else {
    root.classList.toggle("dark", window.matchMedia("(prefers-color-scheme: dark)").matches);
  }
}

export function SettingsWindow() {
  const { setConfig } = useConfigStore();
  const [activeTab, setActiveTab] = useState<TabId>("general");
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
      applyThemeNow(draft.theme);
      setSaveStatus("ok");
      toast("设置已保存", "success");
      setTimeout(() => setSaveStatus("idle"), 2500);
    } catch (err: unknown) {
      setSaveStatus("err");
      const msg = err instanceof Error ? err.message : JSON.stringify(err);
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
      <div className="flex items-center justify-center h-full gap-2 text-sm text-[var(--text-tertiary)]">
        <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
        加载中…
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen bg-[var(--bg-secondary)] dark:bg-[var(--bg-primary)] text-[var(--text-primary)] select-none">
      {/* ── 标题栏 ── */}
      <div className="px-6 py-4 bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border-b border-[var(--border-secondary)]">
        <h1 className="text-base font-semibold tracking-wide" style={{ fontFamily: "var(--font-display)" }}>
          设置
        </h1>
      </div>

      {/* ── macOS Tab ── */}
      <div className="flex gap-1 px-6 pt-3 bg-[var(--bg-secondary)]">
        {(["general", "provider"] as TabId[]).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={[
              "px-4 py-1.5 text-[13px] font-medium rounded-lg transition-all duration-150",
              activeTab === tab
                ? "bg-[var(--surface-primary)] dark:bg-[var(--surface-tertiary)] text-[var(--text-primary)] shadow-sm"
                : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]",
            ].join(" ")}
          >
            {tab === "general" ? "常规" : "翻译源"}
          </button>
        ))}
      </div>

      {/* ── 内容区 ── */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        {activeTab === "general" && (
          <GeneralTab draft={draft} onChange={updateDraft} onClipboardToggle={handleClipboardToggle} stats={stats} statsLoading={statsLoading} />
        )}
        {activeTab === "provider" && (
          <ProviderTab
            draft={draft}
            onChange={updateDraft}
            onTest={handleTest}
            testingId={testingId}
            testResults={testResults}
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
}: {
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onClipboardToggle: (enabled: boolean) => void;
  stats: StatsResult | null;
  statsLoading: boolean;
}) {
  return (
    <div className="space-y-4">
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
              applyThemeNow(e.target.value);
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
            onChange={(e) => onChange("history_limit", parseInt(e.target.value) || 200)}
            className="input-field w-24 text-center"
          />
        </SettingsRow>
      </SettingsSection>

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
              <div className="macos-card p-3.5 space-y-2">
                <p className="text-[11px] font-semibold text-[var(--text-tertiary)] uppercase tracking-wide">
                  按翻译源
                </p>
                <div className="space-y-1.5">
                  {Object.entries(stats.by_provider)
                    .sort(([, a], [, b]) => b - a)
                    .map(([provider, count]) => (
                      <div key={provider} className="flex items-center justify-between">
                        <span className="text-[12px] text-[var(--text-secondary)]">
                          {provider}
                        </span>
                        <span className="text-[12px] font-medium text-[var(--text-primary)]">
                          {count} 次
                        </span>
                      </div>
                    ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          <p className="text-xs text-[var(--text-tertiary)]">统计数据加载失败</p>
        )}
      </SettingsSection>
    </div>
  );
}

function StatCard({ label, value, unit }: { label: string; value: string; unit: string }) {
  return (
    <div className="macos-card p-3.5">
      <p className="text-[10px] text-[var(--text-tertiary)] uppercase tracking-wide">{label}</p>
      <p className="text-xl font-semibold text-[var(--text-primary)] mt-0.5" style={{ fontFamily: "var(--font-display)" }}>
        {value}
        <span className="text-[11px] font-normal text-[var(--text-tertiary)] ml-1">{unit}</span>
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
}: {
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onTest: (id: string) => void;
  testingId: string | null;
  testResults: Record<string, boolean | null>;
  maskedCredentials: Record<string, boolean>;
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

      <p className="text-[11px] text-[var(--text-tertiary)] -mb-1">
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
        />
      ))}

      {/* Google 无需配置 */}
      <div className="macos-card p-3.5">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">Google Translate</span>
            <span className="text-[10px] bg-[var(--surface-tertiary)] text-[var(--text-tertiary)] px-1.5 py-0.5 rounded-full font-medium">
              无需配置
            </span>
          </div>
          <span className="text-[11px] text-green-500 flex items-center gap-1">
            <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
              <path d="M3 8.5l3.5 3.5 6.5-7" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            始终可用
          </span>
        </div>
        <p className="text-[11px] text-[var(--text-tertiary)] mt-1">用于 Fallback 兜底，无需任何配置</p>
      </div>
    </div>
  );
}

function ProviderCard({
  provider,
  draft,
  onChange,
  onTest,
  isTesting,
  testResult,
  maskedCredentials,
}: {
  provider: (typeof PROVIDERS)[number];
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onTest: () => void;
  isTesting: boolean;
  testResult?: boolean | null;
  maskedCredentials: Record<string, boolean>;
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
          <span className="text-sm font-medium">{provider.name}</span>
          <span
            className="text-[10px] text-white px-1.5 py-px rounded-full font-semibold"
            style={{ backgroundColor: provider.badgeColor.includes("purple") ? "#AF52DE" : provider.badgeColor.includes("orange") ? "#FF9500" : provider.badgeColor.includes("blue") ? "#007AFF" : "#5AC8FA" }}
          >
            {provider.badge}
          </span>
          {hasCredentials && testResult === undefined && (
            <span className="text-[10px] text-green-500">已配置</span>
          )}
          {testResult === true && (
            <span className="text-[10px] text-green-500 flex items-center gap-0.5">
              <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none">
                <path d="M3 8.5l3.5 3.5 6.5-7" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
              验证通过
            </span>
          )}
          {testResult === false && (
            <span className="text-[10px] text-red-500">验证失败</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-[var(--text-tertiary)]">{provider.freeQuota}</span>
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
            <div className="bg-[var(--surface-tertiary)] rounded-lg p-3 space-y-1.5">
              <p className="text-[10px] font-semibold text-[var(--text-secondary)] flex items-center gap-1">
                <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none">
                  <rect x="2" y="2" width="12" height="12" rx="2" stroke="currentColor" strokeWidth="1.3" />
                  <path d="M5 8h6M5 5.5h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
                </svg>
                获取步骤
              </p>
              {provider.setupSteps.map((s, i) => (
                <p key={i} className="text-[11px] text-[var(--text-secondary)] flex gap-1.5">
                  <span className="text-[var(--system-blue)] font-bold shrink-0 mt-0.5">{i + 1}.</span>
                  {s}
                </p>
              ))}
              {provider.setupUrl && (
                <button
                  type="button"
                  onClick={() => openUrl(provider.setupUrl).catch(console.error)}
                  className="text-[11px] text-[var(--system-blue)] hover:underline inline-flex items-center gap-0.5 mt-0.5"
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
              <label className="text-[11px] font-medium text-[var(--text-secondary)] mb-1 block">
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
            className="text-[12px] px-3.5 py-1.5 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors text-[var(--text-secondary)] flex items-center gap-1.5"
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
    <div className="space-y-2.5">
      <p className="text-[11px] font-semibold text-[var(--text-tertiary)] uppercase tracking-wider px-1">
        {label}
      </p>
      <div className="macos-card divide-y divide-[var(--border-secondary)]">
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
        <p className="text-sm font-medium text-[var(--text-primary)]">{label}</p>
        {hint && <p className="text-[11px] text-[var(--text-tertiary)] mt-0.5 leading-snug">{hint}</p>}
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
        "relative w-9 h-5 rounded-full transition-all duration-200",
        value ? "bg-[var(--system-blue)]" : "bg-[var(--surface-tertiary)]",
      ].join(" ")}
      style={{ boxShadow: value ? "0 0 0 0 rgba(0, 122, 255, 0)" : "inset 0 0 0 1px rgba(0,0,0,0.1)" }}
    >
      <span
        className={[
          "absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow-sm",
          "transition-transform duration-200",
          value ? "translate-x-4" : "translate-x-0",
        ].join(" ")}
        style={{ boxShadow: "0 1px 3px rgba(0,0,0,0.15)" }}
      />
    </button>
  );
}
