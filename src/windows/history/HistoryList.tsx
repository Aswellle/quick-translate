// src/windows/history/HistoryList.tsx
// 历史记录列表（macOS 风格 — 简约明亮灵动）

import { useCallback, memo, useMemo, useState } from "react";
import { CopyButton } from "@/components/CopyButton";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { getLangName, PROVIDER_LABELS } from "@/lib/constants";
import { useHistoryStore } from "@/stores/historyStore";
import { deleteHistoryRecord, toggleStarRecord } from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import type { TranslationRecord } from "@/lib/commands";

const TRUNCATE_LEN = 110;

/**
 * 命中感知截断：搜索时把命中词移到可见区，而不是死板地取前 110 字。
 *
 * 修复前的问题：搜索「季度报告」时，如果命中出现在第 300 字，列表里
 * 那一行只显示前 80 字 —— 用户看到一条结果却看不到它为什么匹配，
 * 只能逐条展开确认。命中感知截断让高亮词一定落在可见窗口内。
 */
function truncateAround(text: string, query: string, len = TRUNCATE_LEN) {
  if (text.length <= len) return text;
  const q = query.trim().toLowerCase();
  if (!q) return text.slice(0, len) + "…";
  const idx = text.toLowerCase().indexOf(q);
  if (idx < 0) return text.slice(0, len) + "…";
  // 命中点前留 1/3 上下文，保证命中词落在中前部且不被截断
  const start = Math.max(0, idx - Math.floor(len / 3));
  const end = Math.min(text.length, start + len);
  const prefix = start > 0 ? "…" : "";
  const suffix = end < text.length ? "…" : "";
  return prefix + text.slice(start, end) + suffix;
}

/** 相对时间：刚刚 / N 分钟前 / 今天 HH:mm / 昨天 HH:mm / MM-DD */
function formatRelative(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const diffMin = Math.floor((now.getTime() - ms) / 60_000);
  if (diffMin < 1) return "刚刚";
  if (diffMin < 60) return `${diffMin} 分钟前`;
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  const hhmm = d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  if (sameDay) return `今天 ${hhmm}`;
  const yest = new Date(now);
  yest.setDate(now.getDate() - 1);
  if (
    d.getFullYear() === yest.getFullYear() &&
    d.getMonth() === yest.getMonth() &&
    d.getDate() === yest.getDate()
  ) {
    return `昨天 ${hhmm}`;
  }
  return d.toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" });
}

/** 分组标题：今天 / 昨天 / 本周 / YYYY年M月 */
function groupLabel(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (ms >= startOfToday) return "今天";
  if (ms >= startOfToday - 86_400_000) return "昨天";
  if (ms >= startOfToday - 6 * 86_400_000) return "最近一周";
  return `${d.getFullYear()} 年 ${d.getMonth() + 1} 月`;
}

