// src/lib/domUtils.ts
// DOM 元素语义判定工具
//
// 浮窗内有两处需要判断"这个元素是不是交互控件"：
//   1. 全局键盘快捷键：焦点在控件上时应让位，否则 preventDefault 会杀死
//      按钮的原生 Enter/Space 激活（F2 的成因）
//   2. 整窗拖拽：mousedown 落在控件上时不应启动窗口拖拽
//
// 两者此前各自维护一份 tag 名单且已经漂移（BUTTON 只在拖拽名单里），
// 新增控件时极易只改一处（C5）。收敛到本模块统一维护。

/** 表单输入类控件：这些元素需要自己消费键盘输入 */
const INPUT_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

/** 可激活控件：这些元素需要自己消费 Enter/Space */
const ACTIVATABLE_TAGS = new Set(["BUTTON", "A"]);

function toElement(target: EventTarget | null): HTMLElement | null {
  return target instanceof HTMLElement ? target : null;
}

/**
 * 该元素是否会自行消费键盘按键 —— 全局快捷键应对其让位。
 *
 * 覆盖表单控件（需要输入空格等字符）、可激活控件（Enter/Space 即点击）、
 * 以及 contenteditable 区域。
 */
export function isKeyConsumingTarget(target: EventTarget | null): boolean {
  const el = toElement(target);
  if (!el) return false;
  return (
    INPUT_TAGS.has(el.tagName) ||
    ACTIVATABLE_TAGS.has(el.tagName) ||
    el.isContentEditable
  );
}

/**
 * 该元素是否应阻止整窗拖拽。
 *
 * 比 isKeyConsumingTarget 多一条 `[data-no-drag]` 祖先判定 —— 用于标记
 * 那些本身不是控件、但内部承载控件的容器（如红绿灯分组）。
 */
export function isDragBlockingTarget(target: EventTarget | null): boolean {
  if (isKeyConsumingTarget(target)) return true;
  // 命中元素可能是 SVG（红绿灯按钮内的字形是 svg/path）。SVGElement 不是
  // HTMLElement，toElement() 会返回 null，走 Element.closest 才能找到
  // data-no-drag 祖先。此前漏掉这条路径会让字形点击触发 startDragging()，
  // 窗口进入 OS 拖拽、click 被吞，按钮看似失效（C5 回归）。
  const el = target instanceof Element ? target : null;
  return el !== null && el.closest("[data-no-drag]") !== null;
}
