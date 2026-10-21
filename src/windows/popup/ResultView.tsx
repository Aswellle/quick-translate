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
      {/* ── 顶部：语言方向标签（pt 收窄，上方已有红绿灯标题栏）──
          11px 是 PRD 规格，但颜色用 secondary 而非 tertiary —— 方向标签
          是理解「译成了什么语言」的关键信息，不是装饰 */}
      <div className="px-4 pt-1 pb-2.5 flex items-center gap-2">
        <span className="text-[11px] font-semibold text-[var(--text-secondary)] tracking-wide uppercase">
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
        <span className="text-[11px] font-semibold text-[var(--text-secondary)] tracking-wide uppercase">
          {targetLang}
        </span>
        {result.truncated && (
          <span className="ml-auto text-[10px] text-orange-500 dark:text-orange-400 bg-orange-50 dark:bg-orange-950/30 px-1.5 py-0.5 rounded-md font-medium">
            已截断
          </span>
        )}
      </div>

      {/* ── 翻译结果文本 ──
          译文是浮窗的主角：14.5px / 500 字重（实机验证反馈 13.5px/400
          在玻璃材质上存在感不足），行高放宽到 1.8 保证中文可读性。

          阅读衬底（.popup-reading-surface）只铺在这一块：标题栏和底部
          操作栏保持通透，让 OS 原生材质仍然可见。整块面板加厚会把玻璃
          闷成色板，那是这条链路历史上反复踩过的坑。

          上方不再放分隔线 —— 衬底自带内描边，再叠一条会形成双重 border。

          maxHeight 从 360 降到 300：阅读衬底自身带来 padding(24) + 内描边(2)，
          叠加标题栏、语言标签、分隔线与操作栏后，360 会让容器总高达到约
          497px，超过后端 MAX_HEIGHT=480 的硬上限。被 clamp 的后果是译文
          滚动区被压扁，而用户看不到「还有内容」的提示。300 留出约 43px
          余量，滚动区仍能显示约 11 行中文。 */}
      <div className="px-2 pb-2">
        <div
          className="popup-reading-surface selectable px-3 py-3 text-[14.5px] font-medium leading-[1.8] tracking-[0.005em] text-[var(--popup-text-primary)] overflow-y-auto whitespace-pre-wrap break-words"
          style={{ maxHeight: "300px", fontFamily: "var(--font-ui)" }}
        >
          {result.translated_text}
        </div>
      </div>

      {/* ── macOS 分隔线 ──
          上方那条已移除：阅读衬底自带内描边，再叠一条会形成双重 border。
          这一条保留 —— 它分界的是「正文区」与「操作栏」，两者都在衬底
          之外，需要一条线把操作栏和玻璃层分开。 */}
      <div className="h-px mx-4 bg-[var(--popup-divider)]" />

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
          <span className="flex items-center gap-1 text-[10.5px] text-[var(--text-secondary)]">
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
            <span className="text-[10.5px] text-[var(--text-tertiary)] tabular-nums">
              {result.duration_ms}ms
            </span>
          )}
          <span className="text-[10.5px] text-[var(--text-secondary)] font-semibold tracking-wide">
            {providerLabel}
          </span>
        </div>
      </div>
    </div>
  );
}
