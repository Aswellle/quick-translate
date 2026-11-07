// src/lib/__tests__/hotkeys.test.ts
// 全局快捷键前端纯逻辑：键盘事件 → 加速键字符串、展示格式化。
// 合法性最终由 Rust 注册结果裁决；这里只锁定转换与展示契约。

import { describe, expect, it } from "vitest";
import {
  HOTKEY_CONFIG_KEYS,
  acceleratorFromKeyEvent,
  formatAccelerator,
} from "../hotkeys";

type KeyEventLike = {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
};

function event(partial: Partial<KeyEventLike> & { key: string }): KeyEventLike {
  return {
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    ...partial,
  };
}

describe("acceleratorFromKeyEvent", () => {
  it("Ctrl + 字母 → CmdOrControl", () => {
    expect(acceleratorFromKeyEvent(event({ key: "t", ctrlKey: true }))).toBe(
      "CmdOrControl+T"
    );
  });

  it("Meta（macOS ⌘）与 Ctrl 统一映射为 CmdOrControl", () => {
    expect(acceleratorFromKeyEvent(event({ key: "T", metaKey: true }))).toBe(
      "CmdOrControl+T"
    );
  });

  it("多修饰键按固定顺序拼接", () => {
    expect(
      acceleratorFromKeyEvent(
        event({ key: "t", ctrlKey: true, altKey: true, shiftKey: true })
      )
    ).toBe("CmdOrControl+Alt+Shift+T");
  });

  it("数字键与功能键", () => {
    expect(acceleratorFromKeyEvent(event({ key: "1", altKey: true }))).toBe(
      "Alt+1"
    );
    expect(acceleratorFromKeyEvent(event({ key: "F5", ctrlKey: true }))).toBe(
      "CmdOrControl+F5"
    );
  });

  it("空格与方向键使用具名键", () => {
    expect(
      acceleratorFromKeyEvent(event({ key: " ", ctrlKey: true }))
    ).toBe("CmdOrControl+Space");
    expect(
      acceleratorFromKeyEvent(event({ key: "ArrowUp", altKey: true }))
    ).toBe("Alt+ArrowUp");
  });

  it("只按修饰键不构成快捷键", () => {
    for (const key of ["Control", "Shift", "Alt", "Meta"]) {
      expect(
        acceleratorFromKeyEvent(event({ key, ctrlKey: true }))
      ).toBeNull();
    }
  });

  it("没有修饰键的普通按键被拒绝（不抢占全局普通键）", () => {
    expect(acceleratorFromKeyEvent(event({ key: "t" }))).toBeNull();
    expect(acceleratorFromKeyEvent(event({ key: "F5" }))).toBeNull();
  });

  it("不支持的按键返回 null", () => {
    expect(
      acceleratorFromKeyEvent(event({ key: "Dead", altKey: true }))
    ).toBeNull();
  });
});

describe("formatAccelerator", () => {
  it("Windows 使用文字修饰键", () => {
    const expected = ["Ctrl", "Alt", "T"].join("+");
    expect(formatAccelerator("CmdOrControl+Alt+T", false)).toBe(expected);
    expect(formatAccelerator("CmdOrControl+Shift+F5", false)).toBe(
      "Ctrl+Shift+F5"
    );
  });

  it("macOS 使用符号修饰键", () => {
    expect(formatAccelerator("CmdOrControl+Alt+T", true)).toBe("⌘⌥T");
  });

  it("空串表示禁用", () => {
    expect(formatAccelerator("", false)).toBe("");
    expect(formatAccelerator("", true)).toBe("");
  });
});

describe("HOTKEY_CONFIG_KEYS", () => {
  it("覆盖后端的三个动作，且映射到 AppConfig 字段", () => {
    expect(Object.keys(HOTKEY_CONFIG_KEYS).sort()).toEqual([
      "open_manual",
      "toggle_monitor",
      "translate_clipboard",
    ]);
    expect(Object.values(HOTKEY_CONFIG_KEYS)).toEqual([
      "hotkey_translate_clipboard",
      "hotkey_open_manual",
      "hotkey_toggle_monitor",
    ]);
  });
});
