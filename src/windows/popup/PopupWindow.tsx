// src/windows/popup/PopupWindow.tsx
// 翻译浮窗主组件（macOS 风格 — 灵动有活力）

import { useEffect, useCallback, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { EVENTS } from "@/lib/constants";
import {
  hidePopup,
  resizePopup,
  getPopupGeometry,
  type PopupGeometry,
} from "@/lib/commands";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useTranslationStore } from "@/stores/translationStore";
import { LoadingView } from "./LoadingView";
import { ResultView } from "./ResultView";
import { ErrorView } from "./ErrorView";
import { ErrorBoundary } from "./ErrorBoundary";
import { TrafficLights } from "./TrafficLights";
import { isKeyConsumingTarget, isDragBlockingTarget } from "@/lib/domUtils";
import type {
  TranslationResultPayload,
  TranslationErrorPayload,
} from "@/lib/types";

// 浮窗尺寸契约由后端 popup_geometry 下发（唯一来源，C3）。
// 此处仅保留首帧兜底值 —— get_popup_geometry 是异步 IPC，到达前需要
// 一组可渲染的尺寸；到达后即被后端权威值覆盖。
const FALLBACK_GEOMETRY: PopupGeometry = {
  width_normal: 400,
  width_wide: 520,
  height_collapsed: 44,
  height_loading: 160,
  min_width: 280,
  max_width: 520,
  min_height: 40,
  max_height: 480,
};

