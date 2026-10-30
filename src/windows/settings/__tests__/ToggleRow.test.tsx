// src/windows/settings/__tests__/ToggleRow.test.tsx
// 开关行「就近生效提示」契约：切换开关后，状态提示必须出现在按钮
// 邻近的空白区域（role=status），而不是只存在于底部状态栏 —— 用户盯着
// 开关看时视线不该跨过整屏去找结果。

// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";

import { ToggleRow } from "../SettingsWindow";

describe("ToggleRow 就近生效提示", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  it("切换后状态提示就近出现并标注开启/关闭，2.5s 后淡出", () => {
    const onApply = vi.fn();
    act(() => {
      root.render(<ToggleRow label="开机自启动" value={false} onApply={onApply} />);
    });

    const toggle = container.querySelector('[role="switch"]') as HTMLButtonElement;
    expect(toggle).not.toBeNull();
    expect(container.querySelector('[role="status"]')).toBeNull();

    act(() => {
      toggle.click();
    });

    expect(onApply).toHaveBeenCalledWith(true);
    const status = container.querySelector('[role="status"]');
    expect(status).not.toBeNull();
    expect(status!.textContent).toBe("已开启");

    act(() => {
      vi.advanceTimersByTime(2500);
    });
    expect(container.querySelector('[role="status"]')).toBeNull();
  });
});
