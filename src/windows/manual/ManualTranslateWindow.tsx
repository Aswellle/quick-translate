// src/windows/manual/ManualTranslateWindow.tsx
// 手动翻译窗口（计划 Task 09）：托盘「手动翻译…」打开的独立小窗。
//
// 与剪贴板路径共用同一翻译核心（translate_text → 翻译引擎 → 同一落盘
// 管道），历史来源标为 manual。窗口随关闭隐藏而非销毁 —— 上次的输入
// 与结果保留，连续翻译几段内容时不用重打。

import { useCallback, useState } from "react";
import {
  translateText,
  type TranslationResult,
  type AppError,
} from "@/lib/commands";
import { ERROR_MESSAGES, PROVIDER_LABELS, SUPPORTED_LANGUAGES } from "@/lib/constants";
import { CopyButton } from "@/components/CopyButton";
import { useConfigStore } from "@/stores/configStore";

/** 单次翻译的文本长度上限。与 Rust `system::translation::request::MAX_TEXT_CHARS`
 *  保持一致 —— 后端仍是最终防线（超限静默截断并返回 truncated 标记），
 *  前端提前拦截是为了不出现「无反馈截断」。 */
export const MANUAL_MAX_CHARS = 5000;

export function ManualTranslateWindow() {
  const config = useConfigStore((s) => s.config);
  const [text, setText] = useState("");
  // 显式源语言是 DeepL 术语表生效的前提（自动检测路径不应用术语表）
  const [sourceLang, setSourceLang] = useState<string>("auto");
  const [targetLang, setTargetLang] = useState<string>(config?.target_lang ?? "zh");
  const [result, setResult] = useState<TranslationResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const charCount = [...text].length;
  const overLimit = charCount > MANUAL_MAX_CHARS;
  const canSubmit = text.trim().length > 0 && !overLimit && !busy;

  const run = useCallback(async () => {
    if (text.trim().length === 0 || overLimit || busy) return;
    setBusy(true);
    setError(null);
    try {
      const r = await translateText(
        text.trim(),
        targetLang,
        sourceLang === "auto" ? undefined : sourceLang
      );
      setResult(r);
    } catch (err) {
      // 错误码契约（AppError::error_code ↔ ERROR_MESSAGES）：展示层
      // 只用契约文案，不透传上游原始消息
      const code = typeof err === "object" && err && "code" in err
        ? String((err as AppError).code)
        : "UNKNOWN";
      setError(ERROR_MESSAGES[code] ?? ERROR_MESSAGES["UNKNOWN"]);
      setResult(null);
    } finally {
      setBusy(false);
    }
  }, [text, targetLang, sourceLang, overLimit, busy]);

  return (
    <div className="flex flex-col h-full bg-[var(--surface-primary)] text-[var(--text-primary)]">
      {/* ── 控制行：源语言 + 目标语言 + 字符计数 ── */}
      <div className="px-4 pt-3 pb-1 flex items-center gap-2 shrink-0">
        <label className="text-[12px] text-[var(--text-secondary)] shrink-0" htmlFor="manual-source-lang">
          源语言
        </label>
        <select
          id="manual-source-lang"
          value={sourceLang}
          onChange={(e) => setSourceLang(e.target.value)}
          className="input-field text-xs py-1.5 flex-1 min-w-0"
        >
          <option value="auto">自动检测</option>
          {SUPPORTED_LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.name}
            </option>
          ))}
        </select>
        <label className="text-[12px] text-[var(--text-secondary)] shrink-0" htmlFor="manual-target-lang">
          目标
        </label>
        <select
          id="manual-target-lang"
          value={targetLang}
          onChange={(e) => setTargetLang(e.target.value)}
          className="input-field text-xs py-1.5 flex-1 min-w-0"
        >
          {SUPPORTED_LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.name}
            </option>
          ))}
        </select>
      </div>
      <div className="px-4 pb-1 flex justify-end shrink-0">
        <span
          className={[
            "text-[11px] tabular-nums",
            overLimit ? "text-red-500 font-medium" : "text-[var(--text-tertiary)]",
          ].join(" ")}
        >
          {charCount} / {MANUAL_MAX_CHARS}
        </span>
      </div>

      {/* ── 输入区 ── */}
      <div className="px-4 shrink-0">
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              void run();
            }
          }}
          placeholder="输入或粘贴要翻译的文本（Ctrl+Enter 翻译）"
          rows={5}
          className="input-field w-full font-normal text-[13px] leading-relaxed resize-none"
          spellCheck={false}
        />
      </div>

      {/* ── 结果区：有结果才占位，阅读衬底与浮窗同款视觉语义 ── */}
      {(result || error) && (
        <div className="px-4 mt-2 min-h-0 overflow-y-auto">
          {error ? (
            <p className="px-3 py-2.5 text-[13px] font-medium text-[var(--popup-text-primary)] leading-relaxed rounded-lg bg-[var(--surface-secondary)]">
              {error}
            </p>
          ) : (
            result && (
              <>
                <div className="px-1 pb-1.5 flex items-center gap-2">
                  <span className="text-[11px] font-semibold text-[var(--text-secondary)] tracking-wide uppercase">
                    {result.provider === "cache"
                      ? "本地缓存"
                      : `→ ${PROVIDER_LABELS[result.provider] ?? result.provider}`}
                  </span>
                  {result.truncated && (
                    <span className="text-[10px] text-orange-500 dark:text-orange-400 bg-orange-50 dark:bg-orange-950/30 px-1.5 py-0.5 rounded-md font-medium">
                      超出部分未翻译
                    </span>
                  )}
                </div>
                <div className="px-3 py-2.5 text-[13.5px] font-medium leading-[1.8] text-[var(--popup-text-primary)] rounded-lg bg-[var(--surface-secondary)] whitespace-pre-wrap break-words selectable">
                  {result.translated_text}
                </div>
                <div className="pt-1.5 flex justify-end">
                  <CopyButton text={result.translated_text} label="复制译文" />
                </div>
              </>
            )
          )}
        </div>
      )}

      {/* ── 底部：动作行 ── */}
      <div className="mt-auto px-4 pb-4 pt-2 flex items-center gap-3 shrink-0">
        <button
          type="button"
          onClick={() => void run()}
          disabled={!canSubmit}
          className="px-4 py-1.5 rounded-lg text-[12.5px] font-medium bg-[var(--system-blue)] text-white transition-all active:scale-95 shadow-sm disabled:opacity-40 disabled:cursor-not-allowed disabled:active:scale-100"
        >
          {busy ? "翻译中…" : "翻译"}
        </button>
        <span className="text-[11px] text-[var(--text-tertiary)]">
          {overLimit
            ? `超出上限 ${MANUAL_MAX_CHARS} 字符，请缩短文本`
            : "历史记录会标记为「手动」来源"}
        </span>
      </div>
    </div>
  );
}
