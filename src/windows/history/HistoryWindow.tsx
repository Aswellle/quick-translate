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
  const [confirmClear, setConfirmClear] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportFormat, setExportFormat] = useState<"json" | "markdown" | "html">("markdown");

  const loadHistory = useCallback(
    async (query: string, pageNum: number, onlyStarred: boolean) => {
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
            className="text-base font-semibold"
            style={{ fontFamily: "var(--font-display)" }}
          >
            翻译历史
          </h1>
          <p className="text-[11px] text-[var(--text-tertiary)] mt-0.5">
            {searchQuery
              ? `${total} 条匹配结果`
              : starredOnly
              ? `${total} 条收藏`
              : `共 ${total} 条记录`}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {/* 格式选择 */}
          <div className="flex rounded-lg border border-[var(--border-primary)] overflow-hidden text-[11px]">
            {(["markdown", "html", "json"] as const).map((fmt) => (
              <button
                key={fmt}
                onClick={() => setExportFormat(fmt)}
                className={[
                  "px-2.5 py-1 transition-colors",
                  exportFormat === fmt
                    ? "bg-[var(--system-blue)] text-white"
                    : "text-[var(--text-tertiary)] hover:bg-[var(--hover-bg)] hover:text-[var(--text-secondary)]",
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
                ? "text-[var(--text-tertiary)] cursor-not-allowed opacity-50 border-[var(--border-primary)]"
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
                ? "text-[var(--text-tertiary)] cursor-not-allowed opacity-50"
                : "text-red-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-950/20 active:scale-95",
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
              ? "border-yellow-400 text-yellow-500 bg-yellow-50 dark:bg-yellow-950/20"
              : "border-[var(--border-primary)] text-[var(--text-tertiary)] hover:bg-[var(--hover-bg)] hover:text-[var(--text-secondary)]",
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
          <button
            disabled={!hasPrev || isLoading}
            onClick={() => setPage(page - 1)}
            className={[
              "flex items-center gap-1 text-[12px] px-3.5 py-1.5 rounded-lg border transition-all",
              hasPrev && !isLoading
                ? "border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] active:scale-95"
                : "border-[var(--border-primary)] text-[var(--text-tertiary)] opacity-40 cursor-not-allowed",
            ].join(" ")}
          >
            <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
              <path d="M7 2L3 6l4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            上一页
          </button>

          <span className="text-[11px] text-[var(--text-tertiary)] tabular-nums">
            {page + 1} / {totalPages}
          </span>

          <button
            disabled={!hasNext || isLoading}
            onClick={() => setPage(page + 1)}
            className={[
              "flex items-center gap-1 text-[12px] px-3.5 py-1.5 rounded-lg border transition-all",
              hasNext && !isLoading
                ? "border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] active:scale-95"
                : "border-[var(--border-primary)] text-[var(--text-tertiary)] opacity-40 cursor-not-allowed",
            ].join(" ")}
          >
            下一页
            <svg className="w-3 h-3" viewBox="0 0 12 12" fill="none">
              <path d="M5 2l4 4-4 4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        </div>
      )}

      {/* ── 清空确认浮层 ── */}
      {confirmClear && (
        <ConfirmDialog onConfirm={handleClearConfirm} onCancel={handleClearCancel} />
      )}
    </div>
  );
}

// ──────────── 导出格式化函数 ────────────

type ExportRecord = {
  id: string;
  source_text: string;
  translated_text: string;
  source_lang: string;
  target_lang: string;
  provider: string;
  created_at: number;
  duration_ms: number;
  is_starred: boolean;
};

function formatAsMarkdown(records: ExportRecord[]): string {
  const date = new Date().toLocaleString("zh-CN");
  const lines: string[] = [
    `# QuickTranslate 翻译历史`,
    ``,
    `> 导出时间：${date}　共 ${records.length} 条记录`,
    ``,
    `---`,
    ``,
  ];
  for (const r of records) {
    const ts = new Date(r.created_at).toLocaleString("zh-CN");
    const star = r.is_starred ? " ⭐" : "";
    lines.push(`## ${r.source_lang} → ${r.target_lang}　\`${r.provider}\`${star}`);
    lines.push(``);
    lines.push(`**原文**`);
    lines.push(``);
    lines.push(r.source_text);
    lines.push(``);
    lines.push(`**译文**`);
    lines.push(``);
    lines.push(r.translated_text);
    lines.push(``);
    lines.push(`_${ts}　耗时 ${r.duration_ms} ms_`);
    lines.push(``);
    lines.push(`---`);
    lines.push(``);
  }
  return lines.join("\n");
}

function formatAsHtml(records: ExportRecord[]): string {
  const esc = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const date = new Date().toLocaleString("zh-CN");
  const rows = records
    .map((r) => {
      const ts = new Date(r.created_at).toLocaleString("zh-CN");
      return `
    <tr>
      <td>${esc(r.source_text)}</td>
      <td>${esc(r.translated_text)}</td>
      <td>${esc(r.source_lang)} → ${esc(r.target_lang)}</td>
      <td>${esc(r.provider)}</td>
      <td>${r.is_starred ? "⭐" : ""}</td>
      <td>${esc(ts)}</td>
      <td>${r.duration_ms} ms</td>
    </tr>`;
    })
    .join("\n");

  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<title>QuickTranslate 翻译历史</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; padding: 24px; color: #1a1a1a; }
  h1 { font-size: 20px; margin-bottom: 4px; }
  p.meta { color: #888; font-size: 13px; margin-bottom: 20px; }
  table { border-collapse: collapse; width: 100%; font-size: 13px; }
  th { background: #f5f5f5; padding: 8px 12px; text-align: left; border-bottom: 2px solid #ddd; }
  td { padding: 8px 12px; border-bottom: 1px solid #eee; vertical-align: top; white-space: pre-wrap; max-width: 320px; }
  tr:hover td { background: #fafafa; }
</style>
</head>
<body>
<h1>QuickTranslate 翻译历史</h1>
<p class="meta">导出时间：${esc(date)}　共 ${records.length} 条记录</p>
<table>
  <thead>
    <tr>
      <th>原文</th>
      <th>译文</th>
      <th>语言</th>
      <th>引擎</th>
      <th>收藏</th>
      <th>时间</th>
      <th>耗时</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table>
</body>
</html>`;
}

// ──────────── 子组件 ────────────

function LoadingPlaceholder() {
  return (
    <div className="p-5 space-y-3">
      {[1, 2, 3, 4, 5].map((i) => (
        <div key={i} className="space-y-2 p-3.5 rounded-xl border border-[var(--border-secondary)]">
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
    <div className="flex flex-col items-center justify-center h-full gap-3 text-[var(--text-tertiary)]">
      <svg className="w-14 h-14 opacity-20" viewBox="0 0 48 48" fill="none">
        <rect x="8" y="14" width="32" height="22" rx="4" stroke="currentColor" strokeWidth="1.5" />
        <path d="M16 22h16M16 28h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        {hasSearch && (
          <>
            <circle cx="36" cy="12" r="7" stroke="currentColor" strokeWidth="1.5" />
            <path d="M33 12h6M36 9v6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </>
        )}
      </svg>
      <p className="text-sm font-medium text-[var(--text-secondary)]">
        {hasSearch
          ? "没有找到匹配的记录"
          : starredOnly
          ? "暂无收藏记录"
          : "暂无翻译历史"}
      </p>
      {hasSearch && (
        <p className="text-[11px]">试试其他关键词</p>
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
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/20 dark:bg-black/40 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        className="mx-6 p-5 rounded-xl shadow-macos-lg w-full max-w-xs animate-scale-in bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border border-[var(--border-secondary)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 mb-3">
          <div className="w-9 h-9 rounded-full bg-red-50 dark:bg-red-950/30 flex items-center justify-center shrink-0">
            <svg className="w-5 h-5 text-red-500" viewBox="0 0 20 20" fill="none">
              <path d="M9 11V7m0 6v.5M5.07 16h9.86A2 2 0 0016.8 13L11.9 4a2 2 0 00-3.8 0L3.2 13a2 2 0 001.87 3z" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </div>
          <div>
            <p className="text-sm font-semibold">清空所有历史？</p>
            <p className="text-[11px] text-[var(--text-tertiary)] mt-0.5">此操作不可恢复</p>
          </div>
        </div>
        <div className="flex gap-2 justify-end mt-4">
          <button
            onClick={onCancel}
            className="px-4 py-1.5 rounded-lg text-[13px] border border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] transition-colors"
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-1.5 rounded-lg text-[13px] bg-[var(--system-red)] hover:bg-[#E62E24] text-white transition-colors active:scale-95"
          >
            确认清空
          </button>
        </div>
      </div>
    </div>
  );
}
