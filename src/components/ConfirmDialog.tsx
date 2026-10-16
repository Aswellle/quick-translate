// src/components/ConfirmDialog.tsx
// 通用确认浮层（破坏性操作守卫）。
//
// 从 HistoryWindow 内联版本抽出并补齐键盘可及性：打开时焦点落到「取消」
// （破坏性对话框的首选焦点位是安全选项），Escape 触发取消，Tab 在
// 对话框内循环。历史单条删除与「清空全部」共用同一组件，避免两个入口
// 的确认行为各改各的。

import { useEffect, useRef } from "react";

interface ConfirmDialogProps {
  title: string;
  description: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  description,
  confirmLabel,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // 首焦点落在「取消」：误按 Enter/Space 得到的是安全结果
    cancelRef.current?.focus();
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
        return;
      }
      if (e.key === "Tab" && dialogRef.current) {
        // 双按钮对话框：Tab/Shift+Tab 在取消与确认间循环
        const focusables = dialogRef.current.querySelectorAll<HTMLButtonElement>("button");
        if (focusables.length < 2) return;
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        const active = document.activeElement;
        if (!e.shiftKey && active === last) {
          e.preventDefault();
          first.focus();
        } else if (e.shiftKey && active === first) {
          e.preventDefault();
          last.focus();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [onCancel]);

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/20 dark:bg-black/40 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-label={title}
        className="mx-6 p-5 rounded-2xl shadow-macos-lg w-full max-w-xs animate-scale-in bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border border-[var(--border-secondary)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-red-50 dark:bg-red-950/30 flex items-center justify-center shrink-0">
            <svg className="w-5 h-5 text-red-500" viewBox="0 0 20 20" fill="none">
              <path d="M9 11V7m0 6v.5M5.07 16h9.86A2 2 0 0016.8 13L11.9 4a2 2 0 00-3.8 0L3.2 13a2 2 0 001.87 3z" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </div>
          <div>
            <p className="text-[13px] font-semibold text-[var(--text-primary)]">{title}</p>
            <p className="text-[12px] text-[var(--text-secondary)] mt-0.5">{description}</p>
          </div>
        </div>
        <div className="flex gap-2 justify-end mt-4">
          <button
            ref={cancelRef}
            onClick={onCancel}
            className="px-4 py-1.5 rounded-lg text-[12.5px] font-medium border border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--system-blue)]"
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-1.5 rounded-lg text-[12.5px] font-medium bg-[var(--system-red)] hover:bg-[#E62E24] text-white transition-all active:scale-95 shadow-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--system-red)]"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
