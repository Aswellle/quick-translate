// src/windows/manual/__tests__/ManualSensitiveGate.test.tsx
// 手动窗口敏感内容确认契约（评审整改）：
// - 后端过滤命中（SENSITIVE_CONTENT_BLOCKED）→ 弹确认框，错误位不显示
// - 确认放行 → 携带 force=true 重新请求
// - 取消 → 不再发起任何请求

// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";
import { ManualTranslateWindow } from "../ManualTranslateWindow";
import { translateText } from "@/lib/commands";
import { useConfigStore } from "@/stores/configStore";

vi.mock("@/lib/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/commands")>();
  return {
    ...actual,
    translateText: vi.fn(),
  };
});

const mockedTranslate = vi.mocked(translateText);

/** 受控 textarea 写入（原生 setter 绕过 React 的值覆盖） */
function typeText(container: HTMLElement, value: string) {
  const input = container.querySelector("textarea") as HTMLTextAreaElement;
  expect(input, "输入框必须存在").toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype,
    "value"
  )!.set!;
  act(() => {
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function clickByText(container: HTMLElement, text: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (b) => (b.textContent ?? "").trim() === text
  );
  expect(button, `按钮「${text}」必须存在`).toBeTruthy();
  act(() => {
    button!.click();
  });
}

describe("ManualTranslateWindow 敏感内容确认", () => {
  let container: HTMLElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    useConfigStore.setState({ config: null, isLoaded: false });
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(<ManualTranslateWindow />);
    });
    typeText(container, "Authorization: Bearer sk-xxx");
  });

  it("过滤命中时弹确认框且错误位不显示", async () => {
    mockedTranslate.mockRejectedValueOnce({
      code: "SENSITIVE_CONTENT_BLOCKED",
      message: "该内容疑似包含 HTTP 鉴权头，未自动发送到翻译服务",
    });

    clickByText(container, "翻译");
    await act(async () => {});

    expect(mockedTranslate).toHaveBeenCalledTimes(1);
    expect(
      Array.from(container.querySelectorAll("button")).some(
        (b) => (b.textContent ?? "").trim() === "仍要翻译"
      ),
      "确认框必须出现"
    ).toBe(true);
    expect(
      container.textContent,
      "敏感拦截不是失败，不得进错误位"
    ).not.toContain("疑似包含");
  });

  it("确认后携带 force=true 重新请求并展示译文", async () => {
    mockedTranslate
      .mockRejectedValueOnce({
        code: "SENSITIVE_CONTENT_BLOCKED",
        message: "该内容疑似包含 HTTP 鉴权头，未自动发送到翻译服务",
      })
      .mockResolvedValueOnce({
        source_text: "Authorization: Bearer sk-xxx",
        translated_text: "授权：承载 sk-xxx",
        detected_source_lang: "en",
        target_lang: "zh",
        provider: "deepl",
        duration_ms: 12,
        truncated: false,
      });

    clickByText(container, "翻译");
    await act(async () => {});
    clickByText(container, "仍要翻译");
    await act(async () => {});

    expect(mockedTranslate).toHaveBeenCalledTimes(2);
    expect(mockedTranslate.mock.calls[1][3]).toBe(true);
    expect(container.textContent).toContain("授权：承载 sk-xxx");
  });

  it("取消后不再发起任何请求", async () => {
    mockedTranslate.mockRejectedValueOnce({
      code: "SENSITIVE_CONTENT_BLOCKED",
      message: "该内容疑似包含 HTTP 鉴权头，未自动发送到翻译服务",
    });

    clickByText(container, "翻译");
    await act(async () => {});
    clickByText(container, "取消");
    await act(async () => {});

    expect(mockedTranslate).toHaveBeenCalledTimes(1);
    expect(
      Array.from(container.querySelectorAll("button")).some(
        (b) => (b.textContent ?? "").trim() === "仍要翻译"
      ),
      "确认框必须关闭"
    ).toBe(false);
  });
});
