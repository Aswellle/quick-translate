// src/windows/history/HistoryWindow.tsx
// 翻译历史面板（macOS 简约明亮灵动风格）

import { useEffect, useCallback, useRef, useState } from "react";
import {
  queryHistory,
  countHistory,
  clearHistory,
  exportHistory,
} from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import { useHistoryStore } from "@/stores/historyStore";
import { formatAsMarkdown, formatAsHtml } from "@/lib/export";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { SearchBar } from "./SearchBar";
import { HistoryList } from "./HistoryList";

export function HistoryWindow() {
  const {
    records,
    total,
    isLoading,
    searchQuery,
    page,
    pageSize,
    starredOnly,
    setRecords,
    setLoading,
    setSearchQuery,
    setPage,
    setStarredOnly,
    clearAll,
  } = useHistoryStore();

  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const loadSeqRef = useRef(0);
  const [confirmClear, setConfirmClear] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportFormat, setExportFormat] = useState<"json" | "markdown" | "html">("markdown");

  const loadHistory = useCallback(
    async (query: string, pageNum: number, onlyStarred: boolean) => {
      // 请求序号门：并发加载（防抖窗口内的连续搜索、收藏切换）乱序返回时，
      // 只让最新一次的结果生效
      const seq = ++loadSeqRef.current;
      setLoading(true);
      try {
        const searchVal = query.trim() || undefined;
        const [recs, count] = await Promise.all([
          queryHistory({
            search: searchVal,
            limit: pageSize,
            offset: pageNum * pageSize,
            starred_only: onlyStarred || undefined,
          }),
          countHistory(searchVal, onlyStarred || undefined),
        ]);
        if (seq !== loadSeqRef.current) return;
        setRecords(recs, count);
      } catch (err) {
        if (seq === loadSeqRef.current) {
          toast("加载历史记录失败", "error");
          console.error(err);
        }
      } finally {
        if (seq === loadSeqRef.current) setLoading(false);
      }
    },
    [pageSize, setLoading, setRecords]
  );

  useEffect(() => {
    loadHistory(searchQuery, page, starredOnly);
  }, [loadHistory, searchQuery, page, starredOnly]);

  const handleSearch = useCallback(
    (query: string) => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
      searchTimer.current = setTimeout(() => {
        setSearchQuery(query);
      }, 300);
    },
    [setSearchQuery]
  );

  const handleClearRequest = useCallback(() => {
    setConfirmClear(true);
  }, []);

  const handleClearConfirm = useCallback(async () => {
    setConfirmClear(false);
    try {
      await clearHistory();
      clearAll();
      toast("历史记录已清空", "success");
    } catch {
      toast("清空失败，请重试", "error");
    }
  }, [clearAll]);

  const handleClearCancel = useCallback(() => {
    setConfirmClear(false);
  }, []);

  const handleExport = useCallback(async () => {
    setExporting(true);
    try {
      const json = await exportHistory();
      const records: Array<{
        id: string;
        source_text: string;
        translated_text: string;
        source_lang: string;
        target_lang: string;
        provider: string;
        created_at: number;
        duration_ms: number;
        is_starred: boolean;
      }> = JSON.parse(json);

      const date = new Date().toISOString().slice(0, 10);
      let content: string;
      let mimeType: string;
      let ext: string;

      if (exportFormat === "json") {
        content = json;
        mimeType = "application/json";
        ext = "json";
      } else if (exportFormat === "markdown") {
        content = formatAsMarkdown(records);
        mimeType = "text/markdown";
        ext = "md";
      } else {
        content = formatAsHtml(records);
        mimeType = "text/html";
        ext = "html";
      }
      const blob = new Blob([content], { type: mimeType });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `quicktranslate-history-${date}.${ext}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast("历史记录已导出", "success");
    } catch {
      toast("导出失败，请重试", "error");
    } finally {
      setExporting(false);
    }
  }, [exportFormat]);

  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const hasPrev = page > 0;
  const hasNext = page + 1 < totalPages;

  return (
    <div className="flex flex-col h-screen bg-[var(--bg-secondary)] dark:bg-[var(--bg-primary)] text-[var(--text-primary)] select-none">
      {/* ── 标题栏 ── */}
      <div className="px-5 pt-4 pb-3 bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border-b border-[var(--border-secondary)] flex items-center justify-between">
        <div>
          <h1
            className="text-[15px] font-semibold tracking-tight"
            style={{ fontFamily: "var(--font-display)" }}
          >
            翻译历史
          </h1>
          <p className="text-[12px] text-[var(--text-secondary)] mt-0.5">
            {searchQuery
              ? `${total} 条匹配结果`
              : starredOnly
              ? `${total} 条收藏`
              : `共 ${total} 条记录`}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {/* 格式选择 — 分段控件 */}
          <div className="inline-flex rounded-lg bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] p-0.5 gap-0.5 text-[12px]">
            {(["markdown", "html", "json"] as const).map((fmt) => (
              <button
                key={fmt}
                onClick={() => setExportFormat(fmt)}
                className={[
                  "px-2 py-1 rounded-[5px] font-medium transition-colors",
                  exportFormat === fmt
                    ? "bg-[var(--surface-primary)] dark:bg-[var(--surface-tertiary)] text-[var(--text-primary)] shadow-sm"
                    : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]",
                ].join(" ")}
              >
                {fmt === "markdown" ? "MD" : fmt.toUpperCase()}
              </button>
            ))}
          </div>
          <button
            onClick={handleExport}
            disabled={exporting || total === 0}
            className={[
              "flex items-center gap-1.5 text-[12px] px-3 py-1.5 rounded-lg border transition-all",
              total === 0 || exporting
                ? "text-[var(--text-secondary)] cursor-not-allowed opacity-50 border-[var(--border-primary)]"
                : "text-[var(--text-secondary)] border-[var(--border-primary)] hover:bg-[var(--hover-bg)] active:scale-95",
            ].join(" ")}
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
              <path d="M8 2v8M5 7l3 3 3-3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
              <path d="M3 12v1.5A1.5 1.5 0 004.5 15h7a1.5 1.5 0 001.5-1.5V12" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
            </svg>
            {exporting ? "导出中…" : "导出"}
          </button>
          <button
            onClick={handleClearRequest}
            disabled={total === 0}
            className={[
              "flex items-center gap-1.5 text-[12px] px-3 py-1.5 rounded-lg transition-all",
              total === 0
                ? "text-[var(--text-secondary)] cursor-not-allowed opacity-50"
                : "text-red-500 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-950/20 active:scale-95",
            ].join(" ")}
          >
            <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
              <path d="M2 4h12M5 4V2.5A.5.5 0 015.5 2h5a.5.5 0 01.5.5V4M6 7v5M10 7v5M3 4l1 9.5A.5.5 0 004.5 14h7a.5.5 0 00.5-.5L13 4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            清空
          </button>
        </div>
      </div>

      {/* ── 搜索栏 + 收藏过滤 ── */}
      <div className="px-5 py-3 border-b border-[var(--border-secondary)] flex items-center gap-2.5">
        <div className="flex-1">
          <SearchBar
            value={searchQuery}
            onChange={handleSearch}
            placeholder="搜索原文或译文…"
          />
        </div>
        <button
          onClick={() => setStarredOnly(!starredOnly)}
          className={[
            "flex-shrink-0 flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[12px] border transition-all",
            starredOnly
              ? "border-yellow-400/70 text-yellow-500 bg-yellow-50/80 dark:bg-yellow-950/20 shadow-sm"
              : "border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] hover:text-[var(--text-primary)]",
          ].join(" ")}
        >
          <svg
            className="w-3.5 h-3.5"
            viewBox="0 0 16 16"
            fill={starredOnly ? "currentColor" : "none"}
          >
            <path
              d="M8 1.5l1.75 3.5 3.9.57-2.82 2.74.66 3.88L8 10.35l-3.49 1.84.66-3.88-2.82-2.74 3.9-.57L8 1.5z"
              stroke="currentColor"
              strokeWidth="1.3"
              strokeLinejoin="round"
            />
          </svg>
          收藏
        </button>
      </div>

      {/* ── 历史列表 ── */}
      <div className="flex-1 overflow-hidden">
        {isLoading ? (
          <LoadingPlaceholder />
        ) : records.length === 0 ? (
          <EmptyState hasSearch={!!searchQuery} starredOnly={starredOnly} />
        ) : (
          <HistoryList records={records} searchQuery={searchQuery} />
        )}
      </div>

      {/* ── 分页控制 ── */}
      {total > pageSize && (
        <div className="px-5 py-3 border-t border-[var(--border-secondary)] bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] flex items-center justify-between">
          <div className="flex items-center gap-2">
            <button
              disabled={!hasPrev || isLoading}
              onClick={() => setPage(page - 1)}
              className={[
                "flex items-center gap-1 text-[12px] px-3 py-1.5 rounded-lg border transition-all",
                hasPrev && !isLoading
                  ? "border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] active:scale-95"
                  : "border-[var(--border-primary)] text-[var(--text-secondary)] opacity-40 cursor-not-allowed",
              ].join(" ")}
            >
              <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
                <path d="M7 2L3 6l4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
              上一页
            </button>
            <button
              disabled={!hasNext || isLoading}
              onClick={() => setPage(page + 1)}
              className={[
                "flex items-center gap-1 text-[12px] px-3 py-1.5 rounded-lg border transition-all",
                hasNext && !isLoading
                  ? "border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] active:scale-95"
                  : "border-[var(--border-primary)] text-[var(--text-secondary)] opacity-40 cursor-not-allowed",
              ].join(" ")}
            >
              下一页
              <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
                <path d="M5 2l4 4-4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </button>
          </div>
          <span className="text-[12px] text-[var(--text-secondary)] tabular-nums">
            {page + 1} / {totalPages}
          </span>
        </div>
      )}

      {/* ── 清空确认浮层 ── */}
      {confirmClear && (
        <ConfirmDialog
          title="清空所有历史？"
          description="此操作不可恢复"
          confirmLabel="确认清空"
          onConfirm={handleClearConfirm}
          onCancel={handleClearCancel}
        />
      )}
    </div>
  );
}

// ──────────── 子组件 ────────────

function LoadingPlaceholder() {
  return (
    <div className="p-5 space-y-2.5">
      {[1, 2, 3, 4, 5].map((i) => (
        <div
          key={i}
          className="space-y-2 p-3.5 rounded-xl border border-[var(--card-border)] bg-[var(--card-bg)]"
        >
          <div className="skeleton h-3 w-3/4 rounded" />
          <div className="skeleton h-3 w-1/2 rounded" />
          <div className="skeleton h-2 w-1/3 rounded mt-1.5" />
        </div>
      ))}
    </div>
  );
}

function EmptyState({
  hasSearch,
  starredOnly,
}: {
  hasSearch: boolean;
  starredOnly: boolean;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 text-[var(--text-secondary)] animate-fade-in">
      <div className="w-16 h-16 rounded-2xl bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)] flex items-center justify-center shadow-sm">
        <svg className="w-8 h-8 opacity-40" viewBox="0 0 48 48" fill="none">
          <rect x="8" y="14" width="32" height="22" rx="4" stroke="currentColor" strokeWidth="1.6" />
          <path d="M16 22h16M16 28h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          {hasSearch && (
            <>
              <circle cx="36" cy="12" r="7" stroke="currentColor" strokeWidth="1.6" />
              <path d="M33 12h6M36 9v6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
            </>
          )}
        </svg>
      </div>
      <div className="text-center space-y-1">
        <p className="text-[13px] font-medium text-[var(--text-secondary)]">
          {hasSearch
            ? "没有找到匹配的记录"
            : starredOnly
            ? "暂无收藏记录"
            : "暂无翻译历史"}
        </p>
        {hasSearch && (
          <p className="text-[12px] text-[var(--text-secondary)]">试试其他关键词</p>
        )}
        {!hasSearch && !starredOnly && (
          <p className="text-[12px] text-[var(--text-secondary)]">
            复制文字即翻译，记录会自动保存在这里
          </p>
        )}
      </div>
    </div>
  );
}

