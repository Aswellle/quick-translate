// src/lib/__tests__/domUtils.test.ts
// DOM 元素语义判定的回归测试：F2（快捷键吞掉按钮原生激活）与
// C5（svg 字形点击触发窗口拖拽）都源于这里的名单/路径漂移。

// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { isDragBlockingTarget, isKeyConsumingTarget } from "../domUtils";

function el<K extends keyof HTMLElementTagNameMap>(tag: K): HTMLElement {
  return document.createElement(tag);
}

describe("isKeyConsumingTarget", () => {
  it("让位给表单输入类控件", () => {
    expect(isKeyConsumingTarget(el("input"))).toBe(true);
    expect(isKeyConsumingTarget(el("textarea"))).toBe(true);
    expect(isKeyConsumingTarget(el("select"))).toBe(true);
  });

  it("让位给可激活控件（Enter/Space 即点击）", () => {
    expect(isKeyConsumingTarget(el("button"))).toBe(true);
    expect(isKeyConsumingTarget(el("a"))).toBe(true);
  });

  it("让位给 contenteditable 区域", () => {
    const div = el("div");
    // jsdom 未实现 contentEditable 反射，直接注入属性值
    Object.defineProperty(div, "isContentEditable", { value: true });
    expect(isKeyConsumingTarget(div)).toBe(true);
  });

  it("普通元素不消费按键", () => {
    expect(isKeyConsumingTarget(el("div"))).toBe(false);
    expect(isKeyConsumingTarget(el("p"))).toBe(false);
    expect(isKeyConsumingTarget(null)).toBe(false);
  });
});

describe("isDragBlockingTarget", () => {
  it("包含全部按键消费型目标", () => {
    expect(isDragBlockingTarget(el("input"))).toBe(true);
    expect(isDragBlockingTarget(el("button"))).toBe(true);
  });

  it("svg/path 字形经 data-no-drag 祖先命中阻断（C5 回归）", () => {
    const group = el("div");
    group.setAttribute("data-no-drag", "");
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    svg.appendChild(path);
    group.appendChild(svg);
    // svg/path 不是 HTMLElement，必须经 Element.closest 命中祖先
    expect(isDragBlockingTarget(path)).toBe(true);
    expect(isDragBlockingTarget(svg)).toBe(true);
    expect(isDragBlockingTarget(group)).toBe(true);
  });

  it("无 data-no-drag 祖先的普通元素可拖拽", () => {
    const div = el("div");
    const child = el("span");
    div.appendChild(child);
    expect(isDragBlockingTarget(child)).toBe(false);
  });

  it("译文文本区（.selectable）阻断拖拽，划选不再变成拖窗", () => {
    const wrap = el("div");
    const p = el("p");
    p.className = "selectable px-4 py-3.5";
    const inner = el("span");
    p.appendChild(inner);
    wrap.appendChild(p);
    // 命中元素本身、内部子元素都应阻断 —— mousedown 上的 preventDefault
    // 会取消文本选区的建立，不避开这里用户划选译文会启动整窗拖拽
    expect(isDragBlockingTarget(p)).toBe(true);
    expect(isDragBlockingTarget(inner)).toBe(true);
    // 外层容器仍可拖拽
    expect(isDragBlockingTarget(wrap)).toBe(false);
  });
});
