// src/windows/popup/ErrorBoundary.tsx
// React Error Boundary — catches render errors in child components

import { Component, type ReactNode, type ErrorInfo } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  errorMessage: string;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, errorMessage: "" };
  }

  static getDerivedStateFromError(error: unknown): State {
    const message =
      error instanceof Error ? error.message : "未知渲染错误";
    return { hasError: true, errorMessage: message };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error("[ErrorBoundary] 渲染错误:", error, info.componentStack);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="px-4 py-5 text-center space-y-3">
          <div className="w-10 h-10 rounded-full bg-red-50 dark:bg-red-950/30 mx-auto flex items-center justify-center">
            <svg className="w-5 h-5 text-red-500" viewBox="0 0 20 20" fill="none">
              <path d="M10 6v4M10 13v.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              <circle cx="10" cy="10" r="8.5" stroke="currentColor" strokeWidth="1.5" />
            </svg>
          </div>
          <p className="text-[13px] font-medium text-[var(--text-secondary)]">渲染出错</p>
          <p className="text-[11px] text-[var(--text-tertiary)] break-words px-4">
            {this.state.errorMessage}
          </p>
          <button
            onClick={() => this.setState({ hasError: false, errorMessage: "" })}
            className="text-[12px] px-4 py-1.5 rounded-lg border border-[var(--border-primary)] hover:bg-[var(--hover-bg)] transition-colors text-[var(--text-secondary)]"
          >
            重试
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
