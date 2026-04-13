// src/App.tsx
// 窗口路由：popup | settings | history | onboarding

import { useEffect, useState } from "react";
import { getConfig, checkOnboarding } from "@/lib/commands";
import { useConfigStore } from "@/stores/configStore";
import { useTheme } from "@/hooks/useTheme";
import { ToastManager } from "@/components/ToastManager";
import { PopupWindow } from "@/windows/popup/PopupWindow";
import { SettingsWindow } from "@/windows/settings/SettingsWindow";
import { HistoryWindow } from "@/windows/history/HistoryWindow";
import { OnboardingWindow } from "@/windows/onboarding/OnboardingWindow";

type WindowType = "popup" | "settings" | "history" | "onboarding";

function getWindowType(): WindowType {
  const hash = window.location.hash.replace("#", "");
  if (hash === "settings")   return "settings";
  if (hash === "history")    return "history";
  if (hash === "onboarding") return "onboarding";
  return "popup";
}

export default function App() {
  const [windowType, setWindowType] = useState<WindowType>(getWindowType);
  const setConfig = useConfigStore((s) => s.setConfig);

  useEffect(() => {
    const handler = () => setWindowType(getWindowType());
    window.addEventListener("hashchange", handler);
    return () => window.removeEventListener("hashchange", handler);
  }, []);

  useEffect(() => {
    getConfig().then(setConfig).catch(console.error);
  }, [setConfig]);

  // 启动时检查是否需要显示引导向导（仅 popup 窗口触发）
  useEffect(() => {
    if (windowType !== "popup") return;
    checkOnboarding().then((needed) => {
      if (needed) setWindowType("onboarding");
    }).catch(console.error);
  }, [windowType]);

  useTheme();

  return (
    <div className="w-full h-full">
      <ToastManager />
      {windowType === "popup"       && <PopupWindow />}
      {windowType === "settings"    && <SettingsWindow />}
      {windowType === "history"     && <HistoryWindow />}
      {windowType === "onboarding"  && <OnboardingWindow />}
    </div>
  );
}
