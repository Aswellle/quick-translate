// src/windows/onboarding/OnboardingWindow.tsx
// 首次使用引导向导（macOS 简约明亮灵动风格）
//
// 流程：欢迎 → 选择翻译源 → 配置密钥 → 验证连接 → 完成（撒花庆祝）。
// 设计原则：
//   - 每一步只做一件事，主按钮永远在右下/底部同一位置；
//   - 任何一步都可「暂时跳过」（Google 兜底源免配置，不把用户卡死在向导里）；
//   - 验证成功后自动进入完成页并撒花 —— 用户配好 key 的那一刻值得庆祝。

import { useState, useCallback, useEffect, useRef } from "react";
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

/** 撒花用色：取自应用系统色板，与 UI 语言一致 */
const CONFETTI_COLORS = ["#007AFF", "#5856D6", "#34C759", "#FFCC00", "#FF2D55", "#5AC8FA"];

/** 完成庆祝：中央主爆发 + 左右礼炮 + 顶部缓落，总时长约 1.2s。 */
async function celebrate() {
  // 尊重系统「减少动态效果」偏好
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

  // 动态加载：撒花只在这一刻用到，不进主 bundle
  const confetti = (await import("canvas-confetti")).default;
  const base: confetti.Options = {
    colors: CONFETTI_COLORS,
    zIndex: 100,
    disableForReducedMotion: true,
  };

  // 中央主爆发
  confetti({ ...base, particleCount: 90, spread: 75, startVelocity: 38, origin: { y: 0.62 } });
  // 左右礼炮
  setTimeout(() => {
    confetti({ ...base, particleCount: 45, angle: 60, spread: 60, origin: { x: 0, y: 0.75 } });
    confetti({ ...base, particleCount: 45, angle: 120, spread: 60, origin: { x: 1, y: 0.75 } });
  }, 220);
  // 顶部缓落补充
  setTimeout(() => {
    confetti({ ...base, particleCount: 55, spread: 110, startVelocity: 26, scalar: 0.85, origin: { y: 0.45 } });
  }, 480);
}

export function OnboardingWindow() {
  const [step, setStep] = useState<Step>("welcome");
  const [state, setState] = useState<WizardState>({
    selectedProvider: "google",
    credentials: {},
    testStatus: "idle",
  });
  const autoAdvanceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

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
    return () => {
      subPromise.then(fn => fn());
      if (autoAdvanceTimer.current) clearTimeout(autoAdvanceTimer.current);
    };
  }, []);

  const handleChooseProvider = useCallback((id: ProviderId) => {
    setState(prev => ({ ...prev, selectedProvider: id, credentials: {}, testStatus: "idle" }));
  }, []);

  const handleCredentialChange = useCallback((key: string, value: string) => {
    // 凭证一旦改动，旧的验证结果立即作废 —— 防止「改了 key 却沿用上次的有效标记」
    setState(prev => ({ ...prev, credentials: { ...prev.credentials, [key]: value }, testStatus: "idle" }));
  }, []);

  const handleTest = useCallback(async () => {
    setState(prev => ({ ...prev, testStatus: "testing" }));
    const updates: [string, string][] = Object.entries(state.credentials).map(([k, v]) => [k, v]);
    updates.push(["provider", state.selectedProvider]);
    try {
      await setConfigBatch(updates);
      const ok = await validateProvider(state.selectedProvider);
      setState(prev => ({ ...prev, testStatus: ok ? "ok" : "fail" }));
      if (!ok) {
        toast("凭证验证失败，请检查后重试", "error");
      } else {
        // 验证成功：稍作停留让用户看到绿色对勾，随后自动进入完成页撒花
        autoAdvanceTimer.current = setTimeout(() => setStep("done"), 750);
      }
    } catch {
      setState(prev => ({ ...prev, testStatus: "fail" }));
      toast("连接测试失败，请检查网络或凭证", "error");
    }
  }, [state.credentials, state.selectedProvider]);

  const handleSkipProvider = useCallback(async () => {
    try {
      await setConfigBatch([["provider", "google"]]);
      await completeOnboarding();
      // 必须先等待 onboarding 标记持久化，再关闭窗口
      // 否则 clipboard monitor 可能读到旧缓存导致划词翻译不触发
      await getCurrentWebviewWindow().close();
    } catch (e) {
      console.error("[onboarding] 跳过配置失败:", e);
      toast("保存配置失败，请重试", "error");
    }
  }, []);

  const handleFinish = useCallback(async () => {
    try {
      if (Object.keys(state.credentials).length > 0) {
        const updates: [string, string][] = Object.entries(state.credentials).map(([k, v]) => [k, v]);
        updates.push(["provider", state.selectedProvider]);
        await setConfigBatch(updates);
      }
      // 必须先等待 onboarding 标记持久化，再关闭窗口
      await completeOnboarding();
      await getCurrentWebviewWindow().close();
    } catch (e) {
      console.error("[onboarding] 完成配置失败:", e);
      toast("保存配置失败，请重试", "error");
    }
  }, [state.credentials, state.selectedProvider]);

  return (
    <div className="flex flex-col h-screen bg-[var(--surface-primary)] text-[var(--text-primary)] select-none overflow-hidden relative">
      {/* 顶部装饰渐变（浅色玻璃感，不参与交互） */}
      <div
        className="absolute inset-x-0 top-0 h-40 pointer-events-none opacity-70 dark:opacity-40"
        style={{
          background:
            "radial-gradient(600px 160px at 50% -40px, rgba(0,122,255,0.10), transparent 70%)," +
            "radial-gradient(400px 120px at 85% -30px, rgba(88,86,214,0.08), transparent 70%)",
        }}
      />

      {/* 步骤指示器 */}
      {(step !== "welcome" && step !== "done") && <StepDots step={step} />}

      {/* 步骤内容：key 驱动的入场动画，每次切步重新播放 */}
      <div className="flex-1 overflow-y-auto relative" key={step}>
        <div className="animate-step-in h-full">
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
    </div>
  );
}

