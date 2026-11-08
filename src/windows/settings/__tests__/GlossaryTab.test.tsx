// src/windows/settings/__tests__/GlossaryTab.test.tsx
// 术语表管理页的行为契约（计划 Task 10）：
// - 按语言对加载条目
// - 新增术语携带语言对与大小写选项
// - 当前翻译源能力如实说明（DeepL 支持 / 其他不支持）

// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";
import { GlossaryTab } from "../GlossaryTab";
import type { AppConfig, GlossaryEntry } from "@/lib/commands";

vi.mock("@/lib/commands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/commands")>();
  return {
    ...actual,
    listGlossaryEntries: vi.fn(),
    createGlossaryEntry: vi.fn(),
    updateGlossaryEntry: vi.fn(),
    deleteGlossaryEntry: vi.fn(),
    clearGlossary: vi.fn(),
    importGlossaryJson: vi.fn(),
    importGlossaryCsv: vi.fn(),
    exportGlossaryJson: vi.fn(),
    exportGlossaryCsv: vi.fn(),
  };
});

vi.mock("@/components/ToastManager", () => ({ toast: vi.fn() }));

import {
  createGlossaryEntry,
  listGlossaryEntries,
} from "@/lib/commands";

const mockedList = vi.mocked(listGlossaryEntries);
const mockedCreate = vi.mocked(createGlossaryEntry);

function config(provider: string): AppConfig {
  return {
    target_lang: "zh",
    provider,
    deepl_api_key: "",
    tencent_secret_id: "",
    tencent_secret_key: "",
    baidu_app_id: "",
    baidu_secret_key: "",
    youdao_app_key: "",
    youdao_app_secret: "",
    auto_start: false,
    history_limit: 200,
    theme: "system",
    fallback_enabled: true,
    onboarding_completed: true,
    clipboard_monitor_enabled: true,
    sensitive_filter_enabled: true,
    reduce_transparency: false,
    export_dir: "",
    cache_enabled: true,
    hotkey_translate_clipboard: "CmdOrControl+Alt+T",
    hotkey_open_manual: "CmdOrControl+Alt+M",
    hotkey_toggle_monitor: "CmdOrControl+Alt+P",
    hotkey_toggle_popup: "CmdOrControl+Alt+H",
    hotkey_retranslate_last: "CmdOrControl+Alt+R",
    glossary_enabled: true,
    baidu_use_term_base: false,
    youdao_vocab_id: "",
    tencent_term_repo_ids: "",
    translation_style: "natural",
  };
}

function setInputByLabel(container: HTMLElement, label: string, value: string) {
  const input = container.querySelector(
    `input[aria-label="${label}"]`
  ) as HTMLInputElement;
  expect(input, `输入框 ${label} 必须存在`).toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
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

describe("GlossaryTab", () => {
  let container: HTMLElement;
  let root: Root;
  const applyConfig = vi.fn<
    (
      key: keyof AppConfig,
      value: AppConfig[keyof AppConfig],
      feedback: "inline" | "toast" | null
    ) => void
  >();

  beforeEach(() => {
    vi.clearAllMocks();
    applyConfig.mockClear();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  function render(provider = "deepl") {
    act(() => {
      root.render(
        <GlossaryTab draft={config(provider)} applyConfig={applyConfig} />
      );
    });
  }

  it("按语言对加载并展示条目", async () => {
    const entry: GlossaryEntry = {
      id: 1,
      source_lang: "en",
      target_lang: "zh",
      source_term: "Apple",
      target_term: "苹果",
      note: "公司名",
      case_sensitive: false,
      created_at: 0,
      updated_at: 0,
    };
    mockedList.mockResolvedValue([entry]);
    render();
    await act(async () => {});
    expect(mockedList).toHaveBeenCalledWith("en", "zh");
    expect(container.textContent).toContain("Apple → 苹果");
    expect(container.textContent).toContain("公司名");
  });

  it("新增术语携带当前语言对与大小写选项", async () => {
    mockedList.mockResolvedValue([]);
    mockedCreate.mockResolvedValue({
      id: 2,
      source_lang: "en",
      target_lang: "zh",
      source_term: "API",
      target_term: "接口",
      note: "",
      case_sensitive: true,
      created_at: 0,
      updated_at: 0,
    });
    render();
    await act(async () => {});

    setInputByLabel(container, "原文术语", "API");
    setInputByLabel(container, "目标译法", "接口");
    const checkbox = container.querySelector(
      'input[type="checkbox"]'
    ) as HTMLInputElement;
    act(() => {
      checkbox.click();
    });
    await act(async () => {
      clickByText(container, "添加术语");
    });

    expect(mockedCreate).toHaveBeenCalledWith({
      source_lang: "en",
      target_lang: "zh",
      source_term: "API",
      target_term: "接口",
      note: "",
      case_sensitive: true,
    });
  });

  it("当前翻译源不支持术语表时如实说明", async () => {
    mockedList.mockResolvedValue([]);
    render("google");
    await act(async () => {});
    expect(container.textContent).toContain("不支持术语表");
  });

  it("百度翻译源提供控制台术语库开关", async () => {
    mockedList.mockResolvedValue([]);
    render("baidu");
    await act(async () => {});
    // 第一个开关是「启用术语表」，第二个是百度控制台术语库
    const switches = container.querySelectorAll('[role="switch"]');
    expect(switches.length).toBeGreaterThanOrEqual(2);
    act(() => {
      (switches[1] as HTMLButtonElement).click();
    });
    expect(applyConfig).toHaveBeenCalledWith(
      "baidu_use_term_base",
      true,
      "inline"
    );
  });

  it("腾讯云术语库 ID 失焦提交", async () => {
    mockedList.mockResolvedValue([]);
    render("tencent");
    await act(async () => {});
    const input = container.querySelector(
      'input[aria-label="腾讯云术语库 ID"]'
    ) as HTMLInputElement;
    expect(input).toBeTruthy();
    const setter = Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype,
      "value"
    )!.set!;
    act(() => {
      input.focus();
      setter.call(input, "repo-1,repo-2");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.blur();
    });
    expect(applyConfig).toHaveBeenCalledWith(
      "tencent_term_repo_ids",
      "repo-1,repo-2",
      "toast"
    );
  });
});
