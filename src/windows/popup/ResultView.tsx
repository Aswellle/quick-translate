// src/windows/popup/ResultView.tsx
// 翻译结果展示区（含语言标签、文本、底部操作栏）

import { CopyButton } from "@/components/CopyButton";
import { getLangName } from "@/lib/constants";
import type { TranslationResult } from "@/lib/commands";

interface ResultViewProps {
  result: TranslationResult;
}

/** 翻译源显示名 */
const PROVIDER_LABELS: Record<string, string> = {
  deepl: "DeepL",
  google: "Google",
};

export function ResultView({ result }: ResultViewProps) {
  const sourceLang = getLangName(result.detected_source_lang);
  const targetLang = getLangName(result.target_lang);
  const providerLabel = PROVIDER_LABELS[result.provider] ?? result.provider;

  return (
    <div className="animate-popup">
      {/* ── 顶部：语言方向标签 ── */}
      <div className="px-4 pt-3 pb-2 flex items-center gap-1.5">
        <span className="text-[11px] text-[var(--text-muted)] font-medium tracking-wide">
          {sourceLang}
        </span>
        <svg
          className="w-3 h-3 text-[var(--text-muted)]"
          viewBox="0 0 12 12"
          fill="none"
        >
          <path
            d="M2 6h8M7 3l3 3-3 3"
            stroke="currentColor"
            strokeWidth="1.3"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
        <span className="text-[11px] text-[var(--text-secondary)] font-medium tracking-wide">
          {targetLang}
        </span>
        {result.truncated && (
          <span className="ml-auto text-[10px] text-amber-500 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 px-1.5 py-0.5 rounded">
            已截断
          </span>
        )}
      </div>

      {/* ── 分隔线 ── */}
      <div className="h-px mx-4 bg-[var(--divider)]" />

      {/* ── 翻译结果文本 ── */}
      <div
        className="selectable px-4 py-3 text-sm leading-relaxed text-[var(--text-primary)] overflow-y-auto"
        style={{ maxHeight: "220px" }}
      >
        {result.translated_text}
      </div>

      {/* ── 分隔线 ── */}
      <div className="h-px mx-4 bg-[var(--divider)]" />

      {/* ── 底部操作栏 ── */}
      <div className="px-3 py-2 flex items-center justify-between">
        <CopyButton text={result.translated_text} />

        <div className="flex items-center gap-2">
          {/* 耗时显示（仅开发模式） */}
          {import.meta.env.DEV && (
            <span className="text-[10px] text-[var(--text-muted)]">
              {result.duration_ms}ms
            </span>
          )}
          {/* 翻译源标识 */}
          <span className="text-[10px] text-[var(--text-muted)] font-medium">
            {providerLabel}
          </span>
        </div>
      </div>
    </div>
  );
}
