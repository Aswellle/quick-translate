// src/hooks/useTheme.ts
// 主题管理 Hook — 实时响应版
//
// 两种触发路径：
// 1. configStore.config.theme 变化（本窗口自身修改）
// 2. Tauri "theme-changed" 事件（其他窗口/Rust 修改后广播）
//
// 两者均指向同一个 applyTheme() 函数，确保所有窗口无需刷新即可切换主题。

import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useConfigStore } from "@/stores/configStore";

/** 将 theme 字符串应用到 document.documentElement */
function applyTheme(theme: string) {
  const root = document.documentElement;

  if (theme === "dark") {
    root.classList.add("dark");
  } else if (theme === "light") {
    root.classList.remove("dark");
  } else {
    // "system"：跟随 OS 设置
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.classList.toggle("dark", prefersDark);
  }
}

export function useTheme() {
  const config = useConfigStore((s) => s.config);
  const setConfig = useConfigStore((s) => s.setConfig);
  const theme = config?.theme ?? "system";

  // 监听系统主题变化（仅 "system" 模式时生效）
  const systemMqRef = useRef<MediaQueryList | null>(null);

  useEffect(() => {
    // 清理旧的系统主题监听器
    if (systemMqRef.current) {
      systemMqRef.current.onchange = null;
      systemMqRef.current = null;
    }

    applyTheme(theme);

    if (theme === "system") {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      systemMqRef.current = mq;
      mq.onchange = (e) => {
        document.documentElement.classList.toggle("dark", e.matches);
      };
    }

    return () => {
      if (systemMqRef.current) {
        systemMqRef.current.onchange = null;
      }
    };
  }, [theme]);

  // 监听 Rust 广播的 "theme-changed" 事件
  // 这是跨窗口实时同步的关键：settings 窗口保存后，popup/history 窗口无需刷新
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<{ theme: string }>("theme-changed", (event) => {
      const newTheme = event.payload.theme;
      applyTheme(newTheme);

      // 同步到本窗口的 configStore（保证 useTheme 下次渲染也用新值）
      if (config) {
        setConfig({ ...config, theme: newTheme });
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => unlisten?.();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // 仅挂载一次，通过闭包外的 config ref 避免重复注册
}
