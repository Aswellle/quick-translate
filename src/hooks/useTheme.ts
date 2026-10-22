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
export function applyTheme(theme: string) {
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

/**
 * 应用内「减少透明效果」开关 → <html class="reduce-transparency">。
 *
 * 为什么要用 class 而不是直接依赖媒体查询：WKWebView 不支持
 * `prefers-reduced-transparency`（WebKit bug 175497），macOS 上媒体查询
 * 恒为 false，系统偏好读不到。这个 class 是给这类平台的确定性回退。
 *
 * 与媒体查询是**叠加**关系：系统偏好命中时 CSS 里的 @media 块照样生效，
 * 这里只负责多挂一条应用内途径。所以传 false 时只移除 class，不会
 * 影响系统偏好的响应。
 */
export function applyReduceTransparency(enabled: boolean) {
  document.documentElement.classList.toggle("reduce-transparency", enabled);
}

export function useTheme() {
  const config = useConfigStore((s) => s.config);
  const theme = config?.theme ?? "system";
  const reduceTransparency = config?.reduce_transparency ?? false;

  // 监听系统主题变化（仅 "system" 模式时生效）
  const systemMqRef = useRef<MediaQueryList | null>(null);
  const systemPollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // 应用内「减少透明效果」开关：配置变化时同步到 <html> 的 class。
  // 与系统媒体查询叠加，互不覆盖（见 applyReduceTransparency 的说明）。
  useEffect(() => {
    applyReduceTransparency(reduceTransparency);
  }, [reduceTransparency]);

  useEffect(() => {
    // 清理旧的系统主题监听器
    if (systemMqRef.current) {
      systemMqRef.current.onchange = null;
      systemMqRef.current = null;
    }
    if (systemPollRef.current) {
      clearInterval(systemPollRef.current);
      systemPollRef.current = null;
    }

    applyTheme(theme);

    if (theme === "system") {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      systemMqRef.current = mq;
      mq.onchange = (e) => {
        document.documentElement.classList.toggle("dark", e.matches);
      };
      // WebView2 的 matchMedia change 事件在部分运行时上不可靠
      // （prefers-color-scheme 动态刷新是已知薄弱点，实机 2026-10-20
      // 验证「跟随系统」下切系统主题浮窗纹丝不动）。轮询重读兜底：
      // applyTheme("system") 幂等 —— 明暗没变时 toggle 不产生 DOM
      // mutation，每 2s 的开销只是一次 matchMedia 查询。
      systemPollRef.current = setInterval(() => applyTheme("system"), 2000);
    }

    return () => {
      if (systemMqRef.current) {
        systemMqRef.current.onchange = null;
        systemMqRef.current = null;
      }
      if (systemPollRef.current) {
        clearInterval(systemPollRef.current);
        systemPollRef.current = null;
      }
    };
  }, [theme]);

  // 监听 Rust 广播的 "theme-changed" 事件
  // 这是跨窗口实时同步的关键：settings 窗口保存后，popup/history 窗口无需刷新。
  // cancelled 标志与 useTauriEvent 同一模式：cleanup 先于 Promise resolve
  // （StrictMode 双挂载）时首个监听器会泄漏，主题事件被重复应用
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    listen<{ theme: string }>("theme-changed", (event) => {
      const newTheme = event.payload.theme;
      applyTheme(newTheme);

      // 同步到本窗口的 configStore（保证 useTheme 下次渲染也用新值）。
      // 用 getState() 取最新快照 —— 挂载时闭包捕获的 config 尚未异步加载，
      // 会让这段同步逻辑永久失效。
      const { config: current, setConfig: set } = useConfigStore.getState();
      if (current) {
        set({ ...current, theme: newTheme });
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // 监听「减少透明效果」开关的跨窗口广播。
  //
  // 浮窗是长期驻留窗口，读的是自己那份 configStore。设置页保存只更新
  // 设置窗口自己的 store，没有这条监听的话，用户点完开关浮窗毫无反应，
  // 要等到下次整体重载才生效。
  //
  // 直接挂 class 而不只更新 store：这个 hook 在浮窗里由 useTheme 驱动，
  // 但 PopupWindow 的材质同步走的是另一条 effect —— 两边都要立刻生效。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    listen<{ enabled: boolean }>("reduce-transparency-changed", (event) => {
      const enabled = event.payload.enabled;
      applyReduceTransparency(enabled);
      const { config: current, setConfig: set } = useConfigStore.getState();
      if (current) {
        set({ ...current, reduce_transparency: enabled });
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
