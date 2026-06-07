// src/windows/onboarding/OnboardingWindow.tsx
// 首次使用引导向导（macOS 简约明亮灵动风格）

import { useState, useCallback, useEffect } from "react";
import {
  setConfigBatch, validateProvider, completeOnboarding, openUrl,
} from "@/lib/commands";
import { PROVIDERS, type ProviderId } from "@/lib/constants";
import { toast } from "@/components/ToastManager";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

type Step = "welcome" | "choose" | "configure" | "test" | "done";

interface WizardState {
  selectedProvider: ProviderId;
  credentials: Record<string, string>;
  testStatus: "idle" | "testing" | "ok" | "fail";
}

export function OnboardingWindow() {
  const [step, setStep] = useState<Step>("welcome");
  const [state, setState] = useState<WizardState>({
    selectedProvider: "google",
    credentials: {},
    testStatus: "idle",
  });

  const providerMeta = PROVIDERS.find(p => p.id === state.selectedProvider)!;

  useEffect(() => {
    const win = getCurrentWebviewWindow();
    let refocusing = false;
    // 使用单个监听器并检查 payload，避免在获焦事件上也触发重焦逻辑
    const subPromise = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && !refocusing) {
        refocusing = true;
        win.setFocus().finally(() => {
          setTimeout(() => { refocusing = false; }, 150);
        });
      } else if (focused) {
        refocusing = false;
      }
    });
    return () => { subPromise.then(fn => fn()); };
  }, []);

  const handleChooseProvider = useCallback((id: ProviderId) => {
    setState(prev => ({ ...prev, selectedProvider: id, credentials: {}, testStatus: "idle" }));
  }, []);

  const handleCredentialChange = useCallback((key: string, value: string) => {
    setState(prev => ({ ...prev, credentials: { ...prev.credentials, [key]: value } }));
  }, []);

  const handleTest = useCallback(async () => {
    setState(prev => ({ ...prev, testStatus: "testing" }));
    const updates: [string, string][] = Object.entries(state.credentials).map(([k, v]) => [k, v]);
    updates.push(["provider", state.selectedProvider]);
    try {
      await setConfigBatch(updates);
      const ok = await validateProvider(state.selectedProvider);
      setState(prev => ({ ...prev, testStatus: ok ? "ok" : "fail" }));
      if (!ok) toast("凭证验证失败，请检查后重试", "error");
    } catch {
      setState(prev => ({ ...prev, testStatus: "fail" }));
      toast("连接测试失败，请检查网络或凭证", "error");
    }
  }, [state.credentials, state.selectedProvider]);

  const handleSkipProvider = useCallback(async () => {
    try { await setConfigBatch([["provider", "google"]]); } catch {}
    await completeOnboarding();
    // 必须先等待 onboarding 标记持久化，再关闭窗口
    // 否则 clipboard monitor 可能读到旧缓存导致划词翻译不触发
    await getCurrentWebviewWindow().close();
  }, []);

  const handleFinish = useCallback(async () => {
    if (Object.keys(state.credentials).length > 0) {
      const updates: [string, string][] = Object.entries(state.credentials).map(([k, v]) => [k, v]);
      updates.push(["provider", state.selectedProvider]);
      try { await setConfigBatch(updates); } catch {}
    }
    // 必须先等待 onboarding 标记持久化，再关闭窗口
    await completeOnboarding();
    await getCurrentWebviewWindow().close();
  }, [state.credentials, state.selectedProvider]);

  return (
    <div className="flex flex-col h-screen bg-[var(--surface-primary)] text-[var(--text-primary)] select-none overflow-hidden">
      {/* 进度条 */}
      <ProgressBar step={step} />

      {/* 步骤内容 */}
      <div className="flex-1 overflow-y-auto">
        {step === "welcome"   && <StepWelcome   onNext={() => setStep("choose")} />}
        {step === "choose"    && (
          <StepChoose
            selected={state.selectedProvider}
            onSelect={handleChooseProvider}
            onNext={() => setStep(providerMeta.requiresApiKey ? "configure" : "done")}
            onSkip={handleSkipProvider}
          />
        )}
        {step === "configure" && (
          <StepConfigure
            provider={providerMeta}
            credentials={state.credentials}
            onChange={handleCredentialChange}
            onBack={() => setStep("choose")}
            onNext={() => setStep("test")}
          />
        )}
        {step === "test" && (
          <StepTest
            provider={providerMeta}
            testStatus={state.testStatus}
            onTest={handleTest}
            onBack={() => setStep("configure")}
            onNext={() => setStep("done")}
          />
        )}
        {step === "done" && (
          <StepDone provider={providerMeta} onFinish={handleFinish} />
        )}
      </div>
    </div>
  );
}

