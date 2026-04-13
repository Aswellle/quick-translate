// src/windows/popup/ErrorView.tsx
// 翻译错误状态展示

import { ERROR_MESSAGES } from "@/lib/constants";

interface ErrorViewProps {
  code: string;
  message?: string;
}

export function ErrorView({ code, message }: ErrorViewProps) {
  const displayMessage =
    ERROR_MESSAGES[code] ?? message ?? ERROR_MESSAGES["UNKNOWN"];

  return (
    <div className="animate-popup px-4 py-5 flex flex-col items-center gap-3 text-center">
      {/* 错误图标 */}
      <div className="w-9 h-9 rounded-full bg-red-50 dark:bg-red-900/20 flex items-center justify-center">
        <svg
          className="w-5 h-5 text-red-500 dark:text-red-400"
          viewBox="0 0 20 20"
          fill="none"
        >
          <circle
            cx="10"
            cy="10"
            r="8.5"
            stroke="currentColor"
            strokeWidth="1.5"
          />
          <path
            d="M10 6v4M10 13v.5"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
          />
        </svg>
      </div>

      {/* 错误信息 */}
      <p className="text-sm text-[var(--text-secondary)] leading-snug">
        {displayMessage}
      </p>

      {/* 错误码（开发模式） */}
      {import.meta.env.DEV && (
        <span className="text-[10px] font-mono text-[var(--text-muted)] bg-[var(--hover-bg)] px-2 py-0.5 rounded">
          {code}
        </span>
      )}
    </div>
  );
}
