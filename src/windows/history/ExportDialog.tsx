// src/windows/history/ExportDialog.tsx
// 历史导出确认对话框：导出前的最后一站。
//
// 流程：点「导出」→ 本对话框（格式 / 条数 / 默认文件名 / 保存位置去向）
// → 确认后走后端落盘（export_dir 已设置直写，未设置弹原生保存对话框）。
// 此前点「导出」直接开写，格式选择器孤零零躺在标题栏、用户对「会写到
// 哪里、写多少条」没有确认机会；导出又属于会产生外部产物的动作，
// 值得一次明确的确认。
//
// 复用 ConfirmDialog 的键盘语义（Escape 取消、Tab 循环），但内容体
// 自持 —— 确认对话框组件绑定的是红色警示形态，不强行套用。

import { useEffect, useRef } from "react";

interface ExportDialogProps {
  /** 已选导出格式（由标题栏分段控件维护，本对话框内可就地改选） */
  format: "markdown" | "html" | "json";
  onFormatChange: (fmt: "markdown" | "html" | "json") => void;
  /** 将被导出的记录条数 */
  count: number;
  /** 按当前配置解析出的保存位置说明（后端 export_dir / 每次询问） */
  destination: string;
  /** 默认文件名（含日期与扩展名） */
  fileName: string;
  onConfirm: () => void;
  onCancel: () => void;
}

const FORMAT_ROWS: Array<{
  id: "markdown" | "html" | "json";
  label: string;
  ext: string;
  note: string;
}> = [
  { id: "markdown", label: "Markdown", ext: ".md", note: "轻量排版，适合笔记与归档" },
  { id: "html", label: "HTML", ext: ".html", note: "带样式的独立网页，浏览器直接打开" },
  { id: "json", label: "JSON", ext: ".json", note: "结构化原始数据，适合备份或再导入" },
];

export function ExportDialog({
  format,
  onFormatChange,
  count,
  destination,
  fileName,
  onConfirm,
  onCancel,
}: ExportDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // 首焦点落在「取消」：导出是外部产物动作，误按 Enter 应得到安全结果
    cancelRef.current?.focus();
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
        return;
      }
      if (e.key === "Tab" && dialogRef.current) {
        const focusables = dialogRef.current.querySelectorAll<HTMLElement>(
          "button, [href], input, select"
        );
        if (focusables.length < 2) return;
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        const active = document.activeElement;
        if (!e.shiftKey && active === last) {
          e.preventDefault();
          first.focus();
        } else if (e.shiftKey && active === first) {
          e.preventDefault();
          last.focus();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [onCancel]);

  const activeExt = FORMAT_ROWS.find((r) => r.id === format)?.ext ?? "";

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/20 dark:bg-black/40 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="导出翻译历史"
        className="mx-6 p-5 rounded-2xl shadow-macos-lg w-full max-w-sm animate-scale-in bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)] border border-[var(--border-secondary)]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2.5">
          <div className="w-9 h-9 rounded-xl bg-[var(--system-blue)]/10 flex items-center justify-center shrink-0">
            <svg className="w-4.5 h-4.5 text-[var(--system-blue)]" style={{ width: 18, height: 18 }} viewBox="0 0 16 16" fill="none">
              <path d="M8 2v8M5 7l3 3 3-3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
              <path d="M3 12v1.5A1.5 1.5 0 004.5 15h7a1.5 1.5 0 001.5-1.5V12" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
            </svg>
          </div>
          <div>
            <p className="text-[13.5px] font-semibold text-[var(--text-primary)]">导出翻译历史</p>
            <p className="text-[11.5px] text-[var(--text-secondary)] mt-0.5">
              将导出 {count} 条记录
            </p>
          </div>
        </div>

        {/* 格式选择：每个选项附一句「适合什么」的说明 */}
        <div className="mt-4 space-y-1.5" role="radiogroup" aria-label="导出格式">
          {FORMAT_ROWS.map((row) => (
            <button
              key={row.id}
              type="button"
              role="radio"
              aria-checked={format === row.id}
              onClick={() => onFormatChange(row.id)}
              className={[
                "w-full text-left px-3 py-2 rounded-lg border transition-colors flex items-start gap-2.5",
                format === row.id
                  ? "border-[var(--system-blue)] bg-[var(--system-blue)]/5"
                  : "border-[var(--border-primary)] hover:bg-[var(--hover-bg)]",
              ].join(" ")}
            >
              <span
                className={[
                  "mt-1 w-3.5 h-3.5 rounded-full border flex items-center justify-center shrink-0",
                  format === row.id ? "border-[var(--system-blue)]" : "border-[var(--border-secondary)]",
                ].join(" ")}
              >
                {format === row.id && (
                  <span className="w-1.5 h-1.5 rounded-full bg-[var(--system-blue)]" />
                )}
              </span>
              <span className="min-w-0">
                <span className="text-[12.5px] font-medium text-[var(--text-primary)]">
                  {row.label}
                  <span className="text-[var(--text-tertiary)] font-normal ml-1">{row.ext}</span>
                </span>
                <span className="block text-[11.5px] text-[var(--text-secondary)] leading-snug mt-0.5">
                  {row.note}
                </span>
              </span>
            </button>
          ))}
        </div>

        {/* 去向说明：export_dir 已设置 = 直写该目录；未设置 = 保存对话框 */}
        <div className="mt-3.5 px-3 py-2 rounded-lg bg-[var(--surface-tertiary)] dark:bg-[var(--surface-secondary)]/60 border border-[var(--border-secondary)]">
          <p className="text-[11.5px] text-[var(--text-secondary)] leading-snug">
            <span className="font-medium text-[var(--text-primary)]">保存位置：</span>
            {destination}
          </p>
          <p className="text-[11px] text-[var(--text-tertiary)] mt-1 font-mono break-all">
            {fileName.replace(/\.(md|html|json)$/, "")}
            <span className="text-[var(--system-blue)]">{activeExt}</span>
          </p>
        </div>

        <div className="flex gap-2 justify-end mt-4">
          <button
            ref={cancelRef}
            onClick={onCancel}
            className="px-4 py-1.5 rounded-lg text-[12.5px] font-medium border border-[var(--border-primary)] text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--system-blue)]"
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-1.5 rounded-lg text-[12.5px] font-medium bg-[var(--system-blue)] hover:brightness-110 text-white transition-all active:scale-95 shadow-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--system-blue)]"
          >
            开始导出
          </button>
        </div>
      </div>
    </div>
  );
}