// ──────────── 进度指示器 ────────────

const STEPS: Step[] = ["welcome", "choose", "configure", "test", "done"];

function ProgressBar({ step }: { step: Step }) {
  const idx = STEPS.indexOf(step);
  const pct = Math.round((idx / (STEPS.length - 1)) * 100);
  return (
    <div className="h-1 bg-[var(--surface-tertiary)]">
      <div
        className="h-full bg-[var(--system-blue)] transition-all duration-500 ease-out"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

// ──────────── Step 1: 欢迎 ────────────

function StepWelcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-full px-10 py-16 text-center gap-8">
      {/* App Icon */}
      <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-blue-50 to-indigo-100 dark:from-blue-950/40 dark:to-indigo-950/40 flex items-center justify-center shadow-macos">
        <svg className="w-10 h-10" viewBox="0 0 40 40" fill="none">
          <path d="M8 20C8 20 14 10 20 10C26 10 32 20 32 20C32 20 26 30 20 30C14 30 8 20 8 20Z" stroke="#007AFF" strokeWidth="2" strokeLinejoin="round" />
          <circle cx="20" cy="20" r="3" fill="#007AFF" />
        </svg>
      </div>

      <div className="space-y-2">
        <h1
          className="text-2xl font-bold text-[var(--text-primary)]"
          style={{ fontFamily: "var(--font-display)" }}
        >
          欢迎使用 QuickTranslate
        </h1>
        <p className="text-sm text-[var(--text-secondary)] leading-relaxed max-w-xs">
          复制任意文本，自动在光标旁弹出翻译结果。
          <br />
          无需快捷键，无需切换应用。
        </p>
      </div>

      {/* Feature pills */}
      <div className="flex items-center gap-2 flex-wrap justify-center">
        {[
          { icon: "🌍", label: "多语言" },
          { icon: "⚡", label: "≤1.5s" },
          { icon: "🔒", label: "本地存储" },
        ].map(({ icon, label }) => (
          <div
            key={label}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-[var(--surface-tertiary)] text-[12px] text-[var(--text-secondary)]"
          >
            <span>{icon}</span>
            <span>{label}</span>
          </div>
        ))}
      </div>

      <button onClick={onNext} className="btn-primary w-full max-w-xs text-[14px] py-2.5">
        开始配置
      </button>
    </div>
  );
}

// ──────────── Step 2: 选择翻译源 ────────────

function StepChoose({
  selected,
  onSelect,
  onNext,
  onSkip,
}: {
  selected: ProviderId;
  onSelect: (id: ProviderId) => void;
  onNext: () => void;
  onSkip: () => void;
}) {
  const selectedMeta = PROVIDERS.find(p => p.id === selected)!;
  return (
    <div className="px-6 py-6 space-y-4">
      <div>
        <h2 className="text-[17px] font-semibold" style={{ fontFamily: "var(--font-display)" }}>
          选择翻译服务
        </h2>
        <p className="text-[11px] text-[var(--text-tertiary)] mt-1">
          可随时在设置中更换，所有服务均提供免费额度
        </p>
      </div>

      <div className="space-y-2">
        {PROVIDERS.map((p) => (
          <button
            key={p.id}
            onClick={() => onSelect(p.id as ProviderId)}
            className={[
              "w-full text-left p-3.5 rounded-xl border-2 transition-all",
              selected === p.id
                ? "border-[var(--system-blue)] bg-blue-50/50 dark:bg-blue-950/20"
                : "border-[var(--border-secondary)] hover:border-blue-200/60 dark:hover:border-blue-700/40 bg-[var(--surface-primary)]",
            ].join(" ")}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div className={[
                  "w-4 h-4 rounded-full border-2 flex items-center justify-center transition-colors",
                  selected === p.id ? "border-[var(--system-blue)] bg-[var(--system-blue)]" : "border-[var(--text-tertiary)]",
                ].join(" ")}>
                  {selected === p.id && (
                    <div className="w-1.5 h-1.5 rounded-full bg-white" />
                  )}
                </div>
                <span className="text-[13px] font-medium">{p.name}</span>
              </div>
              <span className="text-[11px] text-[var(--text-tertiary)]">{p.freeQuota}</span>
            </div>
            <p className="text-[11px] text-[var(--text-tertiary)] mt-1 ml-6">{p.description}</p>
          </button>
        ))}
      </div>

      <div className="flex gap-2.5 pt-1">
        <button onClick={onSkip} className="btn-ghost flex-1 text-[13px]">
          暂时跳过
        </button>
        <button onClick={onNext} className="btn-primary flex-1 text-[13px]">
          {selectedMeta.requiresApiKey ? "配置凭证 →" : "直接使用 →"}
        </button>
      </div>
    </div>
  );
}

