// src/windows/settings/StatsDashboard.tsx
// 设置页统计仪表盘 — recharts 可视化
//
// 数据来自 get_stats()（Rust 侧聚好簇再过 IPC，前端不持有原始记录）。
// 所有颜色走 CSS 变量，深浅主题自动跟随。
//
// tooltip 设计：悬浮一个数据点应回答「那天翻译了多少、多少字、占这周
// 几成」—— 只显示次数的 tooltip 信息密度太低，用户悬浮是在找细节。

import { useMemo } from "react";
import {
  AreaChart,
  Area,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Cell,
} from "recharts";
import { PROVIDER_LABELS } from "@/lib/constants";
import type { StatsResult } from "@/lib/commands";

// 主题色通过 CSS 变量注入 recharts（SVG fill/stroke 接受 var()）
const CHART_ACCENT = "var(--system-blue)";
const CHART_GRID = "var(--border-secondary)";
const CHART_AXIS = "var(--text-tertiary)";

const TICK_STYLE = { fontSize: 10.5, fill: CHART_AXIS } as const;

const WEEKDAYS = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

function dayLabel(dateMs: number, compact: boolean): string {
  const d = new Date(dateMs);
  // 30 天档的横轴放不下 30 个「周X」标签（还会跨周重复造成误读），
  // 压缩为 M/D 短格式；7 天档维持「今天/周X」的人话标签
  if (compact) return `${d.getMonth() + 1}/${d.getDate()}`;
  const today = new Date();
  const isToday =
    d.getFullYear() === today.getFullYear() &&
    d.getMonth() === today.getMonth() &&
    d.getDate() === today.getDate();
  return isToday ? "今天" : WEEKDAYS[d.getDay()];
}