function Highlight({ text, query }: { text: string; query: string }) {
  // useMemo 避免每次渲染都重新编译 RegExp（50条结果×2列 = 100次/页）
  const regex = useMemo(() => {
    if (!query.trim()) return null;
    const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`(${escaped})`, "gi");
  }, [query]);

  if (!regex) return <>{text}</>;
  const parts = text.split(regex);
  return (
    <>
      {parts.map((part, i) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark
            key={i}
            className="bg-yellow-200 dark:bg-yellow-600/50 text-[var(--text-primary)] rounded-sm px-[1px]"
          >
            {part}
          </mark>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}

interface RecordCardProps {
  record: TranslationRecord;
  isExpanded: boolean;
  searchQuery: string;
  onToggle: (id: string) => void;
  onDelete: (id: string) => void;
  onStar: (id: string) => void;
}

const RecordCard = memo(function RecordCard({
  record,
  isExpanded,
  searchQuery,
  onToggle,
  onDelete,
  onStar,
}: RecordCardProps) {
  return (
    <div
      className={[
        "rounded-xl overflow-hidden transition-colors duration-150 cursor-pointer",
        "border",
        isExpanded
          ? "bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border-[var(--system-blue)]/25 dark:border-[var(--system-blue)]/40 shadow-sm"
          : "bg-[var(--card-bg)] border-[var(--card-border)] hover:border-[var(--system-blue)]/25 dark:hover:border-[var(--system-blue)]/40 hover:shadow-sm",
      ].join(" ")}
      role="button"
      tabIndex={0}
      aria-expanded={isExpanded}
      onClick={() => onToggle(record.id)}
      onKeyDown={(e) => {
        // 卡片是键盘可达的展开/收起开关：展开视图里的收藏/删除/复制
        // 只在展开后存在，没有这条路径键盘用户就完全够不到这些操作
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onToggle(record.id);
        }
      }}
    >
      {/* ── 折叠视图 ──
          信息层级：译文（主）> 原文（次）> 语言/来源/时间（元信息）。
          译文用 2 行显示而不是单行截断 —— 单行在 110 字窗口里通常只剩
          前半句，用户要判断「是不是我要的那条」必须展开，扫读效率很低。 */}
      <div className="px-4 py-3">
        <div className="flex items-start justify-between gap-2">
          <div className="flex-1 min-w-0 space-y-1.5">
            {/* 译文：主信息，允许两行 */}
            <p className="text-[13px] text-[var(--text-primary)] leading-snug line-clamp-2">
              <Highlight
                text={truncateAround(record.translated_text, searchQuery)}
                query={searchQuery}
              />
            </p>
            {/* 原文：次信息，单行 */}
            <p className="text-[12px] text-[var(--text-secondary)] truncate font-mono">
              <Highlight
                text={truncateAround(record.source_text, searchQuery)}
                query={searchQuery}
              />
            </p>
          </div>
          {/* 收藏星标 + 展开箭头 */}
          <div className="flex items-center gap-1.5 mt-0.5 flex-shrink-0">
            {record.is_starred && (
              <svg className="w-3.5 h-3.5 text-yellow-400" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8 1.5l1.75 3.5 3.9.57-2.82 2.74.66 3.88L8 10.35l-3.49 1.84.66-3.88-2.82-2.74 3.9-.57L8 1.5z" />
              </svg>
            )}
            <svg
              className={[
                "w-3.5 h-3.5 text-[var(--text-tertiary)]",
                "transition-transform duration-200",
                isExpanded ? "rotate-180" : "",
              ].join(" ")}
              viewBox="0 0 16 16"
              fill="none"
            >
              <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </div>
        </div>
        {/* 元信息行 */}
        <div className="flex items-center gap-2 mt-2">
          <span className="text-[11px] font-medium text-[var(--text-secondary)]">
            {getLangName(record.source_lang)} → {getLangName(record.target_lang)}
          </span>
          <span className="text-[11px] font-medium text-[var(--text-secondary)] bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] px-1.5 py-px rounded-full">
            {PROVIDER_LABELS[record.provider] ?? record.provider}
          </span>
          {record.duration_ms != null && (
            <span className="text-[11px] text-[var(--text-tertiary)] tabular-nums">
              {record.duration_ms}ms
            </span>
          )}
          <span
            className="text-[11px] text-[var(--text-secondary)] ml-auto tabular-nums"
            title={new Date(record.created_at).toLocaleString("zh-CN")}
          >
            {formatRelative(record.created_at)}
          </span>
        </div>
      </div>

      {/* ── 展开视图 ── */}
      {isExpanded && (
        <div
          className="border-t border-[var(--border-secondary)] px-4 py-3.5 space-y-3"
          onClick={(e) => e.stopPropagation()}
        >
          <div>
            <p className="text-[11.5px] font-semibold text-[var(--text-secondary)] tracking-wide mb-1">
              原文
            </p>
            <p className="selectable text-[12.5px] text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap break-words font-mono">
              <Highlight text={record.source_text} query={searchQuery} />
            </p>
          </div>
          <div>
            <p className="text-[11.5px] font-semibold text-[var(--text-secondary)] tracking-wide mb-1">
              译文
            </p>
            <p className="selectable text-[13px] text-[var(--text-primary)] leading-relaxed whitespace-pre-wrap break-words">
              <Highlight text={record.translated_text} query={searchQuery} />
            </p>
          </div>
          {/* 操作栏 */}
          <div className="flex items-center justify-between pt-1.5 border-t border-[var(--border-secondary)]">
            <div className="flex items-center gap-1">
              <CopyButton text={record.translated_text} label="译文" />
              <button
                onClick={() => onStar(record.id)}
                className={[
                  "inline-flex items-center gap-1 px-2 py-1 rounded-md text-[12px] transition-colors",
                  record.is_starred
                    ? "text-yellow-500 hover:text-yellow-600 hover:bg-yellow-50 dark:hover:bg-yellow-950/20"
                    : "text-[var(--text-secondary)] hover:text-yellow-600 dark:hover:text-yellow-500 hover:bg-yellow-50 dark:hover:bg-yellow-950/20",
                ].join(" ")}
              >
                <svg className="w-3 h-3" viewBox="0 0 16 16" fill={record.is_starred ? "currentColor" : "none"}>
                  <path d="M8 1.5l1.75 3.5 3.9.57-2.82 2.74.66 3.88L8 10.35l-3.49 1.84.66-3.88-2.82-2.74 3.9-.57L8 1.5z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
                </svg>
                {record.is_starred ? "已收藏" : "收藏"}
              </button>
              <button
                onClick={() => onDelete(record.id)}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-md text-[12px] text-[var(--text-secondary)] hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-950/20 transition-colors"
              >
                <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none">
                  <path d="M2 4h12M5 4V2.5A.5.5 0 015.5 2h5a.5.5 0 01.5.5V4M6 7v5M10 7v5M3 4l1 9.5A.5.5 0 004.5 14h7a.5.5 0 00.5-.5L13 4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                删除
              </button>
            </div>
            {record.duration_ms != null && (
              <span className="text-[11px] text-[var(--text-secondary)] tabular-nums">
                {record.duration_ms}ms
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
});

export function HistoryList({
  records,
  searchQuery,
}: {
  records: TranslationRecord[];
  searchQuery: string;
}) {
  const { expandedId, setExpanded, removeRecord, toggleStar } = useHistoryStore();
  // 待确认删除的记录：单条删除此前点击即永久删除，而误点风险更高的
  // 小按钮（22px）紧邻收藏 —— 与「清空全部」的保护倒挂了
  const [pendingDelete, setPendingDelete] = useState<TranslationRecord | null>(null);

  // 函数式更新保持引用稳定：否则 expandedId 每次变化都会重建回调，
  // 令全部 memo 化的 RecordCard 在展开/收起时无谓重渲染
  const handleToggle = useCallback(
    (id: string) => setExpanded((prev) => (prev === id ? null : id)),
    [setExpanded]
  );

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await deleteHistoryRecord(id);
        removeRecord(id);
        toast("已删除", "success");
      } catch (err) {
        toast("删除失败，请重试", "error");
        console.error(err);
      }
    },
    [removeRecord]
  );

  const handleStar = useCallback(
    async (id: string) => {
      try {
        const newValue = await toggleStarRecord(id);
        toggleStar(id, newValue);
      } catch (err) {
        toast("操作失败，请重试", "error");
        console.error(err);
      }
    },
    [toggleStar]
  );

  // 按时间分组渲染：记录天然按 created_at 倒序，只需在标签变化处插入分隔。
  // 没有分组时，一屏 50 条全是同质卡片，用户很难判断「这条是刚才的还是上周的」
  // —— 而时间恰恰是回忆翻译内容时最常用的线索。
  const groups: Array<{ label: string; items: TranslationRecord[] }> = [];
  for (const record of records) {
    const label = groupLabel(record.created_at);
    const last = groups[groups.length - 1];
    if (last && last.label === label) {
      last.items.push(record);
    } else {
      groups.push({ label, items: [record] });
    }
  }

  return (
    <div className="overflow-y-auto h-full px-5 py-3 space-y-2">
      {groups.map((group) => (
        <div key={group.label} className="space-y-2">
          <div className="flex items-center gap-2 pt-1 first:pt-0">
            <span className="text-[11px] font-semibold text-[var(--text-tertiary)] tracking-wide">
              {group.label}
            </span>
            <span className="flex-1 h-px bg-[var(--border-secondary)]" />
            <span className="text-[10.5px] text-[var(--text-tertiary)] tabular-nums">
              {group.items.length} 条
            </span>
          </div>
          {group.items.map((record) => (
            <RecordCard
              key={record.id}
              record={record}
              isExpanded={expandedId === record.id}
              searchQuery={searchQuery}
              onToggle={handleToggle}
              onDelete={(id) => setPendingDelete(records.find((r) => r.id === id) ?? null)}
              onStar={handleStar}
            />
          ))}
        </div>
      ))}
      {pendingDelete && (
        <ConfirmDialog
          title="删除这条记录？"
          description="删除后无法恢复"
          confirmLabel="确认删除"
          onConfirm={() => {
            const id = pendingDelete.id;
            setPendingDelete(null);
            void handleDelete(id);
          }}
          onCancel={() => setPendingDelete(null)}
        />
      )}
    </div>
  );
}
