// src/windows/manual/__tests__/ManualTranslateWindow.test.tsx
// 手动翻译窗口的行为契约（计划 Task 09）：
// - 与剪贴板路径共用同一翻译核心（经 translate_text 命令）
// - 超长文本明确提示，不无反馈截断（前端拦截 + 后端 truncated 兜底）
// - 空文本不发起请求

// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";
import { ManualTranslateWindow, MANUAL_MAX_CHARS } from "../ManualTranslateWindow";
import { useConfigStore } from "@/stores/configStore";
import type { AppConfig } from "@/lib/commands";

vi.mock("@/lib/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/commands")>();
  return {
    ...actual,
    translateText: vi.fn(),
  };
});

import { translateText } from "@/lib/commands";

const mockedTranslate = vi.mocked(translateText);

function setInput(container: HTMLElement, value: string) {
  const textarea = container.querySelector("textarea")!;
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype,
    "value"
  )!.set!;
  act(() => {
    setter.call(textarea, value);
    textarea.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** 按出现顺序选下拉框（0 = 源语言，1 = 目标语言）。
 *  jsdom 下用 id 选择器在本环境不可靠，tag + 序号更稳。 */
function setSelect(container: HTMLElement, index: number, value: string) {
  const selects = container.querySelectorAll("select");
  const select = selects[index] as HTMLSelectElement | undefined;
  expect(select, `第 ${index} 个 select 必须存在`).toBeTruthy();
  act(() => {
    select!.value = value;
    select!.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

function submitButton(container: HTMLElement): HTMLButtonElement {
  const btn = Array.from(container.querySelectorAll("button")).find((b) =>
    /翻译$|翻译中/.test(b.textContent ?? "")
  );
  expect(btn, "翻译按钮必须存在").toBeTruthy();
  return btn!;
}

describe("ManualTranslateWindow", () => {
  let container: HTMLElement;
  let root: Root;

  beforeEach(() => {
    vi.clearAllMocks();
    useConfigStore.setState({ config: null, isLoaded: false });
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  function render() {
    act(() => {
      root.render(<ManualTranslateWindow />);
    });
  }

  /** 目标语言下拉（按出现顺序第 1 个 select 的 value） */
  function targetSelectValue(container: HTMLElement): string {
    const selects = container.querySelectorAll("select");
    return (selects[1] as HTMLSelectElement).value;
  }

  /** 配置晚于组件挂载到达（App.tsx 异步 getConfig 的真实时序）：
   *  目标语言必须跟随 store，而不是被挂载瞬间的兜底值钉死。 */
  it("配置晚于挂载到达时，目标语言跟随配置更新", () => {
    render();
    expect(targetSelectValue(container)).toBe("zh");

    const cfg = { target_lang: "ja" } as AppConfig;
    act(() => {
      useConfigStore.getState().setConfig(cfg);
    });
    expect(targetSelectValue(container)).toBe("ja");
  });

  /** 用户主动选择后跟随即停：store 后续刷新不得覆盖用户的选择。 */
  it("用户选择目标语言后，配置刷新不覆盖该选择", () => {
    render();
    setSelect(container, 1, "ja");
    act(() => {
      useConfigStore.getState().setConfig({ target_lang: "en" } as AppConfig);
    });
    expect(targetSelectValue(container)).toBe("ja");
  });

  it("输入文本点击翻译：调用 translate_text 并展示结果与来源标识", async () => {
    mockedTranslate.mockResolvedValue({
      source_text: "hello",
      translated_text: "你好",
      detected_source_lang: "en",
      target_lang: "zh",
      provider: "deepl",
      duration_ms: 120,
      truncated: false,
    });
    render();
    setInput(container, "hello");
    await act(async () => {
      submitButton(container).click();
    });

    expect(mockedTranslate).toHaveBeenCalledTimes(1);
    expect(mockedTranslate).toHaveBeenCalledWith("hello", "zh", undefined, undefined);
    expect(container.textContent).toContain("你好");
    expect(container.textContent).toContain("DeepL");
  });

  it("选择显式源语言后随翻译请求下发", async () => {
    mockedTranslate.mockResolvedValue({
      source_text: "hello",
      translated_text: "你好",
      detected_source_lang: "en",
      target_lang: "zh",
      provider: "deepl",
      duration_ms: 10,
      truncated: false,
    });
    render();
    setSelect(container, 0, "en");
    setInput(container, "hello");
    await act(async () => {
      submitButton(container).click();
    });
    expect(mockedTranslate).toHaveBeenCalledWith("hello", "zh", "en", undefined);
  });

  it("空文本不发起请求", async () => {
    render();
    setInput(container, "   ");
    await act(async () => {
      submitButton(container).click();
    });
    expect(mockedTranslate).not.toHaveBeenCalled();
  });

  it("超长文本禁用按钮并明确提示，不静默截断", async () => {
    render();
    setInput(container, "a".repeat(MANUAL_MAX_CHARS + 1));
    const btn = submitButton(container);
    expect(btn.disabled).toBe(true);
    expect(container.textContent).toContain("超出上限");
    await act(async () => {
      btn.click();
    });
    expect(mockedTranslate).not.toHaveBeenCalled();
  });

  it("后端 truncated 标记转为可见提示", async () => {
    mockedTranslate.mockResolvedValue({
      source_text: "x",
      translated_text: "y",
      detected_source_lang: "en",
      target_lang: "zh",
      provider: "google",
      duration_ms: 10,
      truncated: true,
    });
    render();
    setInput(container, "x");
    await act(async () => {
      submitButton(container).click();
    });
    expect(container.textContent).toContain("超出部分未翻译");
  });

  it("错误按契约文案展示，不透传上游原始消息", async () => {
    mockedTranslate.mockRejectedValue({ code: "AUTH_ERROR", message: "raw 401" });
    render();
    setInput(container, "hello");
    await act(async () => {
      submitButton(container).click();
    });
    expect(container.textContent).not.toContain("raw 401");
    expect(container.textContent!.length).toBeGreaterThan(0);
  });

  it("缓存命中结果显示「本地缓存」而不是裸 provider 串", async () => {
    mockedTranslate.mockResolvedValue({
      source_text: "hello",
      translated_text: "你好",
      detected_source_lang: "en",
      target_lang: "zh",
      provider: "cache",
      duration_ms: 1,
      truncated: false,
    });
    render();
    setInput(container, "hello");
    await act(async () => {
      submitButton(container).click();
    });
    expect(container.textContent).toContain("本地缓存");
  });
});