// ──────────── 步骤指示器 ────────────

const STEP_ORDER: Array<{ id: Step; label: string }> = [
  { id: "choose", label: "选择服务" },
  { id: "configure", label: "填写密钥" },
  { id: "test", label: "验证" },
];

function StepDots({ step }: { step: Step }) {
  const idx = STEP_ORDER.findIndex(s => s.id === step);
  return (
    <div className="relative z-10 flex items-center justify-center gap-2 pt-4 pb-1">
      {STEP_ORDER.map((s, i) => {
        const done = i < idx;
        const active = i === idx;
        return (
          <div key={s.id} className="flex items-center gap-2">
            <div className="flex items-center gap-1.5">
              <div
                className={[
                  "w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold transition-all duration-300",
                  done
                    ? "bg-[var(--system-green)] text-white"
                    : active
                      ? "bg-[var(--system-blue)] text-white scale-110"
                      : "bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] text-[var(--text-secondary)]",
                ].join(" ")}
              >
                {done ? (
                  <svg className="w-2.5 h-2.5" viewBox="0 0 12 12" fill="none">
                    <path d="M2.5 6l2.5 2.5L9.5 3.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                ) : (
                  i + 1
                )}
              </div>
              <span className={[
                "text-[11px] transition-colors duration-300",
                active ? "text-[var(--text-primary)] font-medium" : "text-[var(--text-secondary)]",
              ].join(" ")}>
                {s.label}
              </span>
            </div>
            {i < STEP_ORDER.length - 1 && (
              <div className={[
                "w-6 h-px transition-colors duration-300",
                done ? "bg-[var(--system-green)]/60" : "bg-[var(--border-primary)]",
              ].join(" ")} />
            )}
          </div>
        );
      })}
    </div>
  );
}

// ──────────── Step 1: 欢迎 ────────────

function StepWelcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-full px-10 py-14 text-center gap-8">
      {/* App Icon：渐变光晕中的品牌眼形 */}
      <div className="relative">
        <div className="absolute -inset-3 rounded-3xl bg-gradient-to-br from-blue-100 via-indigo-50 to-transparent dark:from-blue-950/50 dark:via-indigo-950/30 dark:to-transparent blur-md" />
        <div className="relative w-20 h-20 rounded-2xl bg-white dark:bg-[var(--surface-secondary)] flex items-center justify-center shadow-macos border border-[var(--border-secondary)]">
          <svg className="w-10 h-10" viewBox="0 0 40 40" fill="none">
            <path d="M8 20C8 20 14 10 20 10C26 10 32 20 32 20C32 20 26 30 20 30C14 30 8 20 8 20Z" stroke="#007AFF" strokeWidth="2" strokeLinejoin="round" />
            <circle cx="20" cy="20" r="3" fill="#007AFF" />
          </svg>
        </div>
      </div>

      <div className="space-y-2.5">
        <h1
          className="text-[26px] font-bold text-[var(--text-primary)] tracking-tight"
          style={{ fontFamily: "var(--font-display)" }}
        >
          欢迎使用 QuickTranslate
        </h1>
        <p className="text-[13.5px] text-[var(--text-secondary)] leading-relaxed max-w-[300px]">
          复制任意文本，译文即刻出现在光标旁。
          <br />
          只需一步配置，之后的一切自动完成。
        </p>
      </div>

      {/* Feature pills */}
      <div className="flex items-center gap-2 flex-wrap justify-center">
        {[
          { icon: "🌍", label: "11 种语言" },
          { icon: "⚡", label: "秒级响应" },
          { icon: "🔒", label: "密钥本地加密" },
        ].map(({ icon, label }) => (
          <div
            key={label}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] text-[12px] font-medium text-[var(--text-secondary)]"
          >
            <span>{icon}</span>
            <span>{label}</span>
          </div>
        ))}
      </div>

      <div className="w-full max-w-xs space-y-2.5">
        <button onClick={onNext} className="btn-primary w-full text-[14px] py-2.5">
          开始配置
        </button>
        <p className="text-[11.5px] text-[var(--text-secondary)]">
          约 1 分钟 · 可随时跳过，稍后在设置中配置
        </p>
      </div>
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
    <div className="px-6 py-5 space-y-4 min-h-full flex flex-col">
      <div>
        <h2 className="text-[18px] font-semibold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>
          选择你的翻译服务
        </h2>
        <p className="text-[12px] text-[var(--text-secondary)] mt-1">
          先配一个常用的就够用，之后可在设置中添加更多
        </p>
      </div>

      <div className="space-y-2 flex-1">
        {PROVIDERS.map((p) => {
          const isSelected = selected === p.id;
          return (
            <button
              key={p.id}
              onClick={() => onSelect(p.id as ProviderId)}
              className={[
                "w-full text-left p-3 rounded-xl border-2 transition-all duration-150",
                isSelected
                  ? "border-[var(--system-blue)] bg-blue-50/60 dark:bg-blue-950/25 shadow-sm"
                  : "border-[var(--border-secondary)] hover:border-blue-200/70 dark:hover:border-blue-700/40 bg-[var(--surface-primary)] hover:bg-[var(--hover-bg)]",
              ].join(" ")}
            >
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2.5 min-w-0">
                  {/* 品牌色点：与设置页徽章同色 */}
                  <span
                    className="w-2.5 h-2.5 rounded-full shrink-0"
                    style={{
                      backgroundColor: p.badgeColor.includes("purple") ? "#AF52DE"
                        : p.badgeColor.includes("orange") ? "#FF9500"
                        : p.badgeColor.includes("blue") ? "#007AFF" : "#5AC8FA",
                    }}
                  />
                  <span className="text-[13.5px] font-medium text-[var(--text-primary)] truncate">{p.name}</span>
                  {p.id === "google" && (
                    <span className="text-[10.5px] font-medium bg-[var(--system-green)]/10 text-green-600 dark:text-green-500 px-1.5 py-px rounded-full shrink-0">
                      免配置
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <span className="text-[11.5px] text-[var(--text-secondary)]">{p.freeQuota}</span>
                  {/* 选中对勾 */}
                  <span className={[
                    "w-[18px] h-[18px] rounded-full flex items-center justify-center transition-all duration-150",
                    isSelected ? "bg-[var(--system-blue)] scale-100" : "bg-transparent scale-0",
                  ].join(" ")}>
                    <svg className="w-2.5 h-2.5 text-white" viewBox="0 0 12 12" fill="none">
                      <path d="M2.5 6l2.5 2.5L9.5 3.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </span>
                </div>
              </div>
              <p className="text-[12px] text-[var(--text-secondary)] mt-1 ml-[18px]">{p.description}</p>
            </button>
          );
        })}
      </div>

      <div className="flex gap-2.5 pt-1">
        <button onClick={onSkip} className="btn-ghost flex-1 text-[13px]">
          暂时跳过
        </button>
        <button onClick={onNext} className="btn-primary flex-1 text-[13px]">
          {selectedMeta.requiresApiKey ? "下一步 →" : "直接使用 →"}
        </button>
      </div>
    </div>
  );
}

