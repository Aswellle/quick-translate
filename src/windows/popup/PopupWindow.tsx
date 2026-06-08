// src/windows/popup/PopupWindow.tsx
// 翻译浮窗主组件（macOS 风格 — 灵动有活力）

import { useEffect, useCallback, useRef } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { EVENTS } from "@/lib/constants";
import { hidePopup, resizePopup } from "@/lib/commands";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useTranslationStore } from "@/stores/translationStore";
import { LoadingView } from "./LoadingView";
import { ResultView } from "./ResultView";
import { ErrorView } from "./ErrorView";
import { ErrorBoundary } from "./ErrorBoundary";
import type {
  TranslationResultPayload,
  TranslationErrorPayload,
} from "@/lib/types";

export function PopupWindow() {
  const { status, result, errorCode, errorMessage, setLoading, setResult, setError } =
    useTranslationStore();

  // popup 容器的 ref，用于判断点击是否在窗体内部
  const popupRef = useRef<HTMLDivElement>(null);

  // 拖拽进行中标志：阻止 onFocusChanged 在 startDragging 期间误关弹窗
  const isDragging = useRef(false);

  // ── 监听翻译 Loading 事件 ──
  const handleLoading = useCallback(() => {
    setLoading();
    // 重置为骨架屏高度（LoadingView ≈ 160px）
    resizePopup(400, 160).catch(console.error);
  }, [setLoading]);
  useTauriEvent<unknown>(EVENTS.TRANSLATION_LOADING, handleLoading);

  // ── 监听翻译结果事件 ──
  const handleResult = useCallback(
    (event: { payload: TranslationResultPayload }) => {
      setResult(event.payload.result);
    },
    [setResult]
  );
  useTauriEvent<TranslationResultPayload>(EVENTS.TRANSLATION_RESULT, handleResult);

  // ── 监听翻译错误事件 ──
  const handleError = useCallback(
    (event: { payload: TranslationErrorPayload }) => {
      setError(event.payload.code, event.payload.message);
    },
    [setError]
  );
  useTauriEvent<TranslationErrorPayload>(EVENTS.TRANSLATION_ERROR, handleError);

  // ── 翻译结果到达后，测量内容动态调整窗口大小 ──
  useEffect(() => {
    if (!result) return;
    const id = requestAnimationFrame(() => {
      const container = popupRef.current;
      if (!container) return;
      const totalH = container.offsetHeight;
      resizePopup(400, totalH).catch(console.error);
    });
    return () => cancelAnimationFrame(id);
  }, [result]);

  // ── 仅 Escape 关闭浮窗（不影响文本选择等其他按键行为）─────────────
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        hidePopup().catch(console.error);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // ── 点击窗体外部区域时关闭 ──────────────────────────────────────────
  useEffect(() => {
    const handleWindowClick = (e: MouseEvent) => {
      const popup = popupRef.current;
      if (!popup) return;
      if (!popup.contains(e.target as Node)) {
        hidePopup().catch(console.error);
      }
    };
    window.addEventListener("click", handleWindowClick);
    return () => window.removeEventListener("click", handleWindowClick);
  }, []);

  // ── 拖拽移动窗体 ────────────────────────────────────────────────────
  // 整个窗口区域可拖拽：mousedown 触发 Tauri startDragging
  // 点击交互元素（按钮等）时跳过拖拽
  //
  // 修复：调用 startDragging() 前置 isDragging=true，
  //   阻止 onFocusChanged 在 OS 层拖拽期间误触 hidePopup。
  //   重置时机：document.mouseup（鼠标释放）或 finally 块（+100ms 延迟），
  //   两路兜底，防止 startDragging 立即 resolve 或 mouseup 未到达 webview 的边缘情况。
  useEffect(() => {
    const container = popupRef.current;
    if (!container) return;

    const handleMouseDown = async (e: MouseEvent) => {
      // 仅响应左键
      if (e.button !== 0) return;
      // 如果点击目标是交互元素（按钮、输入框等），跳过拖拽
      const target = e.target as HTMLElement;
      if (
        target.tagName === "BUTTON" ||
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.closest("[data-no-drag]")
      ) {
        return;
      }
      e.preventDefault();
      isDragging.current = true;
      try {
        const win = getCurrentWindow();
        await win.startDragging();
      } catch (err) {
        console.error("[PopupWindow] 拖拽移动失败:", err);
      } finally {
        // 延迟重置：给拖拽结束后可能滞留的 onFocusChanged 事件留 100ms 缓冲
        setTimeout(() => {
          isDragging.current = false;
        }, 100);
      }
    };

    // mouseup 兜底重置：当 startDragging() 立即 resolve 时确保标志能被清除
    const handleMouseUp = () => {
      if (isDragging.current) {
        setTimeout(() => {
          isDragging.current = false;
        }, 100);
      }
    };

    container.addEventListener("mousedown", handleMouseDown);
    document.addEventListener("mouseup", handleMouseUp, { capture: true });
    return () => {
      container.removeEventListener("mousedown", handleMouseDown);
      document.removeEventListener("mouseup", handleMouseUp, { capture: true });
    };
  }, []);

  // ── 点击窗体内部时阻止默认行为（防止失去焦点导致自动关闭）──────────
  const handlePopupMouseDown = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
  }, []);

  // ── 窗口失焦到其他应用时关闭（点击其他 app 窗口时）───────────────
  // 守卫：拖拽进行中（isDragging=true）时跳过关闭，
  //   因为 startDragging() 会导致 OS 层短暂 blur，不应触发 hidePopup。
  useEffect(() => {
    const appWindow = getCurrentWebviewWindow();
    let unlisten: (() => void) | undefined;

    appWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused && !isDragging.current) {
          hidePopup().catch(console.error);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => unlisten?.();
  }, []);

  // ── 渲染 ──
  return (
    <ErrorBoundary>
      <div
        ref={popupRef}
        className="popup-container w-full min-h-[60px] overflow-hidden cursor-grab active:cursor-grabbing select-none"
        style={{ minWidth: "280px", maxWidth: "520px" }}
        onMouseDown={handlePopupMouseDown}
      >
        {status === "idle" && (
          <div className="px-4 py-4 text-xs text-[var(--text-tertiary)] text-center">
            等待翻译…
          </div>
        )}
        {status === "loading" && <LoadingView />}
        {status === "success" && result && <ResultView result={result} />}
        {status === "error" && (
          <ErrorView
            code={errorCode ?? "UNKNOWN"}
            message={errorMessage ?? undefined}
          />
        )}
      </div>
    </ErrorBoundary>
  );
}
