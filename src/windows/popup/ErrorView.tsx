// src/windows/popup/ErrorView.tsx
// 翻译错误状态展示（macOS 风格）
//
// 错误视图带操作行：认证/额度类错误指向「前往设置」，其余提示「重新复制
// 即可重试」（剪贴板监控会在新内容到来时自动重译 —— 这是实际存在的重试
// 路径，此前 UI 从不告诉用户）。浮窗失焦即关，"请在设置中检查"若只靠
// 用户记住托盘入口，这条引导基本等于没写。

import { ERROR_MESSAGES } from "@/lib/constants";
import { openSettingsWindow } from "@/lib/commands";

interface ErrorViewProps {
  code: string;
  message?: string;
}

/** 指向设置页的错误类别：修复动作在配置里，不在重试里 */
const SETTINGS_FIX_CODES = new Set(["AUTH_ERROR", "QUOTA_EXHAUSTED"]);

export function ErrorView({ code, message }: ErrorViewProps) {
  const displayMessage =
    ERROR_MESSAGES[code] ?? message ?? ERROR_MESSAGES["UNKNOWN"];
  const settingsFix = SETTINGS_FIX_CODES.has(code);

  const goSettings = () => {
    openSettingsWindow("provider").catch((err) =>
      console.error("[ErrorView] 打开设置失败:", err)
    );
  };

  return (
    <div className="animate-popup px-5 pb-5 pt-2 flex flex-col items-center gap-3 text-center">
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

      {/* 错误信息 — 套阅读衬底：错误文本是用户此刻唯一要读的内容，
          压在裸露的玻璃上最容易在杂乱背景里读丢。只包文字，不包
          图标和操作行，避免衬底变成一块覆盖全屏的暗板。 */}
      <p className="popup-reading-surface px-3.5 py-2 text-[14px] font-medium text-[var(--text-primary)] leading-relaxed max-w-[280px]">
        {displayMessage}
      </p>

      {/* 操作行：按错误类别给出可执行的下一步，而不是让用户自己找托盘 */}
      <div className="flex items-center gap-2">
        {settingsFix && (
          <button
            type="button"
            onClick={goSettings}
            className="px-3 py-1.5 rounded-lg text-[12px] font-medium bg-[var(--system-blue)] hover:brightness-110 text-white transition-all active:scale-95 shadow-sm"
          >
            前往设置
          </button>
        )}
        <span className="text-[11.5px] text-[var(--text-secondary)]">
          重新复制文字即可重试
        </span>
      </div>

      {/* 错误码（开发模式） */}
      {import.meta.env.DEV && (
        <span className="text-[10px] font-mono text-[var(--text-tertiary)] bg-[var(--surface-tertiary)] px-2 py-0.5 rounded">
          {code}
        </span>
      )}
    </div>
  );
}
