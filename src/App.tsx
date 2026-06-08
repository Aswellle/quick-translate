// src/App.tsx
// 窗口路由：popup | settings | history | onboarding

import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getConfig, checkOnboarding, openOnboardingWindow } from "@/lib/commands";
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

  // ── 独立居中向导窗口（不复用 popup webview）────────────────────
  // 仅 popup 窗口负责判断是否需要启动向导
  useEffect(() => {
    if (windowType !== "popup") return;
    checkOnboarding().then((needed) => {
      if (needed) {
        openOnboardingWindow().catch(console.error);
        // 切换到空闲 hash，防止 popup 窗口复用自身渲染 OnboardingWindow
        window.location.hash = "#idle";
      }
    }).catch(console.error);
  }, [windowType]);

  // ── 向导窗口关闭后重新检测 ─────────────────────────────────────
  // 当向导窗口关闭（×按钮），popup 窗口重新检测是否仍需向导
  useEffect(() => {
    if (windowType !== "popup") return;

    let unlisten: (() => void) | undefined;

    getCurrentWebviewWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) {
          // 窗口重新获得焦点时，重新检查是否需要向导
          checkOnboarding().then((needed) => {
            if (needed) {
              openOnboardingWindow().catch(console.error);
              window.location.hash = "#idle";
            }
          }).catch(console.error);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => unlisten?.();
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
