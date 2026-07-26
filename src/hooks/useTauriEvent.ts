// src/hooks/useTauriEvent.ts
// Tauri 事件监听封装 Hook（含自动清理）

import { useEffect, useRef } from "react";
import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 监听 Tauri 事件，组件卸载时自动解除监听
 *
 * handler 通过 ref 转发：监听器只注册一次，但每次触发都调用**最新**的
 * handler。此前 handler 被挂载时的 effect 闭包捕获后不再更新 —— 调用方
 * 用 useCallback 依赖状态（如 PopupWindow 的 width）时，事件永远打到
 * 过期闭包上（F3：wide 模式下 loading 阶段宽度回弹 400px 的根因）。
 * 把 handler 加入 effect deps 会导致每次身份变化都退订/重订，ref 转发
 * 既保新鲜又免重订。
 *
 * @param event  Tauri 事件名（如 "translation-result"）
 * @param handler  事件处理函数（可安全捕获组件状态）
 * @param deps  额外依赖项（一般不需要传；仅当 event 名本身派生自状态时使用）
 */
export function useTauriEvent<T>(
  event: string,
  handler: EventCallback<T>,
  deps: unknown[] = []
) {
  const handlerRef = useRef(handler);
  useEffect(() => {
    handlerRef.current = handler;
  });

  useEffect(() => {
    // cancelled 标志修复注册竞态：listen() 是异步的，若 cleanup 在 Promise
    // resolve 前执行（StrictMode 双调用、或快速卸载），unlisten 仍为 undefined，
    // 解绑变成空操作 → 监听器永久泄漏，事件被重复处理。
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    listen<T>(event, (e) => handlerRef.current(e)).then((fn) => {
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
