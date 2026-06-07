// src/windows/history/HistoryList.tsx
// 历史记录列表（macOS 风格 — 简约明亮灵动）

import { useCallback, memo, useMemo } from "react";
import { CopyButton } from "@/components/CopyButton";
import { getLangName, PROVIDER_LABELS } from "@/lib/constants";
import { useHistoryStore } from "@/stores/historyStore";
import { deleteHistoryRecord, toggleStarRecord } from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import type { TranslationRecord } from "@/lib/commands";

const TRUNCATE_LEN = 80;

function truncate(text: string, len = TRUNCATE_LEN) {
  return text.length > len ? text.slice(0, len) + "…" : text;
}

function formatTime(ms: number) {
  return new Date(ms).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
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
        "rounded-xl overflow-hidden transition-all duration-150 cursor-pointer",
        "border",
        isExpanded
          ? "bg-blue-50/30 dark:bg-blue-950/10 border-blue-200/60 dark:border-blue-800/40 shadow-sm"
          : "bg-[var(--card-bg)] border-[var(--card-border)] hover:border-blue-200/60 dark:hover:border-blue-700/40 hover:shadow-sm",
      ].join(" ")}
      onClick={() => onToggle(record.id)}
    >
      {/* ── 折叠视图 ── */}
      <div className="px-4 py-3">
        <div className="flex items-start justify-between gap-2">
          <div className="flex-1 min-w-0 space-y-1">
            {/* 原文 */}
            <p className="text-[11px] text-[var(--text-tertiary)] truncate font-mono">
              <Highlight text={truncate(record.source_text)} query={searchQuery} />
            </p>
            {/* 译文 */}
            <p className="text-[13px] text-[var(--text-primary)] truncate leading-snug">
              <Highlight text={truncate(record.translated_text)} query={searchQuery} />
            </p>
          </div>
          {/* 收藏星标 + 展开箭头 */}
          <div className="flex items-center gap-1.5 mt-1 flex-shrink-0">
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
          <span className="text-[10px] text-[var(--text-tertiary)]">
            {getLangName(record.source_lang)} → {getLangName(record.target_lang)}
          </span>
          <span className="text-[10px] text-[var(--text-tertiary)] bg-[var(--surface-tertiary)] px-1.5 py-px rounded-full">
            {PROVIDER_LABELS[record.provider] ?? record.provider}
          </span>
          <span className="text-[10px] text-[var(--text-tertiary)] ml-auto">
            {formatTime(record.created_at)}
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
            <p className="text-[10px] font-semibold text-[var(--text-tertiary)] uppercase tracking-wide mb-1">
              原文
            </p>
            <p className="selectable text-[12px] text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap break-words font-mono">
              <Highlight text={record.source_text} query={searchQuery} />
            </p>
          </div>
          <div>
            <p className="text-[10px] font-semibold text-[var(--text-tertiary)] uppercase tracking-wide mb-1">
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
                  "inline-flex items-center gap-1 px-2 py-1 rounded-md text-[11px] transition-colors",
                  record.is_starred
                    ? "text-yellow-500 hover:text-yellow-600 hover:bg-yellow-50 dark:hover:bg-yellow-950/20"
                    : "text-[var(--text-tertiary)] hover:text-yellow-500 hover:bg-yellow-50 dark:hover:bg-yellow-950/20",
                ].join(" ")}
              >
                <svg className="w-3 h-3" viewBox="0 0 16 16" fill={record.is_starred ? "currentColor" : "none"}>
                  <path d="M8 1.5l1.75 3.5 3.9.57-2.82 2.74.66 3.88L8 10.35l-3.49 1.84.66-3.88-2.82-2.74 3.9-.57L8 1.5z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
                </svg>
                {record.is_starred ? "已收藏" : "收藏"}
              </button>
              <button
                onClick={() => onDelete(record.id)}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-[var(--text-tertiary)] hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-950/20 transition-colors"
              >
                <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none">
                  <path d="M2 4h12M5 4V2.5A.5.5 0 015.5 2h5a.5.5 0 01.5.5V4M6 7v5M10 7v5M3 4l1 9.5A.5.5 0 004.5 14h7a.5.5 0 00.5-.5L13 4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                删除
              </button>
            </div>
            {record.duration_ms != null && (
              <span className="text-[10px] text-[var(--text-tertiary)] tabular-nums">
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

  const handleToggle = useCallback(
    (id: string) => setExpanded(expandedId === id ? null : id),
    [expandedId, setExpanded]
  );

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await deleteHistoryRecord(id);
        removeRecord(id);
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

  return (
    <div className="overflow-y-auto h-full px-5 py-3 space-y-2">
      {records.map((record) => (
        <RecordCard
          key={record.id}
          record={record}
          isExpanded={expandedId === record.id}
          searchQuery={searchQuery}
          onToggle={handleToggle}
          onDelete={handleDelete}
          onStar={handleStar}
        />
      ))}
    </div>
  );
}
