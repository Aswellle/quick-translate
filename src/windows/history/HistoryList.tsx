// src/windows/history/HistoryList.tsx
// 历史记录列表：卡片式展示、点击展开、搜索关键词高亮

import { useCallback, memo } from "react";
import { CopyButton } from "@/components/CopyButton";
import { getLangName } from "@/lib/constants";
import { useHistoryStore } from "@/stores/historyStore";
import type { TranslationRecord } from "@/lib/commands";

const PROVIDER_LABELS: Record<string, string> = {
  deepl: "DeepL",
  google: "Google",
};

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

/**
 * 将文本中匹配关键词的部分用 <mark> 高亮
 * 使用 split+filter 避免 dangerouslySetInnerHTML 的 XSS 风险
 */
function Highlight({ text, query }: { text: string; query: string }) {
  if (!query.trim()) return <>{text}</>;

  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = text.split(new RegExp(`(${escaped})`, "gi"));

  return (
    <>
      {parts.map((part, i) =>
        part.toLowerCase() === query.toLowerCase() ? (
          <mark
            key={i}
            className="bg-yellow-200 dark:bg-yellow-600/50 text-[var(--text-primary)] rounded-[2px] px-[1px]"
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
}

const RecordCard = memo(function RecordCard({
  record,
  isExpanded,
  searchQuery,
  onToggle,
}: RecordCardProps) {
  return (
    <div
      className={[
        "border border-[var(--divider)] rounded-xl overflow-hidden",
        "transition-colors duration-150 cursor-pointer",
        "hover:border-blue-200 dark:hover:border-blue-700",
        isExpanded
          ? "bg-blue-50/40 dark:bg-blue-900/10 border-blue-200/70 dark:border-blue-800/50"
          : "bg-white dark:bg-zinc-800/50",
      ].join(" ")}
      onClick={() => onToggle(record.id)}
    >
      {/* ── 折叠视图 ── */}
      <div className="px-4 py-3">
        <div className="flex items-start justify-between gap-2">
          <div className="flex-1 min-w-0 space-y-1">
            {/* 原文（截断 + 高亮） */}
            <p className="text-xs text-[var(--text-muted)] truncate">
              <Highlight text={truncate(record.source_text)} query={searchQuery} />
            </p>
            {/* 译文（截断 + 高亮） */}
            <p className="text-sm text-[var(--text-primary)] truncate font-medium">
              <Highlight text={truncate(record.translated_text)} query={searchQuery} />
            </p>
          </div>
          {/* 展开/折叠箭头 */}
          <svg
            className={[
              "flex-shrink-0 w-3.5 h-3.5 text-[var(--text-muted)] mt-1",
              "transition-transform duration-200",
              isExpanded ? "rotate-180" : "",
            ].join(" ")}
            viewBox="0 0 16 16"
            fill="none"
          >
            <path
              d="M4 6l4 4 4-4"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </div>
        {/* 元信息行 */}
        <div className="flex items-center gap-2 mt-1.5">
          <span className="text-[10px] text-[var(--text-muted)]">
            {getLangName(record.source_lang)} → {getLangName(record.target_lang)}
          </span>
          <span className="text-[10px] text-[var(--text-muted)] bg-[var(--hover-bg)] px-1.5 py-px rounded">
            {PROVIDER_LABELS[record.provider] ?? record.provider}
          </span>
          <span className="text-[10px] text-[var(--text-muted)] ml-auto">
            {formatTime(record.created_at)}
          </span>
        </div>
      </div>

      {/* ── 展开视图（完整内容 + 高亮） ── */}
      {isExpanded && (
        <div
          className="border-t border-[var(--divider)] px-4 py-3 space-y-3"
          onClick={(e) => e.stopPropagation()}
        >
          <div>
            <p className="text-[10px] font-medium text-[var(--text-muted)] mb-1">原文</p>
            <p className="selectable text-xs text-[var(--text-secondary)] leading-relaxed whitespace-pre-wrap break-words">
              <Highlight text={record.source_text} query={searchQuery} />
            </p>
          </div>
          <div>
            <p className="text-[10px] font-medium text-[var(--text-muted)] mb-1">译文</p>
            <p className="selectable text-sm text-[var(--text-primary)] leading-relaxed whitespace-pre-wrap break-words">
              <Highlight text={record.translated_text} query={searchQuery} />
            </p>
          </div>
          <div className="flex items-center justify-between pt-1">
            <CopyButton text={record.translated_text} />
            {record.duration_ms != null && (
              <span className="text-[10px] text-[var(--text-muted)]">
                耗时 {record.duration_ms}ms
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
  const { expandedId, setExpanded } = useHistoryStore();

  const handleToggle = useCallback(
    (id: string) => {
      setExpanded(expandedId === id ? null : id);
    },
    [expandedId, setExpanded]
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
        />
      ))}
    </div>
  );
}
