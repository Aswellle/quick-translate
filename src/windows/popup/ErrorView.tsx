// src/windows/popup/ErrorView.tsx
// 翻译错误状态展示（macOS 风格）

import { ERROR_MESSAGES } from "@/lib/constants";

interface ErrorViewProps {
  code: string;
  message?: string;
}

export function ErrorView({ code, message }: ErrorViewProps) {
  const displayMessage =
    ERROR_MESSAGES[code] ?? message ?? ERROR_MESSAGES["UNKNOWN"];

  return (
    <div className="animate-popup px-5 py-6 flex flex-col items-center gap-3 text-center">
      {/* 错误图标 — macOS SF Symbol 风格 */}
      <div className="w-11 h-11 rounded-full bg-gradient-to-br from-red-50 to-red-100 dark:from-red-950/30 dark:to-red-900/20 flex items-center justify-center shadow-sm">
        <svg
          className="w-5 h-5 text-red-500 dark:text-red-400"
          viewBox="0 0 20 20"
          fill="none"
        >
          <circle cx="10" cy="10" r="8.5" stroke="currentColor" strokeWidth="1.5" />
          <path d="M10 6.5v3.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
          <circle cx="10" cy="13" r="0.8" fill="currentColor" />
        </svg>
      </div>

      {/* 错误信息 */}
      <p className="text-sm text-[var(--text-secondary)] leading-relaxed max-w-[280px]">
        {displayMessage}
      </p>

      {/* 错误码（开发模式） */}
      {import.meta.env.DEV && (
        <span className="text-[10px] font-mono text-[var(--text-tertiary)] bg-[var(--surface-tertiary)] px-2 py-0.5 rounded">
          {code}
        </span>
      )}
    </div>
  );
}
