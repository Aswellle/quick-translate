// src/windows/popup/ResultView.tsx
// 翻译结果展示区（macOS 风格）

import { useState, useCallback } from "react";
import { CopyButton } from "@/components/CopyButton";
import { getLangName, PROVIDER_LABELS } from "@/lib/constants";
import { copyToClipboard } from "@/lib/commands";
import type { TranslationResult } from "@/lib/commands";

interface ResultViewProps {
  result: TranslationResult;
}

export function ResultView({ result }: ResultViewProps) {
  const sourceLang = getLangName(result.detected_source_lang);
  const targetLang = getLangName(result.target_lang);
  const providerLabel = PROVIDER_LABELS[result.provider] ?? result.provider;

  return (
    <div className="animate-popup">
      {/* ── 顶部：语言方向标签 ── */}
      <div className="px-4 pt-3.5 pb-2.5 flex items-center gap-2">
        <span className="text-[11px] font-medium text-[var(--text-tertiary)] tracking-wide uppercase">
          {sourceLang}
        </span>
        <svg
          className="w-3 h-3 text-[var(--text-tertiary)]"
          viewBox="0 0 12 12"
          fill="none"
        >
          <path
            d="M2 6h8M7 3l3 3-3 3"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
        <span className="text-[11px] font-medium text-[var(--text-secondary)] tracking-wide uppercase">
          {targetLang}
        </span>
        {result.truncated && (
          <span className="ml-auto text-[10px] text-orange-500 dark:text-orange-400 bg-orange-50 dark:bg-orange-950/30 px-1.5 py-0.5 rounded-md font-medium">
            已截断
          </span>
        )}
      </div>

      {/* ── macOS 分隔线 ── */}
      <div className="h-px mx-4 bg-[var(--border-secondary)]" />

      {/* ── 翻译结果文本 ── */}
      <div
        className="selectable px-4 py-3.5 text-[13px] leading-[1.8] text-[var(--text-primary)] overflow-y-auto whitespace-pre-wrap break-words"
        style={{ maxHeight: "360px", fontFamily: "var(--font-ui)" }}
      >
        {result.translated_text}
      </div>

      {/* ── macOS 分隔线 ── */}
      <div className="h-px mx-4 bg-[var(--border-secondary)]" />

      {/* ── 底部操作栏 ── */}
      <div className="px-3 py-2 flex items-center justify-between">
        {/* 左：复制按钮组 */}
        <div className="flex items-center gap-0.5">
          <CopyButton text={result.translated_text} label="译文" />
          <SourceCopyButton text={result.source_text} />
        </div>

        {/* 右：meta 信息 */}
        <div className="flex items-center gap-2.5">
          {import.meta.env.DEV && (
            <span className="text-[10px] text-[var(--text-tertiary)] tabular-nums">
              {result.duration_ms}ms
            </span>
          )}
          <span className="text-[10px] text-[var(--text-tertiary)] font-medium tracking-wide">
            {providerLabel}
          </span>
        </div>
      </div>
    </div>
  );
}

/// 复制原文按钮
function SourceCopyButton({ text }: { text: string }) {
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
      title={copied ? "已复制" : "复制原文"}
      className={[
        "inline-flex items-center gap-1 px-2 py-1 rounded-md text-xs",
        "transition-all duration-150",
        "hover:bg-[var(--hover-bg)] active:bg-[var(--active-bg)]",
        "active:scale-95",
        copied
          ? "text-green-500 dark:text-green-400"
          : "text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]",
      ].join(" ")}
    >
      {copied ? (
        <>
          <svg className="w-3.5 h-3.5 animate-checkmark" viewBox="0 0 16 16" fill="none">
            <path d="M3 8.5l3.5 3.5 6.5-7" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          <span>已复制</span>
        </>
      ) : (
        <>
          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
            <rect x="5" y="5" width="8" height="8.5" rx="1.5" stroke="currentColor" strokeWidth="1.3" />
            <path d="M4 11V3.5A1.5 1.5 0 015.5 2H11a1.5 1.5 0 011.5 1.5V9" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
          </svg>
          <span>原文</span>
        </>
      )}
    </button>
  );
}
