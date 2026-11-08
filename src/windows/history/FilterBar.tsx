// src/windows/history/FilterBar.tsx
// 历史筛选行（来源 / 语言对 / 翻译源）。
//
// 选项从库内实际数据派生（get_history_filter_options），不硬编码清单 ——
// 用户只翻过 en→zh 时语言对下拉就只有这一项。三个维度与搜索/收藏/分页
// 正交组合，全部在服务端过滤（domain/history.rs 的统一 WHERE 组装）。

import { useEffect, useRef, useState } from "react";
import { PROVIDER_LABELS, getLangName } from "@/lib/constants";
import type { HistoryFilterOptions } from "@/lib/commands";

export interface HistoryFilters {
  source: "clipboard" | "manual" | null;
  languagePair: string | null;
  provider: string | null;
}

interface FilterBarProps {
  filters: HistoryFilters;
  options: HistoryFilterOptions | null;
  onChange: (next: HistoryFilters) => void;
}

/** 语言对 "en→zh" → "英语 → 中文"（未知代码原样展示） */
function pairLabel(pair: string): string {
  const [src, dst] = pair.split("→");
  if (!src || !dst) return pair;
  return `${getLangName(src)} → ${getLangName(dst)}`;
}

function FilterSelect({
  value,
  placeholder,
  children,
  onChange,
  active,
}: {
  value: string;
  placeholder: string;
  children: React.ReactNode;
  onChange: (v: string) => void;
  active: boolean;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      aria-label={placeholder}
      className={[
        "text-[12px] px-2 py-1.5 rounded-lg border transition-colors cursor-pointer",
        "bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)]",
        "text-[var(--text-secondary)] hover:text-[var(--text-primary)]",
        active
          ? "border-[var(--system-blue)]/50 text-[var(--text-primary)]"
          : "border-[var(--border-primary)]",
        // 箭头换自定义色：原生 select 外观在深色主题下发灰
        "appearance-none pr-6 bg-no-repeat bg-[right_0.4rem_center]",
      ].join(" ")}
      style={{
        backgroundImage:
          "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16' fill='none'%3E%3Cpath d='M4 6l4 4 4-4' stroke='%23999' stroke-width='1.4' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E\")",
        backgroundSize: "10px",
      }}
    >
      {children}
    </select>
  );
}

export function FilterBar({ filters, options, onChange }: FilterBarProps) {
  // 选项在窗口打开时拉一次；库内出现新 provider/语言对后重开窗口即可
  // 看到新选项（与导出目的地快照同一刷新语义）
  const fetchedRef = useRef(false);
  const [localOptions, setLocalOptions] = useState<HistoryFilterOptions | null>(null);

  useEffect(() => {
    if (fetchedRef.current) return;
    fetchedRef.current = true;
    if (options) {
      setLocalOptions(options);
    }
  }, [options]);

  const opts = localOptions ?? options;
  const hasActive = filters.source !== null || filters.languagePair !== null || filters.provider !== null;

  return (
    <div className="flex items-center gap-2">
      <FilterSelect
        value={filters.source ?? ""}
        placeholder="来源类型"
        active={filters.source !== null}
        onChange={(v) => onChange({ ...filters, source: v === "" ? null : (v as "clipboard" | "manual") })}
      >
        <option value="">全部来源</option>
        <option value="clipboard">剪贴板</option>
        <option value="manual">手动</option>
      </FilterSelect>

      <FilterSelect
        value={filters.languagePair ?? ""}
        placeholder="语言对"
        active={filters.languagePair !== null}
        onChange={(v) => onChange({ ...filters, languagePair: v === "" ? null : v })}
      >
        <option value="">全部语言对</option>
        {(opts?.language_pairs ?? []).map((pair) => (
          <option key={pair} value={pair}>
            {pairLabel(pair)}
          </option>
        ))}
      </FilterSelect>

      <FilterSelect
        value={filters.provider ?? ""}
        placeholder="翻译源"
        active={filters.provider !== null}
        onChange={(v) => onChange({ ...filters, provider: v === "" ? null : v })}
      >
        <option value="">全部翻译源</option>
        {(opts?.providers ?? []).map((p) => (
          <option key={p} value={p}>
            {PROVIDER_LABELS[p] ?? p}
          </option>
        ))}
      </FilterSelect>

      {hasActive && (
        <button
          type="button"
          onClick={() => onChange({ source: null, languagePair: null, provider: null })}
          className="text-[12px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] px-2 py-1 rounded-lg hover:bg-[var(--hover-bg)] transition-colors"
        >
          清除筛选
        </button>
      )}
    </div>
  );
}
