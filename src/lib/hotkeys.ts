// src/lib/hotkeys.ts
// 全局快捷键的前端纯逻辑：键盘事件 → Tauri 加速键字符串，以及展示格式化。
//
// 后端（tauri-plugin-global-shortcut）解析加速键语法并裁决合法性；
// 这里只做「把用户按下的组合键转成合理字符串」，不复制一套语法校验。

import type { AppConfig } from "./commands";

/** 动作 id → 配置键。与 Rust `system::hotkeys::ACTIONS` 一一对应。 */
export const HOTKEY_CONFIG_KEYS: Record<string, keyof AppConfig> = {
  translate_clipboard: "hotkey_translate_clipboard",
  open_manual: "hotkey_open_manual",
  toggle_monitor: "hotkey_toggle_monitor",
};

/** 纯修饰键：单独按下不构成快捷键 */
const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "CapsLock"]);

/** 需要一个名字而不是字符本身的按键（与 global-hotkey 的键名一致） */
const NAMED_KEYS: Record<string, string> = {
  " ": "Space",
  Enter: "Enter",
  Tab: "Tab",
  Escape: "Escape",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Insert: "Insert",
  Delete: "Delete",
  Backspace: "Backspace",
};

function keyToken(key: string): string | null {
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase();
  if (/^[0-9]$/.test(key)) return key;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(key)) return key;
  return NAMED_KEYS[key] ?? null;
}

/**
 * 键盘事件 → 加速键字符串（如 "CmdOrControl+Alt+T"）。
 *
 * 返回 null 表示「这不算一个可用的快捷键」：只按了修饰键、没有修饰键
 * （避免抢占普通按键）、或按键不在支持范围内。Ctrl 与 Meta 统一映射为
 * CmdOrControl —— macOS 的用户自然按 ⌘，Windows 用户自然按 Ctrl。
 */
export function acceleratorFromKeyEvent(event: {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;

  const modifiers: string[] = [];
  if (event.ctrlKey || event.metaKey) modifiers.push("CmdOrControl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (modifiers.length === 0) return null;

  const token = keyToken(event.key);
  if (!token) return null;
  return [...modifiers, token].join("+");
}

/**
 * 加速键的展示格式：macOS 用符号（⌘⌥⇧），其余平台用文字（Ctrl+Alt+Shift+）。
 * 空串表示该动作已被禁用。
 */
export function formatAccelerator(accelerator: string, isMac: boolean): string {
  if (!accelerator) return "";
  const parts = accelerator.split("+");
  const separator = isMac ? "" : "+";
  return parts
    .map((part) => {
      if (part === "CmdOrControl") return isMac ? "⌘" : "Ctrl";
      if (part === "Alt") return isMac ? "⌥" : "Alt";
      if (part === "Shift") return isMac ? "⇧" : "Shift";
      return part;
    })
    .join(separator);
}

/** 当前是否为 macOS（仅用于展示格式；不影响注册行为） */
export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Mac/i.test(navigator.userAgent);
}
