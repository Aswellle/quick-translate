// src/components/__tests__/Toast.test.tsx
// Toast 语义契约：
// - 带动作的 toast 不自动消失（它是敏感拦截的唯一放行入口），必须提供
//   显式关闭路径；点击动作即收起
// - 普通 toast 到时自动消失
// - 错误条目以 role=alert 播报（assertive，不被 polite 队列拖延）

// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";
import { ToastItem } from "../Toast";

describe("ToastItem", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("普通 toast 到时自动消失", () => {
    const onClose = vi.fn();
    act(() => {
      root.render(<ToastItem id={1} message="已保存" type="success" duration={3500} onClose={onClose} />);
    });

    act(() => {
      vi.advanceTimersByTime(3500 + 300);
    });
    expect(onClose).toHaveBeenCalledWith(1);
  });

  it("带动作的 toast 不自动消失，点动作即收起", () => {
    const onClose = vi.fn();
    const onAction = vi.fn();
    act(() => {
      root.render(
        <ToastItem
          id={2}
          message="疑似敏感内容"
          type="warning"
          duration={3500}
          action={{ id: "translate_filtered", label: "仍然翻译" }}
          onAction={onAction}
          onClose={onClose}
        />
      );
    });

    act(() => {
      vi.advanceTimersByTime(3500 + 300);
    });
    expect(onClose).not.toHaveBeenCalled();

    const actionBtn = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent === "仍然翻译"
    );
    expect(actionBtn).toBeTruthy();
    act(() => {
      actionBtn!.click();
    });
    expect(onAction).toHaveBeenCalledWith("translate_filtered");
    expect(onClose).toHaveBeenCalledWith(2);
  });

  it("带动作的 toast 提供 × 显式关闭路径", () => {
    const onClose = vi.fn();
    const onAction = vi.fn();
    act(() => {
      root.render(
        <ToastItem
          id={3}
          message="疑似敏感内容"
          type="warning"
          action={{ id: "translate_filtered", label: "仍然翻译" }}
          onAction={onAction}
          onClose={onClose}
        />
      );
    });

    const closeBtn = container.querySelector('button[aria-label="关闭通知"]') as HTMLButtonElement;
    expect(closeBtn).toBeTruthy();
    act(() => {
      closeBtn.click();
    });
    expect(onClose).toHaveBeenCalledWith(3);
    expect(onAction).not.toHaveBeenCalled();
  });

  it("错误条目以 role=alert 播报（管理器传入）", () => {
    act(() => {
      root.render(
        <ToastItem id={4} message="导出失败" type="error" role="alert" onClose={vi.fn()} />
      );
    });
    expect(container.querySelector('[role="alert"]')).not.toBeNull();
  });

  it("非错误条目不抢占 assertive 通道", () => {
    act(() => {
      root.render(<ToastItem id={5} message="已生效" type="success" onClose={vi.fn()} />);
    });
    expect(container.querySelector('[role="alert"]')).toBeNull();
  });
});
