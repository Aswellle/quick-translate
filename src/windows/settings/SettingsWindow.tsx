// src/windows/settings/SettingsWindow.tsx
// 设置面板 — 完整版（支持 5 个翻译源凭证配置 + 快捷键 + 主题等）

import { useEffect, useState, useCallback } from "react";
import {
  getConfig, setConfigBatch, validateProvider, getAutostart,
  type AppConfig,
} from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import { SUPPORTED_LANGUAGES, PROVIDERS } from "@/lib/constants";
import { useConfigStore } from "@/stores/configStore";

type TabId = "general" | "provider";

export function SettingsWindow() {
  const { setConfig } = useConfigStore();
  const [activeTab, setActiveTab] = useState<TabId>("general");
  const [draft, setDraft] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState<"idle" | "ok" | "err">("idle");
  const [testingId, setTestingId] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, boolean | null>>({});

  // 加载配置
  useEffect(() => {
    Promise.all([getConfig(), getAutostart()])
      .then(([cfg, autoStart]) => {
        const merged = { ...cfg, auto_start: autoStart };
        setConfig(merged);
        setDraft(merged);
      })
      .catch((e) => toast("加载配置失败：" + String(e), "error"));
  }, [setConfig]);

  const updateDraft = useCallback(<K extends keyof AppConfig>(key: K, value: AppConfig[K]) => {
    setDraft(prev => prev ? { ...prev, [key]: value } : prev);
  }, []);

  // 保存
  const handleSave = useCallback(async () => {
    if (!draft) return;
    setSaving(true);
    setSaveStatus("idle");
    try {
      const updates: [string, string][] = [
        ["hotkey",            draft.hotkey],
        ["target_lang",       draft.target_lang],
        ["provider",          draft.provider],
        ["deepl_api_key",     draft.deepl_api_key],
        ["tencent_secret_id", draft.tencent_secret_id],
        ["tencent_secret_key",draft.tencent_secret_key],
        ["baidu_app_id",      draft.baidu_app_id],
        ["baidu_secret_key",  draft.baidu_secret_key],
        ["youdao_app_key",    draft.youdao_app_key],
        ["youdao_app_secret", draft.youdao_app_secret],
        ["auto_start",        String(draft.auto_start)],
        ["history_limit",     String(draft.history_limit)],
        ["theme",             draft.theme],
        ["fallback_enabled",  String(draft.fallback_enabled)],
      ];
      await setConfigBatch(updates);
      setConfig(draft);
      setSaveStatus("ok");
      toast("设置已保存", "success");
      setTimeout(() => setSaveStatus("idle"), 2500);
    } catch (err: unknown) {
      setSaveStatus("err");
      const msg = err instanceof Error ? err.message : JSON.stringify(err);
      if (msg.includes("hotkey") || msg.includes("快捷键")) {
        toast("快捷键冲突，请更换", "error");
      } else {
        toast("保存失败：" + msg, "error");
      }
    } finally {
      setSaving(false);
    }
  }, [draft, setConfig]);

  // 测试某个翻译源
  const handleTest = useCallback(async (providerId: string) => {
    setTestingId(providerId);
    setTestResults(prev => ({ ...prev, [providerId]: null }));
    try {
      // 先保存当前 draft 中该 provider 的凭证
      const providerMeta = PROVIDERS.find(p => p.id === providerId);
      if (providerMeta && draft) {
        const credUpdates: [string, string][] = providerMeta.credentialFields.map(
          f => [f.key, (draft as Record<string, string>)[f.key] ?? ""]
        );
        if (credUpdates.length > 0) await setConfigBatch(credUpdates);
      }
      const ok = await validateProvider(providerId);
      setTestResults(prev => ({ ...prev, [providerId]: ok }));
      if (!ok) toast(`${PROVIDERS.find(p => p.id === providerId)?.name} 凭证无效`, "warning");
      else toast("连接测试通过 ✓", "success");
    } catch {
      setTestResults(prev => ({ ...prev, [providerId]: false }));
      toast("连接测试失败，请检查网络", "error");
    } finally {
      setTestingId(null);
    }
  }, [draft]);

  if (!draft) {
    return (
      <div className="flex items-center justify-center h-full gap-2 text-[var(--text-muted)] text-sm">
        <div className="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" />
        加载中…
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen bg-white dark:bg-zinc-900 text-[var(--text-primary)] select-none">
      {/* 标题栏 */}
      <div className="px-6 py-4 border-b border-[var(--divider)]">
        <h1 className="text-base font-semibold">QuickTranslate 设置</h1>
      </div>

      {/* Tab */}
      <div className="flex gap-1 px-6 pt-3 border-b border-[var(--divider)]">
        {(["general", "provider"] as TabId[]).map(tab => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={[
              "px-4 py-2 text-sm rounded-t-md transition-colors",
              activeTab === tab
                ? "text-blue-600 dark:text-blue-400 font-medium border-b-2 border-blue-500"
                : "text-[var(--text-secondary)] hover:bg-[var(--hover-bg)]",
            ].join(" ")}
          >
            {tab === "general" ? "常规" : "翻译源"}
          </button>
        ))}
      </div>

      {/* 内容 */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        {activeTab === "general" && (
          <GeneralTab draft={draft} onChange={updateDraft} />
        )}
        {activeTab === "provider" && (
          <ProviderTab
            draft={draft}
            onChange={updateDraft}
            onTest={handleTest}
            testingId={testingId}
            testResults={testResults}
          />
        )}
      </div>

      {/* 底部 */}
      <div className="px-6 py-4 border-t border-[var(--divider)] flex items-center justify-end gap-3">
        {saveStatus === "ok"  && <span className="text-sm text-green-500">✓ 已保存</span>}
        {saveStatus === "err" && <span className="text-sm text-red-500">保存失败</span>}
        <button
          onClick={handleSave}
          disabled={saving}
          className="px-5 py-2 rounded-lg text-sm font-medium bg-blue-500 hover:bg-blue-600 text-white disabled:opacity-50 active:scale-95 transition-all"
        >
          {saving ? "保存中…" : "保存设置"}
        </button>
      </div>
    </div>
  );
}

// ──────────── 常规 Tab ────────────

function GeneralTab({
  draft,
  onChange,
}: {
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
}) {
  return (
    <div className="space-y-6">
      <SettingRow label="全局快捷键" hint="按此快捷键触发翻译；冲突时会弹出提示">
        <HotkeyInput value={draft.hotkey} onChange={v => onChange("hotkey", v)} />
      </SettingRow>
      <SettingRow label="目标语言" hint="翻译输出语言">
        <select value={draft.target_lang} onChange={e => onChange("target_lang", e.target.value)} className="input-field w-44">
          {SUPPORTED_LANGUAGES.map(l => (
            <option key={l.code} value={l.code}>{l.flag} {l.name}</option>
          ))}
        </select>
      </SettingRow>
      <SettingRow label="界面主题">
        <select value={draft.theme} onChange={e => onChange("theme", e.target.value)} className="input-field w-36">
          <option value="system">跟随系统</option>
          <option value="light">浅色</option>
          <option value="dark">深色</option>
        </select>
      </SettingRow>
      <SettingRow label="开机自启动">
        <Toggle value={draft.auto_start} onChange={v => onChange("auto_start", v)} />
      </SettingRow>
      <SettingRow label="历史记录上限" hint="50–1000 条，超出自动删除最旧记录">
        <input
          type="number" min={50} max={1000}
          value={draft.history_limit}
          onChange={e => onChange("history_limit", parseInt(e.target.value) || 200)}
          className="input-field w-24"
        />
      </SettingRow>
      <SettingRow label="自动 Fallback" hint="主翻译源失败时自动切换备用">
        <Toggle value={draft.fallback_enabled} onChange={v => onChange("fallback_enabled", v)} />
      </SettingRow>
    </div>
  );
}

// ──────────── 翻译源 Tab ────────────

function ProviderTab({
  draft,
  onChange,
  onTest,
  testingId,
  testResults,
}: {
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onTest: (id: string) => void;
  testingId: string | null;
  testResults: Record<string, boolean | null>;
}) {
  return (
    <div className="space-y-3">
      <SettingRow label="默认翻译源" hint="可在托盘菜单快速切换">
        <select
          value={draft.provider}
          onChange={e => onChange("provider", e.target.value)}
          className="input-field w-44"
        >
          {PROVIDERS.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
      </SettingRow>

      <div className="h-px bg-[var(--divider)] my-3" />
      <p className="text-xs text-[var(--text-muted)] pb-1">凭证配置（AES-256 本地加密存储）</p>

      {PROVIDERS.filter(p => p.requiresApiKey).map(provider => (
        <ProviderCard
          key={provider.id}
          provider={provider}
          draft={draft}
          onChange={onChange}
          onTest={() => onTest(provider.id)}
          isTesting={testingId === provider.id}
          testResult={testResults[provider.id]}
        />
      ))}

      {/* Google 无需配置 */}
      <div className="p-3.5 rounded-xl border border-[var(--divider)] bg-[var(--hover-bg)]">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">Google Translate</span>
            <span className="text-[10px] bg-gray-200 dark:bg-zinc-600 text-[var(--text-muted)] px-1.5 py-0.5 rounded-full">
              无需配置
            </span>
          </div>
          <span className="text-[11px] text-green-500">✓ 始终可用</span>
        </div>
        <p className="text-[11px] text-[var(--text-muted)] mt-1">用于 Fallback 兜底，无需任何配置</p>
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
}: {
  provider: typeof PROVIDERS[number];
  draft: AppConfig;
  onChange: <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => void;
  onTest: () => void;
  isTesting: boolean;
  testResult?: boolean | null;
}) {
  const [expanded, setExpanded] = useState(false);
  const hasCredentials = provider.credentialFields.every(
    f => !!(draft as Record<string, string>)[f.key]?.trim()
  );

  return (
    <div className="border border-[var(--divider)] rounded-xl overflow-hidden">
      {/* 折叠头 */}
      <button
        onClick={() => setExpanded(e => !e)}
        className="w-full px-4 py-3 flex items-center justify-between hover:bg-[var(--hover-bg)] transition-colors text-left"
      >
        <div className="flex items-center gap-2.5">
          <span className="text-sm font-medium">{provider.name}</span>
          <span className={`text-[10px] text-white px-1.5 py-px rounded-full ${provider.badgeColor}`}>
            {provider.badge}
          </span>
          {hasCredentials && testResult === undefined && (
            <span className="text-[10px] text-green-500">已配置</span>
          )}
          {testResult === true  && <span className="text-[10px] text-green-500">✓ 验证通过</span>}
          {testResult === false && <span className="text-[10px] text-red-400">✗ 验证失败</span>}
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-[var(--text-muted)]">{provider.freeQuota}</span>
          <svg
            className={["w-3.5 h-3.5 text-[var(--text-muted)] transition-transform", expanded ? "rotate-180" : ""].join(" ")}
            viewBox="0 0 16 16" fill="none"
          >
            <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </div>
      </button>

      {/* 展开内容 */}
      {expanded && (
        <div className="border-t border-[var(--divider)] px-4 py-3 space-y-3 bg-white dark:bg-zinc-900">
          {/* 配置步骤 */}
          {provider.setupSteps.length > 0 && (
            <div className="bg-blue-50 dark:bg-blue-900/10 rounded-lg p-3 space-y-1.5">
              <p className="text-[10px] font-semibold text-blue-600 dark:text-blue-400">获取步骤</p>
              {provider.setupSteps.map((s, i) => (
                <p key={i} className="text-[11px] text-[var(--text-secondary)] flex gap-1.5">
                  <span className="text-blue-400 font-bold shrink-0">{i + 1}.</span>{s}
                </p>
              ))}
              {provider.setupUrl && (
                <a href={provider.setupUrl} target="_blank" rel="noreferrer"
                  className="text-[11px] text-blue-500 hover:underline inline-block mt-0.5">
                  前往控制台 →
                </a>
              )}
            </div>
          )}

          {/* 凭证输入 */}
          {provider.credentialFields.map(field => (
            <div key={field.key}>
              <label className="text-xs text-[var(--text-muted)] mb-1 block">{field.label}</label>
              <input
                type={field.type}
                value={(draft as Record<string, string>)[field.key] ?? ""}
                onChange={e => onChange(field.key as keyof AppConfig, e.target.value as AppConfig[keyof AppConfig])}
                placeholder={field.placeholder}
                className="input-field w-full font-mono text-xs"
                autoComplete="off"
              />
            </div>
          ))}

          {/* 测试按钮 */}
          <button
            onClick={onTest}
            disabled={isTesting || !hasCredentials}
            className="text-xs px-3 py-1.5 rounded-lg border border-[var(--divider)] hover:bg-[var(--hover-bg)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors text-[var(--text-secondary)]"
          >
            {isTesting ? "测试中…" : "测试连接"}
          </button>
        </div>
      )}
    </div>
  );
}

// ──────────── 通用子组件 ────────────

function SettingRow({ label, hint, children }: {
  label: string; hint?: string; children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium">{label}</p>
        {hint && <p className="text-xs text-[var(--text-muted)] mt-0.5 leading-snug">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Toggle({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button onClick={() => onChange(!value)} role="switch" aria-checked={value}
      className={["relative w-9 h-5 rounded-full transition-colors duration-200", value ? "bg-blue-500" : "bg-gray-200 dark:bg-zinc-600"].join(" ")}
    >
      <span className={["absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow-sm transition-transform duration-200", value ? "translate-x-4" : "translate-x-0"].join(" ")} />
    </button>
  );
}

function HotkeyInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [recording, setRecording] = useState(false);
  const [keys, setKeys] = useState<string[]>([]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.preventDefault();
    if (!recording) return;
    const parts: string[] = [];
    if (e.ctrlKey)  parts.push("Ctrl");
    if (e.metaKey)  parts.push("Super");
    if (e.altKey)   parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    const ignored = ["Control", "Meta", "Alt", "Shift"];
    const key = e.key;
    if (!ignored.includes(key)) parts.push(key.length === 1 ? key.toUpperCase() : key);
    setKeys(parts);
    if (parts.length >= 2 && !ignored.includes(key)) {
      onChange(parts.join("+"));
      setRecording(false);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <input
        readOnly
        value={recording ? (keys.join("+") || "…") : value}
        onKeyDown={handleKeyDown}
        onBlur={() => { setRecording(false); setKeys([]); }}
        onClick={() => { setRecording(true); setKeys([]); }}
        className={["input-field w-40 cursor-pointer font-mono text-xs text-center", recording ? "ring-2 ring-blue-400 border-blue-400" : ""].join(" ")}
      />
      {recording && <span className="text-xs text-[var(--text-muted)] animate-pulse">录入中…</span>}
    </div>
  );
}
