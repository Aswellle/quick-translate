// src/components/ToastManager.tsx
// 全局 Toast 管理器：监听 Rust 的 "toast" 事件，渲染 Toast 队列
// 挂载在每个窗口根节点，自动收集并堆叠展示所有通知

import { useState, useCallback, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { ToastItem, type ToastType, type ToastAction } from "./Toast";
import { translateFilteredText } from "@/lib/commands";

interface ToastEntry {
  id: number;
  message: string;
  type: ToastType;
  duration?: number;
  action?: ToastAction;
}

let _nextId = 1;

// 全局命令式 API（供前端 JS 主动触发）
let _addToast: ((entry: Omit<ToastEntry, "id">) => void) | null = null;

export function toast(message: string, type: ToastType = "info", duration?: number) {
  _addToast?.({ message, type, duration });
}

export function ToastManager() {
  const [toasts, setToasts] = useState<ToastEntry[]>([]);

  const addToast = useCallback((entry: Omit<ToastEntry, "id">) => {
    const id = _nextId++;
    setToasts((prev) => [...prev.slice(-4), { ...entry, id }]); // 最多同时显示 5 条
  }, []);

  const removeToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  // Toast 动作分发：目前唯一动作是敏感过滤拦截后的「仍然翻译」。
  // 只把**用户点击**这一件事转成后端命令调用 —— 命令本身消费的是
  // coordinator 暂存的那一条，前端拿不到也无法注入文本。
  const handleAction = useCallback((actionId: string) => {
    if (actionId === "translate_filtered") {
      translateFilteredText()
        .then((ok) => {
          if (!ok) toast("这条内容已失效，请重新复制", "info");
        })
        .catch((err) => {
          console.error("[ToastManager] 覆盖过滤翻译失败:", err);
          toast("翻译未能启动，请重试", "error");
        });
    }
  }, []);

  // 注册全局命令式 API
  useEffect(() => {
    _addToast = addToast;
    return () => { _addToast = null; };
  }, [addToast]);

  // 监听 Rust 广播的 "toast" 事件
  // cancelled 标志与 useTauriEvent 同一模式：cleanup 先于 Promise resolve
  // （StrictMode 双挂载）时首个监听器会被泄漏，toast 全部重复弹出
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    listen<{
      message: string;
      kind: string;
      duration?: number;
      action?: ToastAction;
    }>("toast", (event) => {
      const { message, kind, duration, action } = event.payload;
      const validTypes: ToastType[] = ["error", "success", "warning", "info"];
      const type: ToastType = validTypes.includes(kind as ToastType)
        ? (kind as ToastType)
        : "info";
      addToast({ message, type, duration, action });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [addToast]);

  // live region 必须常驻挂载：与首条内容同帧插入的区域读屏不可靠播报
  // （live region 的规则是「节点先存在，内容后变化」—— 此前空队列时
  // return null，第一条 toast 常被漏播）。error 条目自带 role="alert"，
  // 按规范以 assertive 播报，不被 polite 的空闲队列拖延。
  return (
    <div
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 flex flex-col gap-2 items-center pointer-events-none"
      aria-live="polite"
    >
      {toasts.map((t) => (
        <ToastItem
          key={t.id}
          id={t.id}
          message={t.message}
          type={t.type}
          duration={t.duration}
          action={t.action}
          onAction={handleAction}
          onClose={removeToast}
          role={t.type === "error" ? "alert" : undefined}
        />
      ))}
    </div>
  );
}