// ──────────── Step 3: 配置凭证 ────────────

function StepConfigure({
  provider,
  credentials,
  onChange,
  onBack,
  onNext,
}: {
  provider: (typeof PROVIDERS)[number];
  credentials: Record<string, string>;
  onChange: (key: string, value: string) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const allFilled = provider.credentialFields.every(f => !!credentials[f.key]?.trim());

  return (
    <div className="px-6 py-6 space-y-5">
      <div>
        <h2 className="text-[17px] font-semibold" style={{ fontFamily: "var(--font-display)" }}>
          配置 {provider.name}
        </h2>
        <p className="text-[11px] text-[var(--text-tertiary)] mt-1">
          凭证仅保存在本地，AES-256 加密存储
        </p>
      </div>

      {provider.setupSteps.length > 0 && (
        <div className="bg-[var(--surface-tertiary)] rounded-xl p-3.5 space-y-2">
          <p className="text-[11px] font-semibold text-[var(--text-secondary)] flex items-center gap-1.5">
            <svg className="w-3.5 h-3.5 text-[var(--system-blue)]" viewBox="0 0 16 16" fill="none">
              <rect x="2" y="2" width="12" height="12" rx="2" stroke="currentColor" strokeWidth="1.3" />
              <path d="M5 8h6M5 5.5h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
            </svg>
            获取步骤
          </p>
          <ol className="space-y-1.5">
            {provider.setupSteps.map((step, i) => (
              <li key={i} className="text-[11px] text-[var(--text-secondary)] flex gap-2">
                <span className="shrink-0 w-5 h-5 rounded-full bg-[var(--system-blue)]/10 text-[var(--system-blue)] text-[10px] font-bold flex items-center justify-center mt-0.5">
                  {i + 1}
                </span>
                {step}
              </li>
            ))}
          </ol>
          {provider.setupUrl && (
            <button
              onClick={() => openUrl(provider.setupUrl)}
              className="inline-flex items-center gap-1 text-[11px] text-[var(--system-blue)] hover:underline mt-1"
            >
              前往控制台
              <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
                <path d="M4 2H2v8h8V8M6 2h4v4M8 2L4 6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          )}
        </div>
      )}

      <div className="space-y-3">
        {provider.credentialFields.map((field) => (
          <div key={field.key}>
            <label className="text-[11px] font-medium text-[var(--text-secondary)] mb-1 block">
              {field.label}
            </label>
            <input
              type={field.type}
              value={credentials[field.key] ?? ""}
              onChange={e => onChange(field.key, e.target.value)}
              placeholder={field.placeholder}
              className="input-field w-full font-mono text-xs"
              autoComplete="off"
              spellCheck={false}
            />
          </div>
        ))}
      </div>

      <div className="flex gap-2.5">
        <button onClick={onBack} className="btn-ghost flex-1 text-[13px]">← 返回</button>
        <button
          onClick={onNext}
          disabled={!allFilled}
          className="btn-primary flex-1 text-[13px] disabled:opacity-40"
        >
          测试连接 →
        </button>
      </div>
    </div>
  );
}

// ──────────── Step 4: 连接测试 ────────────

function StepTest({
  provider,
  testStatus,
  onTest,
  onBack,
  onNext,
}: {
  provider: (typeof PROVIDERS)[number];
  testStatus: WizardState["testStatus"];
  onTest: () => void;
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <div className="px-6 py-8 space-y-8">
      <div>
        <h2 className="text-[17px] font-semibold" style={{ fontFamily: "var(--font-display)" }}>
          验证连接
        </h2>
        <p className="text-[11px] text-[var(--text-tertiary)] mt-1">
          发送一次测试请求确认凭证有效
        </p>
      </div>

      {/* 状态图标 */}
      <div className="flex flex-col items-center gap-4 py-4">
        <div className={[
          "w-16 h-16 rounded-full flex items-center justify-center text-2xl transition-all duration-300",
          testStatus === "idle"    ? "bg-[var(--surface-tertiary)]" :
          testStatus === "testing" ? "bg-blue-50 dark:bg-blue-950/30 animate-pulse" :
          testStatus === "ok"      ? "bg-green-50 dark:bg-green-950/30 shadow-glow-green" :
                                    "bg-red-50 dark:bg-red-950/30",
        ].join(" ")}>
          {testStatus === "idle"    && "🔌"}
          {testStatus === "testing" && (
            <svg className="w-7 h-7 text-[var(--system-blue)] animate-spin" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2" strokeDasharray="60" strokeDashoffset="20" />
            </svg>
          )}
          {testStatus === "ok"      && (
            <svg className="w-7 h-7 text-green-500" viewBox="0 0 24 24" fill="none">
              <path d="M5 12l5 5 9-9" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
          {testStatus === "fail"    && (
            <svg className="w-7 h-7 text-red-500" viewBox="0 0 24 24" fill="none">
              <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
          )}
        </div>
        <p className="text-[13px] text-[var(--text-secondary)] text-center leading-snug">
          {testStatus === "idle"    && `点击「开始测试」验证 ${provider.name} 凭证`}
          {testStatus === "testing" && "正在连接，请稍候…"}
          {testStatus === "ok"      && "连接成功！"}
          {testStatus === "fail"   && "连接失败，请检查凭证是否正确"}
        </p>
      </div>

      <div className="flex gap-2.5">
        <button onClick={onBack} className="btn-ghost flex-1 text-[13px]">← 修改</button>
        {testStatus !== "ok" ? (
          <button
            onClick={onTest}
            disabled={testStatus === "testing"}
            className="btn-primary flex-1 text-[13px] disabled:opacity-50"
          >
            {testStatus === "testing" ? "测试中…" : "开始测试"}
          </button>
        ) : (
          <button onClick={onNext} className="btn-primary flex-1 text-[13px]">
            完成配置 →
          </button>
        )}
      </div>
    </div>
  );
}

// ──────────── Step 5: 完成 ────────────

function StepDone({
  provider,
  onFinish,
}: {
  provider: (typeof PROVIDERS)[number];
  onFinish: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full px-10 py-16 text-center gap-7">
      {/* 成功图标 */}
      <div className="w-20 h-20 rounded-full bg-gradient-to-br from-green-50 to-emerald-100 dark:from-green-950/40 dark:to-emerald-950/40 flex items-center justify-center shadow-glow-green">
        <svg className="w-10 h-10 text-green-500" viewBox="0 0 40 40" fill="none">
          <path d="M8 20l8 8 16-16" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>

      <div className="space-y-2">
        <h2 className="text-2xl font-bold" style={{ fontFamily: "var(--font-display)" }}>
          配置完成！
        </h2>
        <p className="text-sm text-[var(--text-secondary)] leading-relaxed">
          已选择 <span className="font-semibold text-[var(--text-primary)]">{provider.name}</span> 作为翻译服务
        </p>
      </div>

      {/* 快速上手 */}
      <div className="w-full max-w-xs space-y-2">
        {[
          { n: 1, text: "在任意应用中选中文字" },
          { n: 2, text: "按下 Ctrl+C 复制" },
          { n: 3, text: "翻译浮窗自动弹出" },
        ].map(({ n, text }) => (
          <div key={n} className="flex items-center gap-3 text-[12px] text-[var(--text-secondary)]">
            <div className="w-6 h-6 rounded-full bg-[var(--system-blue)]/10 text-[var(--system-blue)] text-[11px] font-bold flex items-center justify-center shrink-0">
              {n}
            </div>
            {text}
          </div>
        ))}
      </div>

      <button onClick={onFinish} className="btn-primary w-full max-w-xs text-[14px] py-2.5">
        开始使用 →
      </button>
    </div>
  );
}
