// src/components/CopyButton.tsx
// 复制按钮：点击后 1 秒内显示 ✓，再恢复

import { useState, useCallback } from "react";
import { copyToClipboard } from "@/lib/commands";

interface CopyButtonProps {
  text: string;
  className?: string;
}

export function CopyButton({ text, className = "" }: CopyButtonProps) {
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
      title={copied ? "已复制" : "复制翻译结果"}
      className={[
        "flex items-center gap-1 px-2 py-1 rounded text-xs transition-all duration-150",
        "hover:bg-black/5 dark:hover:bg-white/10",
        "active:scale-95",
        copied
          ? "text-green-500 dark:text-green-400"
          : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]",
        className,
      ].join(" ")}
    >
      {copied ? (
        <>
          <svg
            className="w-3.5 h-3.5 animate-checkmark"
            viewBox="0 0 16 16"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              d="M3 8l3.5 3.5L13 4"
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
            xmlns="http://www.w3.org/2000/svg"
          >
            <rect
              x="5"
              y="5"
              width="8"
              height="9"
              rx="1.5"
              stroke="currentColor"
              strokeWidth="1.3"
            />
            <path
              d="M4 11H3a1 1 0 01-1-1V3a1 1 0 011-1h7a1 1 0 011 1v1"
              stroke="currentColor"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
          </svg>
          <span>复制</span>
        </>
      )}
    </button>
  );
}
