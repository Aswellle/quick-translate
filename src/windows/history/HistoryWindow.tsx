// src/windows/history/HistoryWindow.tsx
// 翻译历史面板：搜索高亮 + 正确分页边界 + 清空确认 Toast

import { useEffect, useCallback, useRef, useState } from "react";
import { queryHistory, countHistory, clearHistory } from "@/lib/commands";
import { toast } from "@/components/ToastManager";
import { useHistoryStore } from "@/stores/historyStore";
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
    setRecords,
    setLoading,
    setSearchQuery,
    setPage,
    clearAll,
  } = useHistoryStore();

  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // 控制清空确认对话框
  const [confirmClear, setConfirmClear] = useState(false);

  /** 加载历史记录（含精确 total 计数） */
  const loadHistory = useCallback(
    async (query: string, pageNum: number) => {
      setLoading(true);
      try {
        // 并行获取列表 + 总数（精确分页边界）
        const [recs, count] = await Promise.all([
          queryHistory({
            search: query.trim() || undefined,
            limit: pageSize,
            offset: pageNum * pageSize,
          }),
          countHistory(query.trim() || undefined),
        ]);
        setRecords(recs, count);
      } catch (err) {
        toast("加载历史记录失败", "error");
        console.error(err);
      } finally {
        setLoading(false);
      }
    },
    [pageSize, setLoading, setRecords]
  );

  // 初次 + page/searchQuery 变化时加载
  useEffect(() => {
    loadHistory(searchQuery, page);
  }, [loadHistory, searchQuery, page]);

  // 搜索防抖 300ms
  const handleSearch = useCallback(
    (query: string) => {
      setSearchQuery(query);   // 立即更新 UI
      if (searchTimer.current) clearTimeout(searchTimer.current);
      searchTimer.current = setTimeout(() => {
        loadHistory(query, 0); // 防抖后发请求
      }, 300);
    },
    [loadHistory, setSearchQuery]
  );

  // 清空历史（两步确认）
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

  // 分页边界计算
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const hasPrev = page > 0;
  const hasNext = page + 1 < totalPages;

  return (
    <div className="flex flex-col h-screen bg-white dark:bg-zinc-900 text-[var(--text-primary)] select-none">
      {/* ── 标题栏 ── */}
      <div className="px-5 pt-4 pb-3 border-b border-[var(--divider)] flex items-center justify-between">
        <div>
          <h1 className="text-base font-semibold">翻译历史</h1>
          <p className="text-xs text-[var(--text-muted)] mt-0.5">
            {searchQuery
              ? `找到 ${total} 条匹配记录`
              : `共 ${total} 条记录`}
          </p>
        </div>
        <button
          onClick={handleClearRequest}
          className={[
            "text-xs px-2.5 py-1.5 rounded-lg transition-colors",
            total === 0
              ? "text-[var(--text-muted)] cursor-not-allowed opacity-40"
              : "text-red-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20",
          ].join(" ")}
          disabled={total === 0}
        >
          清空历史
        </button>
      </div>

      {/* ── 搜索栏 ── */}
      <div className="px-5 py-3 border-b border-[var(--divider)]">
        <SearchBar
          value={searchQuery}
          onChange={handleSearch}
          placeholder="搜索原文或译文关键词…"
        />
      </div>

      {/* ── 历史列表 ── */}
      <div className="flex-1 overflow-hidden">
        {isLoading ? (
          <LoadingPlaceholder />
        ) : records.length === 0 ? (
          <EmptyState hasSearch={!!searchQuery} />
        ) : (
          <HistoryList records={records} searchQuery={searchQuery} />
        )}
      </div>

      {/* ── 分页控制（正确边界） ── */}
      {total > pageSize && (
        <div className="px-5 py-3 border-t border-[var(--divider)] flex items-center justify-between">
          <button
            disabled={!hasPrev || isLoading}
            onClick={() => setPage(page - 1)}
            className={[
              "text-xs px-3 py-1.5 rounded-lg border border-[var(--divider)] transition-colors",
              hasPrev && !isLoading
                ? "hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]"
                : "opacity-30 cursor-not-allowed",
            ].join(" ")}
          >
            ← 上一页
          </button>

          <span className="text-xs text-[var(--text-muted)]">
            第 {page + 1} / {totalPages} 页
          </span>

          <button
            disabled={!hasNext || isLoading}
            onClick={() => setPage(page + 1)}
            className={[
              "text-xs px-3 py-1.5 rounded-lg border border-[var(--divider)] transition-colors",
              hasNext && !isLoading
                ? "hover:bg-[var(--hover-bg)] text-[var(--text-secondary)]"
                : "opacity-30 cursor-not-allowed",
            ].join(" ")}
          >
            下一页 →
          </button>
        </div>
      )}

      {/* ── 清空确认浮层 ── */}
      {confirmClear && (
        <ConfirmDialog
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
    <div className="p-5 space-y-3">
      {[1, 2, 3, 4, 5].map((i) => (
        <div key={i} className="space-y-2 p-3 rounded-xl border border-[var(--divider)]">
          <div className="skeleton h-3 w-3/4 rounded" />
          <div className="skeleton h-3 w-1/2 rounded" />
          <div className="skeleton h-2.5 w-1/3 rounded mt-1" />
        </div>
      ))}
    </div>
  );
}

function EmptyState({ hasSearch }: { hasSearch: boolean }) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 text-[var(--text-muted)]">
      <svg className="w-12 h-12 opacity-25" viewBox="0 0 48 48" fill="none">
        <rect x="8" y="14" width="32" height="22" rx="4" stroke="currentColor" strokeWidth="2" />
        <path d="M16 22h16M16 28h10" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        {hasSearch && (
          <circle cx="36" cy="12" r="8" fill="currentColor" fillOpacity="0.08"
            stroke="currentColor" strokeWidth="1.5" />
        )}
        {hasSearch && (
          <path d="M33 12h6M36 9v6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        )}
      </svg>
      <p className="text-sm font-medium">
        {hasSearch ? "没有找到匹配的记录" : "暂无翻译历史"}
      </p>
      {hasSearch && (
        <p className="text-xs text-[var(--text-muted)]">试试其他关键词</p>
      )}
    </div>
  );
}

function ConfirmDialog({
  onConfirm,
  onCancel,
}: {
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    // 遮罩层
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/30 dark:bg-black/50 backdrop-blur-sm"
      onClick={onCancel}
    >
      {/* 对话框 */}
      <div
        className={[
          "mx-6 p-5 rounded-2xl shadow-xl w-full max-w-xs",
          "bg-white dark:bg-zinc-800",
          "border border-[var(--divider)]",
          "animate-popup",
        ].join(" ")}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 mb-3">
          <div className="w-9 h-9 rounded-full bg-red-50 dark:bg-red-900/30 flex items-center justify-center shrink-0">
            <svg className="w-5 h-5 text-red-500" viewBox="0 0 20 20" fill="none">
              <path
                d="M9 11V7m0 6v.5M5.07 16h9.86A2 2 0 0016.8 13L11.9 4a2 2 0 00-3.8 0L3.2 13a2 2 0 001.87 3z"
                stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"
              />
            </svg>
          </div>
          <div>
            <p className="text-sm font-semibold text-[var(--text-primary)]">清空所有历史？</p>
            <p className="text-xs text-[var(--text-muted)] mt-0.5">此操作不可恢复</p>
          </div>
        </div>
        <div className="flex gap-2 justify-end mt-4">
          <button
            onClick={onCancel}
            className="px-4 py-1.5 rounded-lg text-sm border border-[var(--divider)] hover:bg-[var(--hover-bg)] transition-colors text-[var(--text-secondary)]"
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-1.5 rounded-lg text-sm bg-red-500 hover:bg-red-600 text-white transition-colors active:scale-95"
          >
            确认清空
          </button>
        </div>
      </div>
    </div>
  );
}
