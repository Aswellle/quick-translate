// src/hooks/useVisibleRefresh.ts
// 隐藏窗口的刷新降级。
//
// 历史窗口与设置窗口都是 hide 而非销毁：组件树与事件监听器永远活着，
// 「每条新翻译都全量重查一遍」对看不见的 UI 是纯查询税。事件到达时
// 窗口不可见就只记一笔 dirty，转可见（visibilitychange）时统一补刷。
// 若宿主窗口的隐藏没有传导到 document.hidden（平台差异），本 hook
// 退化为「每次都刷新」——与旧行为一致，不会引入过期数据。

import { useEffect, useRef } from "react";

/** 返回值可直接用作事件 handler：可见时立即刷新，隐藏时挂起到转可见 */
export function useVisibleRefresh(refresh: () => void): () => void {
  const pendingRef = useRef(false);
  const refreshRef = useRef(refresh);

  useEffect(() => {
    refreshRef.current = refresh;
  });

  useEffect(() => {
    const onVisibility = () => {
      if (!document.hidden && pendingRef.current) {
        pendingRef.current = false;
        refreshRef.current();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () =>
      document.removeEventListener("visibilitychange", onVisibility);
  }, []);

  return () => {
    if (document.hidden) {
      pendingRef.current = true;
      return;
    }
    refreshRef.current();
  };
}
