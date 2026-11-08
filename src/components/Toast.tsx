// src/components/Toast.tsx
// 单条 Toast 组件（macOS 风格 — 灵动有活力）

import { useEffect, useState } from "react";

export type ToastType = "error" | "success" | "warning" | "info";

/** Toast 上的动作按钮（Rust 侧 ToastAction 的前端镜像） */
export interface ToastAction {
  /** 动作标识，目前只有 "translate_filtered" */
  id: string;
  label: string;
}

interface ToastProps {
  id: number;
  message: string;
  type?: ToastType;
  duration?: number;
  action?: ToastAction;
  onAction?: (id: string) => void;
  onClose: (id: number) => void;
}

const TOAST_ICONS: Record<ToastType, React.ReactNode> = {
  error: (
    <svg className="w-4 h-4 shrink-0" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.3" />
      <path d="M8 5v3M8 10.5v.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  success: (
    <svg className="w-4 h-4 shrink-0" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.3" />
      <path d="M5 8l2 2 4-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  warning: (
    <svg className="w-4 h-4 shrink-0" viewBox="0 0 16 16" fill="none">
      <path d="M8 2L14.5 13H1.5L8 2z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
      <path d="M8 6v3M8 11v.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  info: (
    <svg className="w-4 h-4 shrink-0" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.3" />
      <path d="M8 7v4M8 5.5V5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
};

const TOAST_COLORS: Record<ToastType, string> = {
  error:   "text-red-500    dark:text-red-400",
  success: "text-green-500 dark:text-green-400",
  warning: "text-orange-500 dark:text-orange-400",
  info:    "text-blue-500  dark:text-blue-400",
};

export function ToastItem({
  id,
  message,
  type = "info",
  duration = 3500,
  action,
  onAction,
  onClose,
}: ToastProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const show = requestAnimationFrame(() => setVisible(true));
    let fade: ReturnType<typeof setTimeout> | null = null;
    const hide = setTimeout(() => {
      setVisible(false);
      fade = setTimeout(() => onClose(id), 250);
    }, duration);
    return () => {
      cancelAnimationFrame(show);
      clearTimeout(hide);
      if (fade) clearTimeout(fade);
    };
  }, [id, duration, onClose]);

  // 带动作按钮的 toast 需要指针事件：容器是 pointer-events-none 的，
  // 按钮自身必须显式恢复 auto，否则点击会穿透到底层窗口。
  const handleAction = () => {
    if (!action) return;
    onAction?.(action.id);
    // 点击动作后立即收起这一条：它的语义是「对刚才那次拦截作出决定」，
    // 决定已作出，提示继续停留只会挡住结果浮窗。
    onClose(id);
  };

  return (
    <div
      className={[
        "flex items-center gap-2.5 px-3.5 py-2.5 rounded-xl max-w-xs pointer-events-auto",
        "bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)]",
        "border border-[var(--border-secondary)]",
        "shadow-macos",
        "transition-all duration-200 ease-out",
        visible ? "opacity-100 translate-y-0 scale-100" : "opacity-0 translate-y-2 scale-95",
        TOAST_COLORS[type],
      ].join(" ")}
    >
      {TOAST_ICONS[type]}
      <span className="text-[var(--text-primary)] text-[12px] leading-snug">{message}</span>
      {action && (
        <button
          type="button"
          onClick={handleAction}
          className="ml-1 shrink-0 px-2 py-0.5 rounded-md text-[11.5px] font-medium bg-[var(--system-blue)] hover:brightness-110 text-white transition-all active:scale-95"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
