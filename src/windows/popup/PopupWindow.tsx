// src/windows/popup/PopupWindow.tsx
// 翻译浮窗主组件：监听 Tauri 事件 → 更新状态 → 渲染对应视图

import { useEffect, useCallback } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { EVENTS } from "@/lib/constants";
import { hidePopup } from "@/lib/commands";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useTranslationStore } from "@/stores/translationStore";
import { LoadingView } from "./LoadingView";
import { ResultView } from "./ResultView";
import { ErrorView } from "./ErrorView";
import type {
  TranslationResultPayload,
  TranslationErrorPayload,
} from "@/lib/types";

export function PopupWindow() {
  const { status, result, errorCode, errorMessage, setLoading, setResult, setError } =
    useTranslationStore();

  // ── 监听翻译 Loading 事件 ──
  useTauriEvent<unknown>(EVENTS.TRANSLATION_LOADING, () => {
    setLoading();
  });

  // ── 监听翻译结果事件 ──
  useTauriEvent<TranslationResultPayload>(
    EVENTS.TRANSLATION_RESULT,
    (event) => {
      setResult(event.payload.result);
    }
  );

  // ── 监听翻译错误事件 ──
  useTauriEvent<TranslationErrorPayload>(
    EVENTS.TRANSLATION_ERROR,
    (event) => {
      setError(event.payload.code, event.payload.message);
    }
  );

  // ── Esc 关闭浮窗 ──
  const handleKeyDown = useCallback(
    async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        await hidePopup();
      }
    },
    []
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // ── 点击浮窗外部（失焦）关闭 ──
  useEffect(() => {
    const appWindow = getCurrentWebviewWindow();
    let unlisten: (() => void) | undefined;

    appWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused) {
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
    <div
      className="popup-container w-full min-h-[60px] overflow-hidden"
      style={{ minWidth: "200px", maxWidth: "480px" }}
    >
      {status === "idle" && (
        <div className="px-4 py-3 text-xs text-[var(--text-muted)] text-center">
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
  );
}
