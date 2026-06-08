// src/components/CopyButton.tsx
// 复制按钮（macOS 风格）

import { useState, useCallback } from "react";
import { copyToClipboard } from "@/lib/commands";

interface CopyButtonProps {
  text: string;
  className?: string;
  label?: string;
}

export function CopyButton({ text, className = "", label = "复制" }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await copyToClipboard(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch (err) {
      console.error("复制失败:", err);
    }
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      title={copied ? "已复制" : `复制${label}`}
      className={[
        "inline-flex items-center gap-1 px-2 py-1 rounded-md text-xs",
        "transition-all duration-150",
        "hover:bg-[var(--hover-bg)] active:bg-[var(--active-bg)]",
        "active:scale-95",
        copied
          ? "text-green-500 dark:text-green-400"
          : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]",
        className,
      ].join(" ")}
    >
      {copied ? (
        <>
          <svg
            className="w-3.5 h-3.5 animate-checkmark"
            viewBox="0 0 16 16"
            fill="none"
          >
            <path
              d="M3 8.5l3.5 3.5 6.5-7"
              stroke="currentColor"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <span>已复制</span>
        </>
      ) : (
        <>
          <svg
            className="w-3.5 h-3.5"
            viewBox="0 0 16 16"
            fill="none"
          >
            <rect
              x="5"
              y="5"
              width="8"
              height="8.5"
              rx="1.5"
              stroke="currentColor"
              strokeWidth="1.3"
            />
            <path
              d="M4 11V3.5A1.5 1.5 0 015.5 2H11a1.5 1.5 0 011.5 1.5V9"
              stroke="currentColor"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
          </svg>
          <span>{label}</span>
        </>
      )}
    </button>
  );
}
