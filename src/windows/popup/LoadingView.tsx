// src/windows/popup/LoadingView.tsx
// 翻译浮窗加载态（macOS 骨架屏风格）

export function LoadingView() {
  return (
    <div className="px-4 pb-4 pt-1 space-y-3 animate-popup">
      {/* 语言标签骨架 */}
      <div className="skeleton h-3 w-24 rounded" />

      {/* 翻译结果骨架 — 多行展示结构化文本预留空间。
          套用与 ResultView 相同的阅读衬底：加载态与结果态之间若衬底
          有无不一致，切换时会看到一块区域突然变暗，像是闪了一下。 */}
      <div className="px-2">
        <div className="popup-reading-surface px-3 py-3 space-y-2.5">
          <div className="skeleton h-3.5 w-full rounded" />
          <div className="skeleton h-3.5 w-11/12 rounded" />
          <div className="skeleton h-3.5 w-5/6 rounded" />
          <div className="skeleton h-3.5 w-4/5 rounded" />
          <div className="skeleton h-3.5 w-3/5 rounded" />
          <div className="skeleton h-3.5 w-2/3 rounded" />
        </div>
      </div>

      {/* 底部操作栏骨架 */}
      <div className="h-px bg-[var(--popup-divider)]" />
      <div className="flex items-center justify-between">
        <div className="skeleton h-2.5 w-20 rounded" />
        <div className="skeleton h-2.5 w-12 rounded" />
      </div>
    </div>
  );
}
