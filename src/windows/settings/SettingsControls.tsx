// src/windows/settings/SettingsControls.tsx
// 设置页的通用行与开关组件。
//
// 从 SettingsWindow 抽出：术语表页（GlossaryTab）也要用 ToggleRow，
// 直接互相 import 会形成 SettingsWindow ⇄ GlossaryTab 循环依赖。

import { useCallback, useRef, useState } from "react";

export function SettingsRow({
  label,
  hint,
  htmlFor,
  children,
}: {
  label: string;
  hint?: string;
  /** 传入时 label 渲染为 <label htmlFor>，控件可被标签点击聚焦、屏幕
      阅读器可正确关联 —— 纯开关行（Toggle 自带 role=switch）无需传入 */
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="px-4 py-3 flex items-center justify-between gap-4">
      <div className="flex-1 min-w-0">
        {htmlFor ? (
          <label
            htmlFor={htmlFor}
            className="text-[13px] font-medium text-[var(--text-primary)] block cursor-pointer"
          >
            {label}
          </label>
        ) : (
          <p className="text-[13px] font-medium text-[var(--text-primary)]">{label}</p>
        )}
        {hint && <p className="text-[12px] text-[var(--text-secondary)] mt-0.5 leading-snug">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

/**
 * 开关行：Toggle + 就近的生效状态提示。
 *
 * 状态提示紧贴按钮右侧的空白区域：11.5px / medium / 语义色
 * （成功绿、失败红）。此前「已保存」只出现在底部保存栏——用户盯着
 * 开关看时视线要跨过整屏去找结果。busy 期间开关禁用防连点。
 *
 * 反馈必须是**真实结果**而非切换瞬间的乐观文案：onApply 返回
 * Promise<boolean>（或 boolean / void）时，生效成功才显示绿色
 * 「已开启/已关闭」，失败显示红色「未生效，已还原」—— 持久化失败时
 * 开关视觉上会弹回，若绿色「已开启」仍驻留，用户盯着看到的是两个
 * 互相矛盾的反馈。onApply 返回 void 视为成功（调用方自担结果展示）。
 */
export function ToggleRow({
  label,
  hint,
  value,
  onApply,
}: {
  label: string;
  hint?: string;
  value: boolean;
  onApply: (v: boolean) => boolean | void | Promise<boolean | void>;
}) {
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<null | { ok: boolean; text: string }>(null);
  // 显示自己最近一次的结果：连续切换时新结果覆盖旧结果
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const apply = useCallback(
    (v: boolean) => {
      if (busy) return;
      if (timerRef.current) clearTimeout(timerRef.current);
      setBusy(true);
      const settle = (ok: boolean) => {
        setStatus({
          ok,
          text: ok ? (v ? "已开启" : "已关闭") : "未生效，已还原",
        });
        // 状态提示驻留 2.5s 后淡出（下一次切换会立即覆盖）
        timerRef.current = setTimeout(() => setStatus(null), 2500);
        setBusy(false);
      };
      let result: boolean | void | Promise<boolean | void>;
      try {
        result = onApply(v);
      } catch {
        settle(false);
        return;
      }
      Promise.resolve(result)
        .then((ok) => settle(ok !== false))
        .catch(() => settle(false));
    },
    [busy, onApply]
  );

  return (
    <SettingsRow label={label} hint={hint}>
      <div className="flex items-center gap-2.5">
        {status && (
          <span
            role="status"
            className={[
              "text-[11.5px] font-medium animate-fade-in",
              status.ok
                ? "text-green-600 dark:text-green-500"
                : "text-red-500",
            ].join(" ")}
          >
            {status.text}
          </span>
        )}
        <Toggle value={value} onChange={apply} busy={busy} label={label} />
      </div>
    </SettingsRow>
  );
}

// macOS 风格的 Toggle 开关。
//
// aria-label 是屏幕阅读器的唯一名称来源：纯视觉开关没有可见文本，
// 不补的话读屏只报「开关，已开启」—— 全页 8 个开关完全无法区分，
// 关掉敏感过滤这类有安全后果的操作可能被误触。
function Toggle({
  value,
  onChange,
  busy = false,
  label,
}: {
  value: boolean;
  onChange: (v: boolean) => void;
  busy?: boolean;
  label: string;
}) {
  return (
    <button
      onClick={() => onChange(!value)}
      disabled={busy}
      role="switch"
      aria-checked={value}
      aria-label={label}
      className={[
        "relative w-9 h-5 rounded-full transition-colors duration-200",
        value ? "bg-[var(--system-blue)]" : "bg-[var(--surface-tertiary)]",
      ].join(" ")}
      style={{
        boxShadow: value
          ? "0 0 0 0 rgba(0, 122, 255, 0)"
          : "inset 0 0 0 1px rgba(0,0,0,0.08)",
      }}
    >
      <span
        className={[
          "absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full",
          "transition-transform duration-200",
          "shadow-[0_1px_3px_rgba(0,0,0,0.2)]",
          value ? "translate-x-4" : "translate-x-0",
        ].join(" ")}
      />
    </button>
  );
}
