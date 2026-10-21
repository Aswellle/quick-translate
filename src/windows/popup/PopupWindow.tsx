// src/windows/popup/PopupWindow.tsx
// 翻译浮窗主组件（macOS 风格 — 灵动有活力）

import { useEffect, useCallback, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { EVENTS, getLangName } from "@/lib/constants";
import {
  hidePopup,
  resizePopup,
  getPopupGeometry,
  activatePopup,
  rememberPopupPosition,
  setPopupTheme,
  type PopupGeometry,
} from "@/lib/commands";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useTranslationStore } from "@/stores/translationStore";
import { useConfigStore } from "@/stores/configStore";
import { LoadingView } from "./LoadingView";
import { ResultView } from "./ResultView";
import { ErrorView } from "./ErrorView";
import { ErrorBoundary } from "./ErrorBoundary";
import { TrafficLights } from "./TrafficLights";
import { isKeyConsumingTarget, isDragBlockingTarget } from "@/lib/domUtils";
import type {
  TranslationResultPayload,
  TranslationErrorPayload,
  TranslationLoadingPayload,
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

  // 当前请求 id 的同步镜像：后端 coordinator 是 latest-wins 的第一道闸门
  // （迟到的结果根本不会被 emit），这里是前端的第二道防线（计划第 6 节）——
  // 事件投递顺序被打乱时，旧结果/旧错误照样会被这道校验拦下。
  const requestIdRef = useRef<string | null>(null);

  // 拖拽/点击共用的按下起点（≤ CLICK_SLOP_PX 视为点击，超出为拖拽）
  const pressOrigin = useRef<{ x: number; y: number } | null>(null);
  const CLICK_SLOP_PX = 4;

  // ── 红绿灯窗体状态 ──
  // collapsed：折叠成仅标题栏的紧凑条（替代无任务栏可去的 minimize）
  // wide：阅读宽度（520）/ 标准宽度（400）切换（替代无意义的全屏）
  const [collapsed, setCollapsed] = useState(false);
  const [wide, setWide] = useState(false);

  // 浮窗是否已转为可交互态。被动态下键盘事件根本到不了 webview
  // （focusable(false)），"空格关闭"提示此时只是误导 —— 用户照做会把
  // 空格打进正在编辑的文档。激活成功后才允许渲染快捷键提示。
  const [activated, setActivated] = useState(false);

  // 正在播放退场动效：渲染层据此挂上淡出类
  const [closing, setClosing] = useState(false);

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
    // 90ms 淡出后再真正隐藏：窗口级 hide() 是瞬时消失，直接调用会让
    // 浮窗「啪」地断掉。淡出只动 opacity/transform（compositor 层），
    // 90ms 远低于「察觉为等待」的阈值，且不阻塞任何输入 —— 用户在淡出
    // 期间继续操作其他窗口完全不受影响。
    setClosing(true);
    window.setTimeout(() => {
      hidePopup().catch(console.error);
      setClosing(false);
    }, 90);
  }, []);

  // 折叠/宽度切换后主动 blur：点击按钮会让 DOM 焦点停留在按钮上，
  // 之后按空格会被 isKeyConsumingTarget 判定为"按钮消费按键"而放行，
  // 空格变成再次切换折叠/宽度而非关闭浮窗（只有点红色叉号能退出）。
  // 切换完成后释放焦点，空格即恢复为关闭键。
  const blurActive = useCallback(() => {
    const el = document.activeElement;
    if (el instanceof HTMLElement) el.blur();
  }, []);

  // 折叠切换：纯状态更新，窗口尺寸统一交由下方量高效应处理（F9）。
  // 此前在 setCollapsed 的函数式 updater 内调 resizePopup —— StrictMode
  // 会双调用 updater 导致重复 IPC，且与本仓库刚修过的同类问题矛盾。
  const handleToggleCollapse = useCallback(() => {
    setCollapsed((prev) => !prev);
    blurActive();
  }, [blurActive]);

  const handleToggleWide = useCallback(() => {
    setWide((prev) => !prev);
    blurActive();
  }, [blurActive]);

  // ── 监听翻译 Loading 事件 ──
  // 新一轮翻译开始：复位折叠态（否则新结果会被压在紧凑条里看不见），
  // 并复位 activated —— 新浮窗以被动态弹出，键盘事件根本到不了 webview，
  // 残留的 true 会让它渲染「空格关闭」提示而空格实际收不到（提示误导）
  const handleLoading = useCallback(
    (event: { payload: TranslationLoadingPayload }) => {
      requestIdRef.current = event.payload.request_id;
      setLoading();
      setCollapsed(false);
      setActivated(false);
      resizePopup(width, geometry.height_loading).catch(console.error);
    },
    [setLoading, width, geometry.height_loading]
  );
  useTauriEvent<TranslationLoadingPayload>(EVENTS.TRANSLATION_LOADING, handleLoading);

  // ── 监听翻译结果事件（latest-wins 前端防线）──
  const handleResult = useCallback(
    (event: { payload: TranslationResultPayload }) => {
      if (event.payload.request_id !== requestIdRef.current) {
        // 迟到的旧结果：新请求早已接管 UI，直接丢弃
        console.info("[PopupWindow] 丢弃迟到的翻译结果（request_id 不匹配）");
        return;
      }
      setResult(event.payload.result);
    },
    [setResult]
  );
  useTauriEvent<TranslationResultPayload>(EVENTS.TRANSLATION_RESULT, handleResult);

  // ── 监听翻译错误事件（同样过 latest-wins 校验）──
  const handleError = useCallback(
    (event: { payload: TranslationErrorPayload }) => {
      if (event.payload.request_id !== requestIdRef.current) {
        console.info("[PopupWindow] 丢弃迟到的翻译错误（request_id 不匹配）");
        return;
      }
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
  // 按下只记起点；mousemove 超过 SLOP_PX 才启动 OS 拖拽（此刻左键仍按住，
  // startDragging 需要活动按压才能进入模态移动循环）；抬起时位移 ≤ SLOP
  // 视为点击，转入交互态。
  //
  // 此前 mousedown 就无条件 startDragging，每次点击都在 OS 模态移动循环
  // 里空转一圈，与激活链路（set_focusable + set_focus）交错时正是
  // 「点击后 ~96ms 自关」的帮凶；统一到一个判定后点击/拖拽互不干扰。
  //
  // 拖拽禁区（.selectable / 交互元素，C5）不拖窗、也不 preventDefault
  // （后者会取消文本选区的建立），但**按下起点照记**：单击「复制」按钮
  // 同样要转交互态，否则最高频路径之后 Esc/空格全死、快捷键提示永不出现
  // —— 激活判定看的是「按下到抬起几乎没动」，与按压落在谁身上无关。
  useEffect(() => {
    const container = popupRef.current;
    if (!container) return;

    // 本次按压是否已转入 OS 拖拽：进入后 OS 接管鼠标，webview 的
    // mousemove 不再可信，用标志防止重复调用 startDragging
    let dragStarted = false;
    // 本次按压是否落在拖拽禁区上（控件/可选文本）：不拖窗，但点击
    // 激活照常生效
    let suppressDrag = false;

    const activate = () => {
      activatePopup()
        .then(() => setActivated(true))
        .catch((err) => console.error("[PopupWindow] 转入交互态失败:", err));
    };

    const handleMouseDown = (e: MouseEvent) => {
      if (e.button !== 0) return;
      suppressDrag = isDragBlockingTarget(e.target);
      if (!suppressDrag) e.preventDefault();
      pressOrigin.current = { x: e.clientX, y: e.clientY };
      dragStarted = false;
    };

    const handleMouseMove = (e: MouseEvent) => {
      const origin = pressOrigin.current;
      if (!origin || dragStarted || suppressDrag) return;
      if (Math.hypot(e.clientX - origin.x, e.clientY - origin.y) <= CLICK_SLOP_PX) return;
      dragStarted = true;
      getCurrentWindow()
        .startDragging()
        .catch((err) => console.error("[PopupWindow] 拖拽移动失败:", err));
    };

    // mouseup：判定点击或拖拽，并固化位置
    const handleMouseUp = (e: MouseEvent) => {
      const origin = pressOrigin.current;
      pressOrigin.current = null;
      if (!origin) return;

      const moved = Math.hypot(e.clientX - origin.x, e.clientY - origin.y);

      // 禁区上的按压永远不涉及窗口拖拽：划选译文（位移 >4px）不上报
      // 位置 —— 浮窗根本没动，上报会把「没动过」错误固化成拖拽位置、
      // 冲掉光标锚点；单击（≤4px）转交互态
      if (suppressDrag) {
        if (moved <= CLICK_SLOP_PX) activate();
        return;
      }

      if (moved > CLICK_SLOP_PX || dragStarted) {
        // 是拖拽：位置已定，上报后端记住（下次翻译沿用）
        rememberPopupPosition().catch((err) =>
          console.error("[PopupWindow] 上报位置失败:", err)
        );
        return;
      }

      // 是点击：转入交互态，恢复键盘快捷键（Esc / 空格 / Enter）
      activate();
    };

    container.addEventListener("mousedown", handleMouseDown);
    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp, { capture: true });
    return () => {
      container.removeEventListener("mousedown", handleMouseDown);
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp, { capture: true });
    };
  }, []);

  // ── 材质明暗跟随应用主题 ────────────────────────────────────────────
  // Windows 的 DWM 系统 backdrop（DWMWA_SYSTEMBACKDROP_TYPE）跟随**系统**
  // 主题而非应用主题：应用切到深色而系统是浅色时，浮窗背后是一块浅色
  // 亚克力，深色玻璃观感全毁。把 DWMWA_USE_IMMERSIVE_DARK_MODE 同步到
  // 与 CSS `.dark` 相同的来源（documentElement.classList），材质与着色层
  // 才能始终是同一明暗。
  //
  // macOS 侧还要多一个信息：主题偏好是不是 `system`。system 档位下必须
  // 清空窗口 appearance 让原生材质跟随系统，否则会把浮窗 webview 的
  // prefers-color-scheme 钉死，此后系统主题变化再也检测不到（D3）。
  useEffect(() => {
    const root = document.documentElement;
    // observer 的过滤粒度是整个 class 属性而非 .dark 一位 —— 未来任何
    // 往 <html> 挂别的 class 的代码都会触发 sync。缓存上次明暗值，
    // 未变化时跳过 IPC，避免无谓的 DWM 调用。
    //
    // 缓存的是 `dark|followSystem` 组合而非单个布尔：用户在设置页把
    // 主题从"深色"改成"跟随系统"而此刻系统正好是深色时，`.dark` 不变
    // 但语义变了 —— 只比对 dark 会漏掉这次切换，浮窗 appearance 卡在
    // 上一次的钉死状态。
    let lastKey: string | null = null;
    const sync = () => {
      const dark = root.classList.contains("dark");
      const followSystem = useConfigStore.getState().config?.theme === "system";
      const key = `${dark}|${followSystem}`;
      if (key === lastKey) return;
      lastKey = key;
      setPopupTheme(dark, followSystem).catch(console.error);
    };
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });

    // 主题偏好本身变化（settings 窗口保存 → theme-changed 广播 →
    // useTheme 更新 configStore）不会改 <html> 的 class 时，上面的
    // observer 收不到。订阅 store 补上这条路径。
    const unsubscribe = useConfigStore.subscribe(sync);

    // 双保险：系统主题变化（跟随系统模式）在某些 WebView2 运行时上
    // matchMedia 的 change 事件不可靠，主动轮询兜底 —— popup 窗口长期
    // 驻留，用户切了系统深色后浮窗材质应尽快跟上。轮询很便宜：只是读
    // 一次 classList，真正的 IPC 有 lastKey 守着。
    const interval = window.setInterval(sync, 2000);

    return () => {
      observer.disconnect();
      unsubscribe();
      window.clearInterval(interval);
    };
  }, []);

  // ── 渲染 ──
  return (
    <ErrorBoundary>
      <div
        ref={popupRef}
        className={[
          "popup-container w-full overflow-hidden cursor-grab active:cursor-grabbing select-none",
          closing ? "animate-popup-out" : "",
        ].join(" ")}
      >
        {/* ── 标题栏：红绿灯 + 折叠态一体化信息条 ──
            折叠态承载的必要信息按优先级：状态（点）→ 译文摘要 → 语言方向。
            翻译源标识刻意不放 —— 折叠时它属于噪音，展开即见。 */}
        <div
          className={[
            "popup-titlebar",
            collapsed ? "popup-titlebar--collapsed" : "",
          ].join(" ")}
        >
          <TrafficLights
            onClose={handleClose}
            onToggleCollapse={handleToggleCollapse}
            onToggleWide={handleToggleWide}
            collapsed={collapsed}
            wide={wide}
            activated={activated}
          />
          {collapsed && (
            <>
              {/* 状态点：颜色即状态，但颜色对屏幕阅读器不可见。
                  role="status" 让辅助技术播报状态变化；视觉用户看到的
                  仍是那个会呼吸的小圆点，文字用 sr-only 藏起来。
                  aria-live="polite" 而非 assertive：翻译是用户主动触发的，
                  不该打断正在朗读的其他内容。 */}
              <span
                className={`popup-status-dot popup-status-dot--${status}`}
                aria-hidden="true"
              />
              <span className="sr-only" role="status" aria-live="polite">
                {status === "success"
                  ? "翻译完成"
                  : status === "loading"
                    ? "正在翻译"
                    : status === "error"
                      ? "翻译失败"
                      : "等待翻译"}
              </span>
              <span className="popup-titlebar__summary">
                {status === "success" && result
                  ? result.translated_text
                  : status === "loading"
                    ? "翻译中…"
                    : status === "error"
                      ? (errorMessage ?? "翻译失败")
                      : "等待翻译…"}
              </span>
              {status === "success" && result && (
                <span className="popup-titlebar__lang">
                  {getLangName(result.detected_source_lang)}
                  <span className="popup-titlebar__lang-arrow">→</span>
                  {getLangName(result.target_lang)}
                </span>
              )}
            </>
          )}
        </div>

        {/* ── 内容区（折叠时隐藏）──
            key 绑定宽度档位：绿黄灯切换时重挂载以重放 180ms 淡入，
            掩住 OS 窗口瞬时改宽带来的文本重排跳变（纯视觉过渡，无状态损失
            —— 内容视图本身是无状态的） */}
        {!collapsed && (
          <div key={wide ? "wide" : "normal"} className="popup-content-enter">
            {status === "idle" && (
              <div className="px-4 pb-4 text-xs text-[var(--text-tertiary)] text-center">
                等待翻译…
              </div>
            )}
            {status === "loading" && <LoadingView />}
            {status === "success" && result && (
              <ResultView result={result} showKeyboardHint={activated} />
            )}
            {status === "error" && (
              <ErrorView
                code={errorCode ?? "UNKNOWN"}
                message={errorMessage ?? undefined}
              />
            )}
          </div>
        )}
      </div>
    </ErrorBoundary>
  );
}
