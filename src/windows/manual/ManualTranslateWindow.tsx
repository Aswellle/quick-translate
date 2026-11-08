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
  type AppConfig,
} from "@/lib/commands";
import { ERROR_MESSAGES, PROVIDER_LABELS, SUPPORTED_LANGUAGES } from "@/lib/constants";
import { CopyButton } from "@/components/CopyButton";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { mergeExternalConfig } from "@/lib/configSync";
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
  // 目标语言初值跟随配置 store：App.tsx 异步加载 getConfig，挂载瞬间
  // store 可能还是 null —— 对 config 做一次性 useState 快照会把首次
  // 打开恒钉在兜底值。用户主动选择后 targetLang 非空，跟随即停。
  const [targetLangChoice, setTargetLangChoice] = useState<string | null>(null);
  const targetLang = targetLangChoice ?? config?.target_lang ?? "zh";
  const [result, setResult] = useState<TranslationResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 敏感内容拦截待确认：后端过滤命中（未发送任何请求），等用户决定是否放行
  const [pendingSensitive, setPendingSensitive] = useState(false);

  const charCount = [...text].length;
  const overLimit = charCount > MANUAL_MAX_CHARS;
  const canSubmit = text.trim().length > 0 && !overLimit && !busy;

  // config-changed：托盘切目标语言/翻译源等带外写入经此广播到所有窗口。
  // 本窗口 hide 而非销毁、configStore 只在 App.tsx 挂载时加载一次 ——
  // 不合并的话「目标语言跟随配置」跟随的是窗口首次加载时的旧值，托盘
  // 切语言后手动翻译仍译向旧目标且无任何提示。凭证键是掩码占位符、
  // auto_start 以注册表为权威，均不合并（与设置窗口的合并规则同源）；
  // 用户已显式选择的目标语言（targetLangChoice）优先于 store，合并只
  // 改 store、不影响该选择。
  useTauriEvent<AppConfig>("config-changed", (event) => {
    const current = useConfigStore.getState().config;
    if (!current) return;
    const { merged, changed } = mergeExternalConfig(current, event.payload);
    if (changed) useConfigStore.getState().setConfig(merged);
  });

  const run = useCallback(
    async (force = false) => {
      if (text.trim().length === 0 || overLimit || busy) return;
      setBusy(true);
      setError(null);
      try {
        const r = await translateText(
          text.trim(),
          targetLang,
          sourceLang === "auto" ? undefined : sourceLang,
          force || undefined
        );
        setResult(r);
        setPendingSensitive(false);
      } catch (err) {
        // 错误码契约（AppError::error_code ↔ ERROR_MESSAGES）：展示层
        // 只用契约文案，不透传上游原始消息。敏感内容拦截不进错误位 ——
        // 它不是失败，是等确认：弹确认框，用户放行后带 force 重试。
        const code = typeof err === "object" && err && "code" in err
          ? String((err as AppError).code)
          : "UNKNOWN";
        if (code === "SENSITIVE_CONTENT_BLOCKED" && !force) {
          setPendingSensitive(true);
        } else {
          setError(ERROR_MESSAGES[code] ?? ERROR_MESSAGES["UNKNOWN"]);
        }
        setResult(null);
      } finally {
        setBusy(false);
      }
    },
    [text, targetLang, sourceLang, overLimit, busy]
  );

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
          onChange={(e) => setTargetLangChoice(e.target.value)}
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
      {/* ── 敏感内容确认：后端过滤命中后的显式放行（与浮窗「仍然翻译」
          同一知情语义；取消则什么都不发送） ── */}
      {pendingSensitive && (
        <ConfirmDialog
          title="疑似敏感内容"
          description="这段内容可能包含密钥、令牌或鉴权头等敏感信息，尚未发送。仍要把它发送到翻译服务吗？"
          confirmLabel="仍要翻译"
          onConfirm={() => void run(true)}
          onCancel={() => setPendingSensitive(false)}
        />
      )}
    </div>
  );
}
