// src/windows/history/SearchBar.tsx
// 搜索框（macOS 风格）
//
// 修复：使用 localValue 本地状态作为输入框的受控值，
// 避免 value={storeQuery}（防抖后才更新）导致每次按键后输入框立即回退。

import { useState, useEffect } from "react";

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

export function SearchBar({ value, onChange, placeholder }: SearchBarProps) {
  // 本地态：即时响应用户输入；父组件的 value 变化（如外部清空）时同步
  const [localValue, setLocalValue] = useState(value);

  useEffect(() => {
    setLocalValue(value);
  }, [value]);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setLocalValue(v);
    onChange(v);
  };

  const handleClear = () => {
    setLocalValue("");
    onChange("");
  };

  return (
    <div className="relative">
      {/* 搜索图标 */}
      <div className="absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none">
        <svg
          className="w-3.5 h-3.5 text-[var(--text-tertiary)]"
          viewBox="0 0 16 16"
          fill="none"
        >
          <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4" />
          <path d="M10.5 10.5L13 13" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
      </div>
      <input
        type="text"
        value={localValue}
        onChange={handleChange}
        placeholder={placeholder ?? "搜索…"}
        className={[
          "w-full pl-8.5 pr-8 py-2 text-[13px]",
          "rounded-lg",
          "border border-[var(--border-primary)]",
          "bg-[var(--surface-primary)] dark:bg-[var(--surface-secondary)]",
          "text-[var(--text-primary)] placeholder:text-[var(--text-placeholder)]",
          "focus:outline-none focus:border-[var(--system-blue)]",
          "focus:shadow-[0_0_0_3px_rgba(0,122,255,0.18)]",
          "transition-all duration-150",
        ].join(" ")}
      />
      {localValue && (
        <button
          onClick={handleClear}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 p-0.5 rounded text-[var(--text-tertiary)] hover:text-[var(--text-secondary)] hover:bg-[var(--hover-bg)] transition-colors"
        >
          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
        </button>
      )}
    </div>
  );
}
