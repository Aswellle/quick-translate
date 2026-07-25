// src/hooks/useTauriEvent.ts
// Tauri 事件监听封装 Hook（含自动清理）

import { useEffect } from "react";
import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 监听 Tauri 事件，组件卸载时自动解除监听
 *
 * @param event  Tauri 事件名（如 "translation-result"）
 * @param handler  事件处理函数
 * @param deps  额外依赖项（一般不需要传）
 */
export function useTauriEvent<T>(
  event: string,
  handler: EventCallback<T>,
  deps: unknown[] = []
) {
  useEffect(() => {
    // cancelled 标志修复注册竞态：listen() 是异步的，若 cleanup 在 Promise
    // resolve 前执行（StrictMode 双调用、或快速卸载），unlisten 仍为 undefined，
    // 解绑变成空操作 → 监听器永久泄漏，事件被重复处理。
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    listen<T>(event, handler).then((fn) => {
      if (cancelled) {
        fn(); // cleanup 已发生，立即解绑
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [event, ...deps]);
}