// ──────────── Step 3: 配置密钥 ────────────

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
    <div className="px-6 py-5 space-y-4 min-h-full flex flex-col">
      <div>
        <h2 className="text-[18px] font-semibold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>
          配置 {provider.name}
        </h2>
        <p className="text-[12px] text-[var(--text-secondary)] mt-1 flex items-center gap-1">
          <svg className="w-3 h-3 text-[var(--system-green)]" viewBox="0 0 16 16" fill="none">
            <rect x="3" y="7" width="10" height="7" rx="1.5" stroke="currentColor" strokeWidth="1.4" />
            <path d="M5.5 7V5a2.5 2.5 0 015 0v2" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
          密钥仅保存在本机，AES-256 加密存储，不会上传
        </p>
      </div>

      {provider.setupSteps.length > 0 && (
        <div className="bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] rounded-xl p-3.5 space-y-2">
          <p className="text-[11.5px] font-semibold text-[var(--text-secondary)] flex items-center gap-1.5">
            <svg className="w-3.5 h-3.5 text-[var(--system-blue)]" viewBox="0 0 16 16" fill="none">
              <rect x="2" y="2" width="12" height="12" rx="2" stroke="currentColor" strokeWidth="1.3" />
              <path d="M5 8h6M5 5.5h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
            </svg>
            获取密钥（约 2 分钟）
          </p>
          <ol className="space-y-1.5">
            {provider.setupSteps.map((step, i) => (
              <li key={i} className="text-[12px] text-[var(--text-secondary)] flex gap-2 leading-relaxed">
                <span className="shrink-0 w-5 h-5 rounded-full bg-[var(--system-blue)]/10 text-[var(--system-blue)] text-[11px] font-bold flex items-center justify-center mt-0.5">
                  {i + 1}
                </span>
                {step}
              </li>
            ))}
          </ol>
          {provider.setupUrl && (
            <button
              onClick={() => openUrl(provider.setupUrl)}
              className="inline-flex items-center gap-1 text-[12px] font-medium text-[var(--system-blue)] hover:underline mt-1"
            >
              前往控制台获取
              <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
                <path d="M4 2H2v8h8V8M6 2h4v4M8 2L4 6" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          )}
        </div>
      )}

      <div className="space-y-3 flex-1">
        {provider.credentialFields.map((field) => (
          <div key={field.key}>
            <label className="text-[12px] font-medium text-[var(--text-secondary)] mb-1 block">
              {field.label}
            </label>
            <input
              type={field.type}
              value={credentials[field.key] ?? ""}
              onChange={e => onChange(field.key, e.target.value)}
              placeholder={field.placeholder}
              className="input-field w-full font-mono text-[13px]"
              autoComplete="off"
              spellCheck={false}
            />
          </div>
        ))}
      </div>

      <div className="flex gap-2.5 pt-1">
        <button onClick={onBack} className="btn-ghost flex-1 text-[13px]">← 返回</button>
        <button
          onClick={onNext}
          disabled={!allFilled}
          title={allFilled ? undefined : "请先填写全部密钥字段"}
          className="btn-primary flex-1 text-[13px] disabled:opacity-40"
        >
          测试连接 →
        </button>
      </div>
    </div>
  );
}

