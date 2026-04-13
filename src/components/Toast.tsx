// src/components/Toast.tsx
// 单条 Toast 组件（带淡入淡出动画）

import { useEffect, useState } from "react";

export type ToastType = "error" | "success" | "warning" | "info";

interface ToastProps {
  id: number;
  message: string;
  type?: ToastType;
  duration?: number;
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
  error:   "text-red-500   dark:text-red-400",
  success: "text-green-500 dark:text-green-400",
  warning: "text-amber-500 dark:text-amber-400",
  info:    "text-blue-500  dark:text-blue-400",
};

export function ToastItem({ id, message, type = "error", duration = 3500, onClose }: ToastProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    // 下一帧触发淡入
    const show = requestAnimationFrame(() => setVisible(true));
    const hide = setTimeout(() => {
      setVisible(false);
      setTimeout(() => onClose(id), 250);
    }, duration);
    return () => {
      cancelAnimationFrame(show);
      clearTimeout(hide);
    };
  }, [id, duration, onClose]);

  return (
    <div
      className={[
        "flex items-start gap-2.5 px-3.5 py-2.5 rounded-xl max-w-xs",
        "bg-[var(--popup-bg)] border border-[var(--popup-border)]",
        "shadow-[0_4px_16px_rgba(0,0,0,0.14)]",
        "transition-all duration-200 ease-out",
        visible ? "opacity-100 translate-y-0" : "opacity-0 translate-y-2",
        TOAST_COLORS[type],
      ].join(" ")}
    >
      {TOAST_ICONS[type]}
      <span className="text-[var(--text-primary)] text-xs leading-relaxed">{message}</span>
    </div>
  );
}
