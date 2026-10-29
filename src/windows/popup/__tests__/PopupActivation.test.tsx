// src/windows/popup/__tests__/PopupActivation.test.tsx
// 浮窗激活时序契约：
//
//   1. mouseup 发起的激活 IPC 必须晚于 click 事件派发 —— click 先行、
//      激活随后，红绿灯按钮的 onClick 才不会被激活链路的 OS 级干扰吞掉
//   2. 错误浮窗自动激活 —— 被动浮窗收不到键盘事件，Esc/空格全落进底层
//      应用；错误需要用户处置，激活后现有关闭键立即生效

// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";

// 命令层全部 mock：激活是否被调用、何时被调用是本测试的对象
vi.mock("@/lib/commands", () => ({
  hidePopup: vi.fn(async () => {}),
  resizePopup: vi.fn(async () => {}),
  getPopupGeometry: vi.fn(async () => ({
    width_normal: 400,
    width_wide: 520,
    height_collapsed: 44,
    height_loading: 160,
    min_width: 280,
    max_width: 520,
    min_height: 40,
    max_height: 480,
  })),
  activatePopup: vi.fn(async () => {}),
  rememberPopupPosition: vi.fn(async () => {}),
  setPopupTheme: vi.fn(async () => {}),
  setPopupTransparency: vi.fn(async () => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ startDragging: vi.fn(async () => {}) }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { PopupWindow } from "../PopupWindow";
import { activatePopup } from "@/lib/commands";
import { useTranslationStore } from "@/stores/translationStore";

// jsdom 没有实现 matchMedia，PopupWindow 的透明度同步链路需要它
function installMatchMedia() {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
    }),
  });
}

describe("浮窗激活时序", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    installMatchMedia();
    vi.mocked(activatePopup).mockClear();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(<PopupWindow />);
    });
    useTranslationStore.getState().reset();
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  it("点击后激活 IPC 延迟到 click 派发之后才发出", () => {
    const target = container.querySelector(".popup-container") as HTMLElement;
    expect(target).not.toBeNull();

    act(() => {
      target.dispatchEvent(
        new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 10 })
      );
      target.dispatchEvent(
        new MouseEvent("mouseup", { clientX: 10, clientY: 10 })
      );
    });

    expect(
      activatePopup,
      "click 事件尚未派发，激活 IPC 不得已经发出"
    ).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(0);
    });
    expect(activatePopup).toHaveBeenCalledTimes(1);
  });

  it("错误浮窗出现即自动激活，Esc/空格随之可用", () => {
    act(() => {
      useTranslationStore.getState().setError("E_TEST", "测试错误");
    });
    expect(activatePopup).toHaveBeenCalledTimes(1);
  });
});