// ──────────── Step 4: 验证连接 ────────────

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
  const showSpinner = testStatus === "testing";
  return (
    <div className="px-6 py-8 space-y-8 h-full flex flex-col">
      <div className="text-center">
        <h2 className="text-[18px] font-semibold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>
          验证连接
        </h2>
        <p className="text-[12px] text-[var(--text-secondary)] mt-1">
          发送一次测试请求，确认密钥有效
        </p>
      </div>

      {/* 状态图标 */}
      <div className="flex flex-col items-center gap-5 py-2 flex-1 justify-center">
        <div className={[
          "w-20 h-20 rounded-full flex items-center justify-center transition-all duration-300",
          testStatus === "idle"    ? "bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)]" :
          showSpinner              ? "bg-blue-50 dark:bg-blue-950/30 animate-ring-pulse" :
          testStatus === "ok"      ? "bg-green-50 dark:bg-green-950/30 shadow-glow-green scale-110" :
                                     "bg-red-50 dark:bg-red-950/30",
        ].join(" ")}>
          {testStatus === "idle"    && "🔌"}
          {showSpinner && (
            <svg className="w-8 h-8 text-[var(--system-blue)] animate-spin" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2" strokeDasharray="60" strokeDashoffset="20" />
            </svg>
          )}
          {testStatus === "ok"      && (
            <svg className="w-9 h-9 text-green-500 animate-checkmark" viewBox="0 0 24 24" fill="none">
              <path d="M5 12l5 5 9-9" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          )}
          {testStatus === "fail"    && (
            <svg className="w-8 h-8 text-red-500" viewBox="0 0 24 24" fill="none">
              <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            </svg>
          )}
        </div>
        <div className="text-center space-y-1">
          <p className={[
            "text-[14px] font-medium transition-colors duration-300",
            testStatus === "ok" ? "text-green-600 dark:text-green-500" : "text-[var(--text-primary)]",
          ].join(" ")}>
            {testStatus === "idle"    && "一切就绪"}
            {showSpinner              && `正在连接 ${provider.name}…`}
            {testStatus === "ok"      && "连接成功！"}
            {testStatus === "fail"   && "验证未通过"}
          </p>
          <p className="text-[12px] text-[var(--text-secondary)] leading-snug max-w-[260px]">
            {testStatus === "idle"    && `将使用你的 ${provider.name} 密钥发送一次极短的测试翻译`}
            {showSpinner              && "通常 1~2 秒完成"}
            {testStatus === "ok"      && `${provider.name} 已准备就绪，马上就好`}
            {testStatus === "fail"   && "请检查密钥是否输入正确，或稍后重试"}
          </p>
        </div>
      </div>

      <div className="flex gap-2.5">
        <button onClick={onBack} className="btn-ghost flex-1 text-[13px]" disabled={showSpinner}>← 修改密钥</button>
        {testStatus !== "ok" ? (
          <button
            onClick={onTest}
            disabled={showSpinner}
            className="btn-primary flex-1 text-[13px] disabled:opacity-50"
          >
            {showSpinner ? "测试中…" : testStatus === "fail" ? "重试" : "开始测试"}
          </button>
        ) : (
          <button onClick={onNext} className="btn-primary flex-1 text-[13px]">
            完成 →
          </button>
        )}
      </div>
    </div>
  );
}

// ──────────── Step 5: 完成（撒花） ────────────

function StepDone({
  provider,
  onFinish,
}: {
  provider: (typeof PROVIDERS)[number];
  onFinish: () => void;
}) {
  // 进入完成页即撒花：只放一次
  const celebrated = useRef(false);
  useEffect(() => {
    if (celebrated.current) return;
    celebrated.current = true;
    void celebrate();
  }, []);

  return (
    <div className="flex flex-col items-center justify-center h-full px-10 py-12 text-center gap-6">
      {/* 成功图标 */}
      <div className="w-20 h-20 rounded-full bg-gradient-to-br from-green-50 to-emerald-100 dark:from-green-950/40 dark:to-emerald-950/40 flex items-center justify-center shadow-glow-green">
        <svg className="w-10 h-10 text-green-500" viewBox="0 0 40 40" fill="none">
          <path d="M8 20l8 8 16-16" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>

      <div className="space-y-2">
        <h2 className="text-[26px] font-bold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>
          一切就绪！
        </h2>
        <p className="text-[13.5px] text-[var(--text-secondary)] leading-relaxed">
          <span className="font-semibold text-[var(--text-primary)]">{provider.name}</span> 已配置完成
          {provider.id === "google" ? "，开箱即用" : ""}
          ，从现在起复制即翻译。
        </p>
      </div>

      {/* 快速上手 */}
      <div className="w-full max-w-[280px] space-y-2 text-left">
        {[
          { n: 1, text: "在任意应用中选中文字" },
          { n: 2, text: "按下 Ctrl+C 复制" },
          { n: 3, text: "译文浮窗在光标旁弹出" },
        ].map(({ n, text }) => (
          <div key={n} className="flex items-center gap-3 text-[12.5px] text-[var(--text-secondary)]">
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
