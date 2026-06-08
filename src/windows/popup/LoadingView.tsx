// src/windows/popup/LoadingView.tsx
// 翻译浮窗加载态（macOS 骨架屏风格）

export function LoadingView() {
  return (
    <div className="p-4 space-y-3 animate-popup">
      {/* 语言标签骨架 */}
      <div className="skeleton h-3 w-24 rounded" />

      {/* 分隔线 */}
      <div className="h-px bg-[var(--border-secondary)]" />

      {/* 翻译结果骨架 — 多行展示结构化文本预留空间 */}
      <div className="space-y-2.5">
        <div className="skeleton h-3.5 w-full rounded" />
        <div className="skeleton h-3.5 w-11/12 rounded" />
        <div className="skeleton h-3.5 w-5/6 rounded" />
        <div className="skeleton h-3.5 w-4/5 rounded" />
        <div className="skeleton h-3.5 w-3/5 rounded" />
        <div className="skeleton h-3.5 w-2/3 rounded" />
      </div>

      {/* 底部操作栏骨架 */}
      <div className="h-px bg-[var(--border-secondary)]" />
      <div className="flex items-center justify-between">
        <div className="skeleton h-2.5 w-20 rounded" />
        <div className="skeleton h-2.5 w-12 rounded" />
      </div>
    </div>
  );
}
