// src/App.tsx
// 窗口路由：popup | settings | history | onboarding

import { useEffect, useState } from "react";
import { getConfig, checkOnboarding, openOnboardingWindow } from "@/lib/commands";
import { useConfigStore } from "@/stores/configStore";
import { useTheme } from "@/hooks/useTheme";
import { ToastManager } from "@/components/ToastManager";
import { PopupWindow } from "@/windows/popup/PopupWindow";
import { SettingsWindow } from "@/windows/settings/SettingsWindow";
import { HistoryWindow } from "@/windows/history/HistoryWindow";
import { OnboardingWindow } from "@/windows/onboarding/OnboardingWindow";

type WindowType = "popup" | "settings" | "history" | "onboarding";

/** settings 窗口可携带子路径：#settings/provider 直达翻译源配置 Tab */
function getWindowType(): WindowType {
  const hash = window.location.hash.replace("#", "");
  if (hash.startsWith("settings")) return "settings";
  if (hash === "history")    return "history";
  if (hash === "onboarding") return "onboarding";
  return "popup";
}

/** 从 hash 中解析 settings 的初始 Tab（托盘「配置翻译源…」的直达路径） */
function getSettingsInitialTab(): "general" | "provider" {
  const hash = window.location.hash.replace("#", "");
  return hash.startsWith("settings/provider") ? "provider" : "general";
}

export default function App() {
  const [windowType, setWindowType] = useState<WindowType>(getWindowType);
  const [settingsInitialTab, setSettingsInitialTab] = useState<"general" | "provider">(getSettingsInitialTab);
  const setConfig = useConfigStore((s) => s.setConfig);

  useEffect(() => {
    const handler = () => {
      setWindowType(getWindowType());
      setSettingsInitialTab(getSettingsInitialTab());
    };
    window.addEventListener("hashchange", handler);
    return () => window.removeEventListener("hashchange", handler);
  }, []);

  useEffect(() => {
    getConfig().then(setConfig).catch(console.error);
  }, [setConfig]);

  // ── 首启向导检测（仅 popup 窗口负责）────────────────────────────
  // 挂载时立即检查；1.5s 后兜底复查一次，覆盖两类向导打不开的场景：
  //   1. 应用刚启动、后端尚未就绪，首次 checkOnboarding 调用失败；
  //   2. 与 Rust 侧启动检测并发打开向导窗口时的标签冲突。
  // 注意 popup 窗口是 focusable(false)（浮窗不抢焦点），Windows 上
  // onFocusChanged 永远不会带 focused=true 触发，不能依赖获焦来重查。
  useEffect(() => {
    if (windowType !== "popup") return;
    let cancelled = false;

    const tryOpenWizard = async () => {
      try {
        const needed = await checkOnboarding();
        if (!needed || cancelled) return;
        await openOnboardingWindow();
        if (!cancelled) {
          // 切换到空闲 hash，防止 popup 窗口复用自身渲染 OnboardingWindow
          window.location.hash = "#idle";
        }
      } catch (err) {
        console.error("[onboarding] 检查/打开向导失败:", err);
      }
    };

    void tryOpenWizard();
    const fallback = setTimeout(() => { void tryOpenWizard(); }, 1500);

    return () => {
      cancelled = true;
      clearTimeout(fallback);
    };
  }, [windowType]);

  useTheme();

  return (
    <div className="w-full h-full">
      <ToastManager />
      {windowType === "popup"       && <PopupWindow />}
      {windowType === "settings"    && <SettingsWindow key={settingsInitialTab} initialTab={settingsInitialTab} />}
      {windowType === "history"     && <HistoryWindow />}
      {windowType === "onboarding"  && <OnboardingWindow />}
    </div>
  );
}