export function PopupWindow() {
  const { status, result, errorCode, errorMessage, setLoading, setResult, setError } =
    useTranslationStore();

  // popup 容器的 ref，用于判断点击是否在窗体内部
  const popupRef = useRef<HTMLDivElement>(null);

  // 拖拽进行中标志：阻止 onFocusChanged 在 startDragging 期间误关弹窗
  const isDragging = useRef(false);

  // ── 红绿灯窗体状态 ──
  // collapsed：折叠成仅标题栏的紧凑条（替代无任务栏可去的 minimize）
  // wide：阅读宽度（520）/ 标准宽度（400）切换（替代无意义的全屏）
  const [collapsed, setCollapsed] = useState(false);
  const [wide, setWide] = useState(false);

  // 后端权威尺寸契约（C3）：挂载时拉取一次，失败则沿用兜底值
  const [geometry, setGeometry] = useState<PopupGeometry>(FALLBACK_GEOMETRY);
  useEffect(() => {
    let cancelled = false;
    getPopupGeometry()
      .then((g) => {
        if (!cancelled) setGeometry(g);
      })
      .catch((err) => console.error("[PopupWindow] 获取尺寸契约失败，沿用兜底值:", err));
    return () => {
      cancelled = true;
    };
  }, []);

  const width = wide ? geometry.width_wide : geometry.width_normal;

  const handleClose = useCallback(() => {
    hidePopup().catch(console.error);
  }, []);

  // 折叠切换：纯状态更新，窗口尺寸统一交由下方量高效应处理（F9）。
  // 此前在 setCollapsed 的函数式 updater 内调 resizePopup —— StrictMode
  // 会双调用 updater 导致重复 IPC，且与本仓库刚修过的同类问题矛盾。
  const handleToggleCollapse = useCallback(() => {
    setCollapsed((prev) => !prev);
  }, []);

  const handleToggleWide = useCallback(() => {
    setWide((prev) => !prev);
  }, []);

  // ── 监听翻译 Loading 事件 ──
  // 新一轮翻译开始：复位折叠态（否则新结果会被压在紧凑条里看不见）
  const handleLoading = useCallback(() => {
    setLoading();
    setCollapsed(false);
    resizePopup(width, geometry.height_loading).catch(console.error);
  }, [setLoading, width, geometry.height_loading]);
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

  // ── 窗口尺寸的唯一归属：内容/几何变化后统一在此调整 ──
  // 折叠 → 固定压到标题栏高度；展开/内容变化 → 测量当前视图自然高度。
  // 不限定 result 存在：error/idle/loading 视图同样需要在展开时恢复高度。
  // 此前 `!result` 提前返回导致「折叠 → 展开」在非 success 状态下窗口
  // 永远卡在折叠高度（44px），内容被 overflow-hidden 裁切（F1）。
  useEffect(() => {
    if (collapsed) {
      resizePopup(width, geometry.height_collapsed).catch(console.error);
      return;
    }
    // 两段式量高（F6）：首次测量发生在 OS 窗口仍是旧宽度时，文本换行数
    // 与新宽度不符 —— 单次 resizePopup(新宽, 旧宽下的高) 会留白或裁切。
    // 首次 resize 落地后再补测一次，仅高度变化超过 1px 时追加第二次调用。
    let cancelled = false;
    const id = requestAnimationFrame(() => {
      const container = popupRef.current;
      if (!container) return;
      const firstH = container.offsetHeight;
      resizePopup(width, firstH)
        .then(() => {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              if (cancelled) return;
              const c2 = popupRef.current;
              if (!c2) return;
              const secondH = c2.offsetHeight;
              if (Math.abs(secondH - firstH) > 1) {
                resizePopup(width, secondH).catch(console.error);
              }
            });
          });
        })
        .catch(console.error);
    });
    return () => {
      cancelled = true;
      cancelAnimationFrame(id);
    };
  }, [status, result, collapsed, width, geometry.height_collapsed]);

  // ── 键盘快捷键：Escape / 空格 关闭，Enter 折叠切换 ────────────────
  //
  // 空格作为关闭键需要三重防护，否则会吃掉正常输入：
  //   1. 焦点在输入框/文本域/contenteditable/按钮上时放行（元素自行消费按键）
  //   2. 存在文本选区时放行（用户正在选译文，空格不该关窗）
  //   3. e.repeat 时忽略（长按不重复触发）
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        handleClose();
        return;
      }

      // " " 是空格键的标准 KeyboardEvent.key 值
      if (e.key === " ") {
        if (e.repeat || isKeyConsumingTarget(e.target)) return;
        if (!window.getSelection()?.isCollapsed) return;
        e.preventDefault();
        handleClose();
        return;
      }

      if (e.key === "Enter" && !isKeyConsumingTarget(e.target)) {
        e.preventDefault();
        handleToggleCollapse();
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handleClose, handleToggleCollapse]);

  // ── 点击窗体外部区域时关闭 ──────────────────────────────────────────
  useEffect(() => {
    const handleWindowClick = (e: MouseEvent) => {
      const popup = popupRef.current;
      if (!popup) return;
      if (!popup.contains(e.target as Node)) {
        handleClose();
      }
    };
    window.addEventListener("click", handleWindowClick);
    return () => window.removeEventListener("click", handleWindowClick);
  }, [handleClose]);

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
      // 点击交互元素或 [data-no-drag] 容器内时跳过拖拽（C5：判定收敛至 domUtils）
      if (isDragBlockingTarget(e.target)) return;
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
    // cancelled 标志修复注册竞态：onFocusChanged() 异步注册，若 cleanup 在
    // Promise resolve 前执行（StrictMode 双调用），unlisten 仍为 undefined →
    // 首个监听器永久泄漏，失焦时 hidePopup 被重复调用两次。
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    appWindow
      .onFocusChanged(({ payload: focused }) => {
        if (!focused && !isDragging.current) {
          hidePopup().catch(console.error);
        }
      })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // ── 渲染 ──
  return (
    <ErrorBoundary>
      <div
        ref={popupRef}
        className="popup-container w-full overflow-hidden cursor-grab active:cursor-grabbing select-none"
        onMouseDown={handlePopupMouseDown}
      >
        {/* ── 标题栏：红绿灯 + 折叠态摘要 ── */}
        <div className="popup-titlebar">
          <TrafficLights
            onClose={handleClose}
            onToggleCollapse={handleToggleCollapse}
            onToggleWide={handleToggleWide}
            collapsed={collapsed}
            wide={wide}
          />
          {collapsed && (
            <span className="popup-titlebar__summary">
              {status === "success" && result
                ? result.translated_text
                : status === "loading"
                  ? "翻译中…"
                  : status === "error"
                    ? "翻译失败"
                    : "等待翻译…"}
            </span>
          )}
        </div>

        {/* ── 内容区（折叠时隐藏）── */}
        {!collapsed && (
          <>
            {status === "idle" && (
              <div className="px-4 pb-4 text-xs text-[var(--text-tertiary)] text-center">
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
          </>
        )}
      </div>
    </ErrorBoundary>
  );
}
