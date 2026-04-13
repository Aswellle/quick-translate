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
    let unlisten: UnlistenFn | undefined;

    listen<T>(event, handler).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [event, ...deps]);
}
