// src/windows/onboarding/OnboardingWindow.tsx
// 首次使用引导向导：欢迎 → 选择翻译源 → 配置凭证 → 测试 → 完成

import { useState, useCallback } from "react";
import {
  setConfigBatch, validateProvider, completeOnboarding,
  type AppConfig,
} from "@/lib/commands";
import { PROVIDERS, type ProviderId } from "@/lib/constants";
import { toast } from "@/components/ToastManager";

type Step = "welcome" | "choose" | "configure" | "test" | "done";

interface WizardState {
  selectedProvider: ProviderId;
  credentials: Record<string, string>;
  testStatus: "idle" | "testing" | "ok" | "fail";
}

export function OnboardingWindow() {
  const [step, setStep] = useState<Step>("welcome");
  const [state, setState] = useState<WizardState>({
    selectedProvider: "tencent",
    credentials: {},
    testStatus: "idle",
  });

  const providerMeta = PROVIDERS.find(p => p.id === state.selectedProvider)!;

  // ── Step handlers ──

  const handleChooseProvider = useCallback((id: ProviderId) => {
    setState(prev => ({ ...prev, selectedProvider: id, credentials: {}, testStatus: "idle" }));
  }, []);

  const handleCredentialChange = useCallback((key: string, value: string) => {
    setState(prev => ({ ...prev, credentials: { ...prev.credentials, [key]: value } }));
  }, []);

  const handleTest = useCallback(async () => {
    setState(prev => ({ ...prev, testStatus: "testing" }));

    // 先保存凭证，再测试
    const updates: [string, string][] = Object.entries(state.credentials).map(
      ([k, v]) => [k, v]
    );
    // 也写入 provider 选择
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
    // 跳过：使用 Google（无需凭证）
    try {
      await setConfigBatch([["provider", "google"]]);
    } catch {}
    setStep("done");
  }, []);

  const handleFinish = useCallback(async () => {
    // 如果还没保存凭证，保存一次
    if (Object.keys(state.credentials).length > 0) {
      const updates: [string, string][] = Object.entries(state.credentials).map(
        ([k, v]) => [k, v]
      );
      updates.push(["provider", state.selectedProvider]);
      try { await setConfigBatch(updates); } catch {}
    }
    await completeOnboarding();
    // 向 App.tsx 发送完成信号（直接关闭窗口）
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    await getCurrentWebviewWindow().close();
  }, [state.credentials, state.selectedProvider]);

  return (
    <div className="flex flex-col h-screen bg-white dark:bg-zinc-900 text-[var(--text-primary)] select-none overflow-hidden">
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
          <StepDone
            provider={providerMeta}
            onFinish={handleFinish}
          />
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
    <div className="h-1 bg-[var(--divider)]">
      <div
        className="h-full bg-blue-500 transition-all duration-500 ease-out"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

// ──────────── Step 1: 欢迎 ────────────

function StepWelcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-full px-8 py-12 text-center gap-6">
      <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-blue-400 to-blue-600 flex items-center justify-center shadow-lg">
        <span className="text-4xl">⚡</span>
      </div>
      <div className="space-y-2">
        <h1 className="text-2xl font-bold text-[var(--text-primary)]">欢迎使用 QuickTranslate</h1>
        <p className="text-sm text-[var(--text-muted)] leading-relaxed max-w-xs">
          选中任意文本，按 <kbd className="px-1.5 py-0.5 rounded bg-[var(--hover-bg)] font-mono text-xs">Ctrl+Shift+D</kbd> 即可瞬间翻译
        </p>
      </div>
      <div className="grid grid-cols-3 gap-3 w-full max-w-xs text-xs text-[var(--text-muted)]">
        {[
          { icon: "🌍", text: "任意应用" },
          { icon: "⚡", text: "≤1.5 秒" },
          { icon: "🔒", text: "本地存储" },
        ].map(({ icon, text }) => (
          <div key={text} className="flex flex-col items-center gap-1 p-3 rounded-xl bg-[var(--hover-bg)]">
            <span className="text-xl">{icon}</span>
            <span>{text}</span>
          </div>
        ))}
      </div>
      <button onClick={onNext} className="btn-primary w-full max-w-xs">
        开始配置 →
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
    <div className="px-6 py-5 space-y-4">
      <div>
        <h2 className="text-lg font-semibold">选择翻译服务</h2>
        <p className="text-xs text-[var(--text-muted)] mt-1">
          所有服务均提供免费额度，可随时在设置中更换
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
                ? "border-blue-500 bg-blue-50/50 dark:bg-blue-900/10"
                : "border-[var(--divider)] hover:border-blue-200 dark:hover:border-blue-800 bg-white dark:bg-zinc-800/50",
            ].join(" ")}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div className={[
                  "w-2 h-2 rounded-full border-2 transition-colors",
                  selected === p.id ? "border-blue-500 bg-blue-500" : "border-[var(--text-muted)]",
                ].join(" ")} />
                <span className="text-sm font-medium">{p.name}</span>
                <span className={`text-[10px] text-white px-1.5 py-0.5 rounded-full ${p.badgeColor}`}>
                  {p.badge}
                </span>
              </div>
              <span className="text-[11px] text-[var(--text-muted)]">{p.freeQuota}</span>
            </div>
            <p className="text-[11px] text-[var(--text-muted)] mt-1 ml-[18px]">{p.description}</p>
          </button>
        ))}
      </div>

      <div className="flex gap-3 pt-2">
        <button onClick={onSkip} className="btn-ghost flex-1">
          暂时跳过
        </button>
        <button onClick={onNext} className="btn-primary flex-1">
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
  provider: typeof PROVIDERS[number];
  credentials: Record<string, string>;
  onChange: (key: string, value: string) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const allFilled = provider.credentialFields.every(f => !!credentials[f.key]?.trim());

  return (
    <div className="px-6 py-5 space-y-5">
      <div>
        <h2 className="text-lg font-semibold">配置 {provider.name}</h2>
        <p className="text-xs text-[var(--text-muted)] mt-1">
          凭证仅保存在本地，AES-256 加密存储
        </p>
      </div>

      {/* 配置步骤说明 */}
      {provider.setupSteps.length > 0 && (
        <div className="bg-blue-50 dark:bg-blue-900/20 rounded-xl p-4 space-y-2">
          <p className="text-xs font-semibold text-blue-700 dark:text-blue-300 flex items-center gap-1.5">
            <span>📋</span> 获取步骤
          </p>
          <ol className="space-y-1">
            {provider.setupSteps.map((step, i) => (
              <li key={i} className="text-xs text-[var(--text-secondary)] flex gap-2">
                <span className="shrink-0 w-4 h-4 rounded-full bg-blue-200 dark:bg-blue-700 text-blue-700 dark:text-blue-200 flex items-center justify-center text-[10px] font-bold">
                  {i + 1}
                </span>
                {step}
              </li>
            ))}
          </ol>
          {provider.setupUrl && (
            <a
              href={provider.setupUrl}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline mt-1"
            >
              前往控制台 →
            </a>
          )}
        </div>
      )}

      {/* 凭证输入框 */}
      <div className="space-y-3">
        {provider.credentialFields.map((field) => (
          <div key={field.key}>
            <label className="text-xs font-medium text-[var(--text-secondary)] mb-1 block">
              {field.label}
            </label>
            <input
              type={field.type}
              value={credentials[field.key] ?? ""}
              onChange={e => onChange(field.key, e.target.value)}
              placeholder={field.placeholder}
              className="input-field w-full font-mono text-sm"
              autoComplete="off"
              spellCheck={false}
            />
          </div>
        ))}
      </div>

      <div className="flex gap-3">
        <button onClick={onBack} className="btn-ghost flex-1">← 返回</button>
        <button
          onClick={onNext}
          disabled={!allFilled}
          className="btn-primary flex-1 disabled:opacity-40 disabled:cursor-not-allowed"
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
  provider: typeof PROVIDERS[number];
  testStatus: WizardState["testStatus"];
  onTest: () => void;
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <div className="px-6 py-5 space-y-6">
      <div>
        <h2 className="text-lg font-semibold">验证连接</h2>
        <p className="text-xs text-[var(--text-muted)] mt-1">
          发送一次测试请求确认凭证有效
        </p>
      </div>

      {/* 测试状态图示 */}
      <div className="flex flex-col items-center gap-4 py-6">
        <div className={[
          "w-16 h-16 rounded-full flex items-center justify-center text-3xl",
          "transition-all duration-300",
          testStatus === "idle"    ? "bg-[var(--hover-bg)]" :
          testStatus === "testing" ? "bg-blue-50 dark:bg-blue-900/20 animate-pulse" :
          testStatus === "ok"      ? "bg-green-50 dark:bg-green-900/20" :
                                     "bg-red-50 dark:bg-red-900/20",
        ].join(" ")}>
          {testStatus === "idle"    && "🔌"}
          {testStatus === "testing" && "⏳"}
          {testStatus === "ok"      && "✅"}
          {testStatus === "fail"    && "❌"}
        </div>
        <p className="text-sm text-[var(--text-secondary)] text-center">
          {testStatus === "idle"    && `点击「开始测试」验证 ${provider.name} 凭证`}
          {testStatus === "testing" && "正在连接，请稍候…"}
          {testStatus === "ok"      && "连接成功！凭证验证通过"}
          {testStatus === "fail"    && "连接失败，请检查凭证是否正确"}
        </p>
      </div>

      <div className="flex gap-3">
        <button onClick={onBack} className="btn-ghost flex-1">← 修改</button>
        {testStatus !== "ok" ? (
          <button
            onClick={onTest}
            disabled={testStatus === "testing"}
            className="btn-primary flex-1 disabled:opacity-50"
          >
            {testStatus === "testing" ? "测试中…" : "开始测试"}
          </button>
        ) : (
          <button onClick={onNext} className="btn-primary flex-1">
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
  provider: typeof PROVIDERS[number];
  onFinish: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full px-8 py-12 text-center gap-6">
      <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-green-400 to-green-600 flex items-center justify-center shadow-lg">
        <span className="text-4xl">🎉</span>
      </div>
      <div className="space-y-2">
        <h2 className="text-2xl font-bold">配置完成！</h2>
        <p className="text-sm text-[var(--text-muted)] leading-relaxed">
          已选择 <span className="font-semibold text-[var(--text-primary)]">{provider.name}</span> 作为翻译服务
        </p>
      </div>

      <div className="bg-[var(--hover-bg)] rounded-xl p-4 text-left space-y-2 w-full max-w-xs">
        <p className="text-xs font-semibold text-[var(--text-secondary)]">快速上手</p>
        {[
          "在任意应用中选中文字",
          "按 Ctrl+Shift+D（可自定义）",
          "翻译结果即刻弹出",
        ].map((tip, i) => (
          <div key={i} className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
            <span className="w-5 h-5 rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 text-[10px] font-bold flex items-center justify-center shrink-0">
              {i + 1}
            </span>
            {tip}
          </div>
        ))}
      </div>

      <button onClick={onFinish} className="btn-primary w-full max-w-xs">
        开始使用 QuickTranslate →
      </button>
    </div>
  );
}
