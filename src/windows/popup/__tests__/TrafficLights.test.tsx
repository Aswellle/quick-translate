// src/windows/popup/__tests__/TrafficLights.test.tsx
// 红绿灯控件的无障碍契约：键盘用户与屏幕阅读器用户不依赖 hover 与颜色。
//
// 这三条都曾经是真实缺陷：
//   · 字形只在 group:hover 时显出 —— Tab 聚焦能看到 focus ring，看不到图标
//   · 按钮只有 aria-label 写明动作，没有暴露当前状态
//   · 折叠态状态点 aria-hidden，状态完全靠颜色传达

// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { createRoot } from "react-dom/client";
import { act } from "react";
import { TrafficLights } from "../TrafficLights";

function render(props: Partial<Parameters<typeof TrafficLights>[0]> = {}) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => {
    root.render(
      <TrafficLights
        onClose={vi.fn()}
        onToggleCollapse={vi.fn()}
        onToggleWide={vi.fn()}
        collapsed={false}
        wide={false}
        activated={false}
        {...props}
      />
    );
  });
  return { container, root };
}

function buttons(container: HTMLElement): HTMLButtonElement[] {
  return Array.from(container.querySelectorAll("button"));
}

describe("TrafficLights 无障碍契约", () => {
  it("三个按钮都有可读名称，且不依赖 hover 才存在", () => {
    const { container } = render();
    const btns = buttons(container);
    expect(btns).toHaveLength(3);
    for (const b of btns) {
      expect(b.getAttribute("aria-label")?.length).toBeGreaterThan(0);
    }
  });

  it("折叠按钮用 aria-expanded 暴露开合状态", () => {
    const expanded = render({ collapsed: false });
    const [closeA, collapseA, wideA] = buttons(expanded.container);
    expect(collapseA.getAttribute("aria-expanded")).toBe("true");
    expect(closeA.hasAttribute("aria-expanded")).toBe(false);
    expect(wideA.hasAttribute("aria-expanded")).toBe(false);

    const collapsed = render({ collapsed: true });
    const [, collapseB] = buttons(collapsed.container);
    expect(collapseB.getAttribute("aria-expanded")).toBe("false");
  });

  it("宽度按钮用 aria-pressed 暴露当前档位", () => {
    const normal = render({ wide: false });
    const [, , wideNormal] = buttons(normal.container);
    expect(wideNormal.getAttribute("aria-pressed")).toBe("false");

    const wide = render({ wide: true });
    const [, , wideBtn] = buttons(wide.container);
    expect(wideBtn.getAttribute("aria-pressed")).toBe("true");
  });

  it("宽度按钮是 toggle button，不是 radiogroup 成员", () => {
    // 锁定语义选择：当前只有两档（标准 400 / 阅读 520），单按钮在两组
    // 状态间来回切，对应 WAI-ARIA 的 toggle button 模式。若将来档位
    // 增加到 3 个以上，这条测试会失败 —— 那时必须改成 radiogroup +
    // 多个 radio + aria-checked，而不是继续往 aria-pressed 上堆语义。
    const { container } = render();
    const [, , wideBtn] = buttons(container);

    expect(wideBtn.getAttribute("role")).toBeNull();
    expect(wideBtn.getAttribute("aria-checked")).toBeNull();
    expect(container.querySelector('[role="radiogroup"]')).toBeNull();
    // 容器上也不该出现 radio 语义
    for (const b of buttons(container)) {
      expect(b.getAttribute("role")).not.toBe("radio");
    }
  });

  it("折叠态下宽度按钮被禁用", () => {
    // 本用例只锁定 disabled 这一条。不要把它当成「aria-pressed 的播报行为
    // 也被验证过」—— 禁用控件的 aria-pressed 是否被读屏播报属于 AT 行为，
    // jsdom 无法验证，测试名不要许诺超出断言范围的东西。
    const { container } = render({ collapsed: true });
    const [, , wideBtn] = buttons(container);
    expect(wideBtn.disabled).toBe(true);
  });

  it("aria-label 只表达动作，不随当前状态翻转", () => {
    // 曾经的缺陷：宽度按钮的 label 是 `wide ? "标准宽度" : "阅读宽度"`，
    // 而 aria-pressed 同步表达状态。wide=true 时读屏得到「阅读宽度，已按下」
    // —— 用户无法分辨「阅读宽度」是当前状态还是点下去会变成什么。
    // 折叠按钮同理（"展开"/"折叠" 配 aria-expanded）。
    // 约定：label = 动作（点下去会发生什么），aria-* = 当前状态。
    const normal = render({ wide: false, collapsed: false });
    const [, collapseN, wideN] = buttons(normal.container);
    expect(wideN.getAttribute("aria-label")).toBe("阅读宽度");
    expect(collapseN.getAttribute("aria-label")).toBe("折叠");

    const wide = render({ wide: true, collapsed: false });
    const [, , wideW] = buttons(wide.container);
    expect(wideW.getAttribute("aria-label")).toBe("标准宽度");

    const collapsed = render({ collapsed: true });
    const [, collapseC] = buttons(collapsed.container);
    expect(collapseC.getAttribute("aria-label")).toBe("展开");
  });

  it("图标对辅助技术隐藏，避免与 aria-label 重复播报", () => {
    const { container } = render();
    for (const svg of Array.from(container.querySelectorAll("svg"))) {
      expect(svg.getAttribute("aria-hidden")).toBe("true");
    }
  });

  it("容器带 data-no-drag，保证点击不被窗口拖拽逻辑吞掉", () => {
    const { container } = render();
    expect(container.querySelector("[data-no-drag]")).not.toBeNull();
  });
});
