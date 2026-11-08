// src/windows/settings/__tests__/CredentialField.test.tsx
// 凭证输入的显示/隐藏契约（计划 Task 08 实施 4：「凭证输入默认隐藏，
// 支持显示/隐藏」）与「blur 即提交」语义的共存验证。
//
// 关键交互：切换「显示密钥」不能把半截密钥提交出去 —— blur 即提交的
// 语义下，切换按钮若抢走焦点，输入中的凭证会在用户只想核对时被持久化。

// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { createRoot } from "react-dom/client";
import { act } from "react";
import { CredentialField } from "../SettingsWindow";
import { PROVIDERS } from "@/lib/constants";

// 真实字段条目：类型与运行时行为都和线上一致
const secretField = PROVIDERS.find((p) => p.id === "deepl")!.credentialFields[0];
const textField = PROVIDERS.find((p) => p.id === "tencent")!.credentialFields[0];

type CredentialFieldSpec = (typeof PROVIDERS)[number]["credentialFields"][number];

function render(
  field: CredentialFieldSpec = secretField,
  onCommit = vi.fn()
) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => {
    root.render(
      <CredentialField field={field} value="" configured={false} onCommit={onCommit} />
    );
  });
  return { container, root, onCommit };
}

function typeValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    "value"
  )!.set!;
  act(() => {
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("CredentialField 显示/隐藏", () => {
  it("密钥框默认隐藏，切换按钮暴露 aria 状态", () => {
    const { container } = render();
    const input = container.querySelector("input")!;
    expect(input.getAttribute("type")).toBe("password");

    const toggle = container.querySelector<HTMLButtonElement>("button")!;
    expect(toggle.getAttribute("aria-label")).toBe("显示密钥");
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
  });

  it("点击切换后明文显示，状态与文案同步翻转", () => {
    const { container } = render();
    const input = container.querySelector("input")!;
    const toggle = container.querySelector<HTMLButtonElement>("button")!;

    act(() => {
      toggle.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(input.getAttribute("type")).toBe("text");
    expect(toggle.getAttribute("aria-label")).toBe("隐藏密钥");
    expect(toggle.getAttribute("aria-pressed")).toBe("true");

    act(() => {
      toggle.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(input.getAttribute("type")).toBe("password");
  });

  it("输入半截密钥后切换显示不触发提交，blur 才提交", () => {
    const { container, onCommit } = render();
    const input = container.querySelector("input")!;
    const toggle = container.querySelector<HTMLButtonElement>("button")!;

    typeValue(input, "sk-half-typed");
    act(() => {
      input.focus();
      // 点击眼睛：mousedown preventDefault 的意图是不抢焦点 ——
      // jsdom 不模拟 mousedown 的焦点转移，这里验证的是「切换本身
      // 不产生 blur 提交」这一可观察语义
      toggle.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      toggle.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onCommit).not.toHaveBeenCalled();
    expect(input.value).toBe("sk-half-typed");

    act(() => {
      input.blur();
    });
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("sk-half-typed");
  });

  it("文本型字段（SecretId 等）没有切换按钮，恒为明文", () => {
    const { container } = render(textField);
    const input = container.querySelector("input")!;
    expect(input.getAttribute("type")).toBe("text");
    expect(container.querySelector("button")).toBeNull();
  });

  it("已配置字段的占位符说明留空即保持不变", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    act(() => {
      root.render(
        <CredentialField field={secretField} value="" configured={true} onCommit={vi.fn()} />
      );
    });
    const input = container.querySelector("input")!;
    expect(input.getAttribute("placeholder")).toBe(
      "已配置（留空保持不变，输入新值覆盖）"
    );
  });
});
