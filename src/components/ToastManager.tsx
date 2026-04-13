// src/components/ToastManager.tsx
// 全局 Toast 管理器：监听 Rust 的 "toast" 事件，渲染 Toast 队列
// 挂载在每个窗口根节点，自动收集并堆叠展示所有通知

import { useState, useCallback, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { ToastItem, type ToastType } from "./Toast";

interface ToastEntry {
  id: number;
  message: string;
  type: ToastType;
  duration?: number;
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

  // 注册全局命令式 API
  useEffect(() => {
    _addToast = addToast;
    return () => { _addToast = null; };
  }, [addToast]);

  // 监听 Rust 广播的 "toast" 事件
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<{ message: string; kind: string; duration?: number }>("toast", (event) => {
      const { message, kind, duration } = event.payload;
      const validTypes: ToastType[] = ["error", "success", "warning", "info"];
      const type: ToastType = validTypes.includes(kind as ToastType)
        ? (kind as ToastType)
        : "info";
      addToast({ message, type, duration });
    }).then((fn) => { unlisten = fn; });

    return () => unlisten?.();
  }, [addToast]);

  if (toasts.length === 0) return null;

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
          onClose={removeToast}
        />
      ))}
    </div>
  );
}
