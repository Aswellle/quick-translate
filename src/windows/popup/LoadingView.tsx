// src/windows/popup/LoadingView.tsx
// 翻译浮窗加载态（骨架屏）

export function LoadingView() {
  return (
    <div className="p-4 space-y-3 animate-popup">
      {/* 语言标签骨架 */}
      <div className="skeleton h-3 w-28 rounded" />

      {/* 分隔线 */}
      <div className="h-px bg-[var(--divider)]" />

      {/* 翻译结果骨架 */}
      <div className="space-y-2">
        <div className="skeleton h-3.5 w-full rounded" />
        <div className="skeleton h-3.5 w-4/5 rounded" />
        <div className="skeleton h-3.5 w-3/5 rounded" />
      </div>

      {/* 底部操作栏骨架 */}
      <div className="h-px bg-[var(--divider)]" />
      <div className="flex items-center justify-between">
        <div className="skeleton h-3 w-14 rounded" />
        <div className="skeleton h-3 w-10 rounded" />
      </div>
    </div>
  );
}