/** 完整日期：2026/10/20（tooltip 里用，横轴空间不够） */
function fullDate(dateMs: number): string {
  const d = new Date(dateMs);
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()}`;
}

function formatChars(n: number): string {
  if (n >= 10_000) return (n / 10_000).toFixed(1) + " 万";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return String(n);
}

/** 趋势图 —— 渐变面积图 + 详细悬浮卡（数据范围由调用方的档位决定） */
function TrendChart({ daily }: { daily: StatsResult["daily"] }) {
  const compact = daily.length > 10;
  const data = useMemo(
    () =>
      daily.map((d) => ({
        label: dayLabel(d.date_ms, compact),
        date_ms: d.date_ms,
        count: d.count,
        chars: d.chars,
      })),
    [daily, compact]
  );
  const rangeTotal = useMemo(() => data.reduce((s, d) => s + d.count, 0), [data]);

  return (
    <ResponsiveContainer width="100%" height={150}>
      <AreaChart data={data} margin={{ top: 8, right: 8, left: -20, bottom: 0 }}>
        <defs>
          {/* 面积渐变：主色由浓到透，替代纯色平涂 */}
          <linearGradient id="trendFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={CHART_ACCENT} stopOpacity={0.30} />
            <stop offset="90%" stopColor={CHART_ACCENT} stopOpacity={0.02} />
          </linearGradient>
        </defs>
        <CartesianGrid stroke={CHART_GRID} vertical={false} strokeDasharray="3 4" />
        <XAxis
          dataKey="label"
          tick={TICK_STYLE}
          axisLine={{ stroke: CHART_GRID }}
          tickLine={false}
          interval={compact ? "preserveStartEnd" : 0}
          minTickGap={18}
        />
        <YAxis
          tick={TICK_STYLE}
          axisLine={false}
          tickLine={false}
          allowDecimals={false}
          width={38}
        />
        <Tooltip
          content={<TrendTooltip rangeTotal={rangeTotal} />}
          // 跟手 + 无黑框；cursor 线对齐到最近数据点，悬浮不差像素
          cursor={{ stroke: CHART_AXIS, strokeDasharray: "3 3" }}
          wrapperStyle={{ outline: "none", zIndex: 10 }}
        />
        <Area
          type="monotone"
          dataKey="count"
          stroke={CHART_ACCENT}
          strokeWidth={2.2}
          fill="url(#trendFill)"
          activeDot={{ r: 4, fill: CHART_ACCENT, stroke: "var(--surface-primary)", strokeWidth: 2 }}
          isAnimationActive={false}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}

/** 趋势图悬浮卡：日期 + 次数 + 字符量 + 区间占比 */
function TrendTooltip({ active, payload, rangeTotal }: {
  active?: boolean;
  payload?: Array<{ payload: { label: string; date_ms: number; count: number; chars: number } }>;
  rangeTotal: number;
}) {
  if (!active || !payload?.length) return null;
  const d = payload[0].payload;
  const pct = rangeTotal > 0 ? Math.round((d.count / rangeTotal) * 100) : 0;
  return (
    <div className="rounded-xl bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border border-[var(--border-secondary)] shadow-[var(--card-shadow)] px-3 py-2 min-w-[132px]">
      <p className="text-[11px] font-semibold text-[var(--text-primary)]">
        {fullDate(d.date_ms)}
        <span className="text-[var(--text-tertiary)] font-normal ml-1.5">{d.label}</span>
      </p>
      <div className="mt-1.5 space-y-0.5">
        <p className="text-[12px] text-[var(--text-secondary)] flex justify-between gap-3">
          <span>翻译</span>
          <span className="font-semibold text-[var(--text-primary)] tabular-nums">{d.count} 次</span>
        </p>
        <p className="text-[12px] text-[var(--text-secondary)] flex justify-between gap-3">
          <span>字符</span>
          <span className="font-medium text-[var(--text-primary)] tabular-nums">{d.chars.toLocaleString()}</span>
        </p>
        <p className="text-[11px] text-[var(--text-tertiary)] flex justify-between gap-3 pt-0.5 border-t border-[var(--border-secondary)] mt-1">
          <span>占当前区间</span>
          <span className="tabular-nums">{pct}%</span>
        </p>
      </div>
    </div>
  );
}

/** 翻译源分布 —— 渐变横向条形图 + 详细悬浮卡 */
function ProviderChart({ byProvider }: { byProvider: StatsResult["by_provider"] }) {
  const data = useMemo(
    () =>
      Object.entries(byProvider)
        .sort(([, a], [, b]) => b - a)
        .map(([provider, count]) => ({
          name: PROVIDER_LABELS[provider] ?? provider,
          count,
        })),
    [byProvider]
  );
  const total = useMemo(() => data.reduce((s, d) => s + d.count, 0), [data]);

  return (
    <ResponsiveContainer width="100%" height={data.length * 32 + 12}>
      <BarChart data={data} layout="vertical" margin={{ top: 0, right: 24, left: 0, bottom: 0 }}>
        <XAxis type="number" hide allowDecimals={false} />
        <YAxis
          type="category"
          dataKey="name"
          tick={{ ...TICK_STYLE, fontSize: 11 }}
          axisLine={false}
          tickLine={false}
          width={64}
        />
        <Tooltip
          content={<ProviderTooltip total={total} />}
          cursor={{ fill: "var(--hover-bg)" }}
          wrapperStyle={{ outline: "none", zIndex: 10 }}
        />
        <Bar dataKey="count" radius={[0, 5, 5, 0]} barSize={14} isAnimationActive={false}>
          {data.map((_, i) => (
            <Cell key={i} fill={PROVIDER_COLORS[i % PROVIDER_COLORS.length]} />
          ))}
        </Bar>
      </BarChart>
    </ResponsiveContainer>
  );
}

/** 分布图悬浮卡：次数 + 占比 */
function ProviderTooltip({ active, payload, total }: {
  active?: boolean;
  payload?: Array<{ payload: { name: string; count: number } }>;
  total: number;
}) {
  if (!active || !payload?.length) return null;
  const d = payload[0].payload;
  const pct = total > 0 ? Math.round((d.count / total) * 100) : 0;
  return (
    <div className="rounded-xl bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border border-[var(--border-secondary)] shadow-[var(--card-shadow)] px-3 py-2">
      <p className="text-[11px] font-semibold text-[var(--text-primary)]">{d.name}</p>
      <p className="text-[12px] text-[var(--text-secondary)] mt-0.5 tabular-nums">
        <span className="font-semibold text-[var(--text-primary)]">{d.count}</span> 次
        <span className="text-[var(--text-tertiary)] ml-1.5">· {pct}%</span>
      </p>
    </div>
  );
}

// 翻译源条形图配色：主色 + 同明度的 macOS 系统色序列（深浅主题都可读）
const PROVIDER_COLORS = [
  "var(--system-blue)",
  "var(--system-teal)",
  "var(--system-purple)",
  "var(--system-orange)",
  "var(--system-green)",
  "var(--system-pink)",
];

export { TrendChart, ProviderChart };
