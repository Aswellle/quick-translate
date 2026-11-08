// src/windows/popup/__tests__/ErrorView.test.tsx
// 错误视图的恢复操作契约（计划 Task 07 验收：「错误操作可以直接引导用户修复」）。
//
// 分流规则：AUTH_ERROR / QUOTA_EXHAUSTED 的修复动作在配置里（补凭证/换源），
// 指向「前往设置」；其余类别的实际重试路径是「重新复制文字」（剪贴板监控
// 会在新内容到来时自动重译）——两条路径都必须真的出现在 UI 上，而不是
// 只让用户自己想办法。

// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { createRoot } from "react-dom/client";
import { act } from "react";
import { ErrorView } from "../ErrorView";

vi.mock("@/lib/commands", () => ({
  openSettingsWindow: vi.fn().mockResolvedValue(undefined),
}));

function render(props: Partial<Parameters<typeof ErrorView>[0]> = {}) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => {
    root.render(<ErrorView code="TIMEOUT" {...props} />);
  });
  return { container, root };
}

function settingsButton(container: HTMLElement): HTMLButtonElement | null {
  return (
    Array.from(container.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("前往设置")
    ) ?? null
  );
}

describe("ErrorView 恢复操作分流", () => {
  it("鉴权与额度类错误给出「前往设置」按钮", () => {
    for (const code of ["AUTH_ERROR", "QUOTA_EXHAUSTED"]) {
      const { container } = render({ code });
      const btn = settingsButton(container);
      expect(btn, `${code} 必须出现前往设置按钮`).not.toBeNull();
      // 可访问名不依赖 hover：按钮文本本身就是 accessible name
      expect(btn!.textContent).toContain("前往设置");
    }
  });

  it("重试类错误不出现设置按钮，但告知真实重试路径", () => {
    for (const code of ["TIMEOUT", "NETWORK_ERROR", "RATE_LIMIT", "PROVIDER_REJECTED"]) {
      const { container } = render({ code });
      expect(settingsButton(container), `${code} 不该指向设置`).toBeNull();
      expect(container.textContent).toContain("重新复制文字即可重试");
    }
  });

  it("未知错误码回退到 UNKNOWN 文案而不是空白", () => {
    const { container } = render({ code: "SOMETHING_NEW" });
    expect(container.textContent?.length).toBeGreaterThan(0);
    expect(container.textContent).toContain("重新复制文字即可重试");
  });

  it("未知错误码携带 message 时优先展示后端消息", () => {
    const { container } = render({ code: "SOMETHING_NEW", message: "上游返回了意外格式" });
    expect(container.textContent).toContain("上游返回了意外格式");
  });

  it("已知错误码显示契约文案，不显示后端原始消息", () => {
    // 错误文案契约（constants.test.ts 已锁 Rust↔前端映射）；这里验证
    // 展示层确实用的是契约文案 —— 原始 message 可能含上游响应细节
    const { container } = render({ code: "AUTH_ERROR", message: "raw: 401 from upstream" });
    expect(container.textContent).not.toContain("raw: 401 from upstream");
  });
});
