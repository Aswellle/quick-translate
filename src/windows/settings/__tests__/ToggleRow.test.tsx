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
    // 清掉上一个测试遗留的容器：同 id 控件跨测试累积会让 jsdom 的
    // getElementById 快路径作用域查询错返回 null。
    document.body.innerHTML = "";
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  it("切换后状态提示就近出现并标注开启/关闭，2.5s 后淡出", async () => {
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
    // 提示在 onApply 落定后出现（真实结果而非切换瞬间的乐观文案）
    await act(async () => {});

    expect(onApply).toHaveBeenCalledWith(true);
    const status = container.querySelector('[role="status"]');
    expect(status).not.toBeNull();
    expect(status!.textContent).toBe("已开启");

    act(() => {
      vi.advanceTimersByTime(2500);
    });
    expect(container.querySelector('[role="status"]')).toBeNull();
  });

  it("开关带可访问名称（aria-label 与行标签一致）", () => {
    const onApply = vi.fn();
    act(() => {
      root.render(<ToggleRow label="剪贴板监控" value={false} onApply={onApply} />);
    });

    const toggle = container.querySelector('[role="switch"]') as HTMLButtonElement;
    expect(toggle.getAttribute("aria-label")).toBe("剪贴板监控");
    expect(toggle.getAttribute("aria-checked")).toBe("false");
  });

  it("持久化失败显示红色未生效提示，而非乐观的已开启", async () => {
    // applyConfig 失败时返回 false（内部已回滚草稿并 toast）
    const onApply = vi.fn().mockResolvedValue(false);
    act(() => {
      root.render(<ToggleRow label="剪贴板监控" value={true} onApply={onApply} />);
    });

    const toggle = container.querySelector('[role="switch"]') as HTMLButtonElement;
    act(() => {
      toggle.click();
    });
    // busy 在 IPC 落定前保持禁用（真正的防连点）
    expect(
      (container.querySelector('[role="switch"]') as HTMLButtonElement).disabled
    ).toBe(true);

    await act(async () => {});
    const status = container.querySelector('[role="status"]');
    expect(status!.textContent).toBe("未生效，已还原");
    expect(
      (container.querySelector('[role="switch"]') as HTMLButtonElement).disabled
    ).toBe(false);
  });

  it("onApply 抛错同样走红色失败提示", async () => {
    const onApply = vi.fn().mockRejectedValue(new Error("boom"));
    act(() => {
      root.render(<ToggleRow label="本地缓存" value={false} onApply={onApply} />);
    });

    act(() => {
      (container.querySelector('[role="switch"]') as HTMLButtonElement).click();
    });
    await act(async () => {});
    expect(container.querySelector('[role="status"]')!.textContent).toBe(
      "未生效，已还原"
    );
  });
});

