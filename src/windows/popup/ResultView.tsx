// src/windows/popup/ResultView.tsx
// 翻译结果展示区（macOS 风格）

import { CopyButton } from "@/components/CopyButton";
import { getLangName, PROVIDER_LABELS } from "@/lib/constants";
import type { TranslationResult } from "@/lib/commands";

interface ResultViewProps {
  result: TranslationResult;
  /** 浮窗已转为可交互态（用户点击过）才显示键盘提示 —— 被动态下
      键盘事件到不了 webview，提示只会误导用户往文档里打空格 */
  showKeyboardHint?: boolean;
}

export function ResultView({ result, showKeyboardHint = false }: ResultViewProps) {
  const sourceLang = getLangName(result.detected_source_lang);
  const targetLang = getLangName(result.target_lang);
  const providerLabel = PROVIDER_LABELS[result.provider] ?? result.provider;

  return (
    <div className="animate-popup">
      {/* ── 顶部：语言方向标签（pt 收窄，上方已有红绿灯标题栏）── */}
      <div className="px-4 pt-1 pb-2.5 flex items-center gap-2">
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
        className="selectable px-4 py-3.5 text-[13.5px] leading-[1.75] tracking-[0.005em] text-[var(--text-primary)] overflow-y-auto whitespace-pre-wrap break-words"
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
          <CopyButton text={result.source_text} label="原文" />
        </div>

        {/* 右：meta 信息 */}
        <div className="flex items-center gap-2.5">
          {/* 快捷键提示仅在可交互态显示：被动态下 Esc/空格收不到，
              "点击浮窗后按空格关闭"告知正确的两步操作。
              不加响应式前缀 —— sm: 断点是 640px，浮窗视口仅 280–520px，
              `hidden sm:flex` 会让本提示永不渲染（F7） */}
          <span className="flex items-center gap-1 text-[10px] text-[var(--text-tertiary)]">
            {showKeyboardHint ? (
              <>
                <kbd className="kbd-hint">空格</kbd>
                关闭
              </>
            ) : (
              "点击后空格关闭"
            )}
          </span>
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
