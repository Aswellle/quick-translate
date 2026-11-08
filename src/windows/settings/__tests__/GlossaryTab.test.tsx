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
  deleteGlossaryEntry,
  importGlossaryJson,
  listGlossaryEntries,
} from "@/lib/commands";

const mockedList = vi.mocked(listGlossaryEntries);
const mockedCreate = vi.mocked(createGlossaryEntry);
const mockedDelete = vi.mocked(deleteGlossaryEntry);
const mockedImportJson = vi.mocked(importGlossaryJson);

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
    ) => Promise<boolean>
  >();

  beforeEach(() => {
    vi.clearAllMocks();
    applyConfig.mockClear();
    // 清掉上一个测试遗留的容器：多个测试渲染同名 id 的控件后，jsdom 的
    // querySelector("#id") 走文档级 getElementById 快路径，首个同 id 元素
    // 在遗留容器里会让本容器的作用域查询错返回 null。
    document.body.innerHTML = "";
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

  // 请求序号门回归：旧语言对的迟到列表不得覆盖新语言对的结果。
  // 若没有序号门，界面会在 en→zh 标题下渲染 ja→zh 的条目，此时点
  // 「保存修改」还会把旧条目的语言对静默改写成当前对。
  it("语言对快速切换时只让最新一次加载生效", async () => {
    const staleEntry: GlossaryEntry = {
      id: 1,
      source_lang: "en",
      target_lang: "zh",
      source_term: "Apple",
      target_term: "苹果",
      note: "",
      case_sensitive: false,
      created_at: 0,
      updated_at: 0,
    };
    const freshEntry: GlossaryEntry = {
      id: 2,
      source_lang: "ja",
      target_lang: "zh",
      source_term: "桜",
      target_term: "樱花",
      note: "",
      case_sensitive: false,
      created_at: 0,
      updated_at: 0,
    };

    let resolveStale!: (v: GlossaryEntry[]) => void;
    // 第一次加载（en→zh）挂起；切语言对后第二次加载（ja→zh）立即返回
    mockedList.mockImplementationOnce(
      () => new Promise<GlossaryEntry[]>((res) => { resolveStale = res; })
    );
    mockedList.mockImplementationOnce(() => Promise.resolve([freshEntry]));

    render();
    await act(async () => {});

    const selectSetter = Object.getOwnPropertyDescriptor(
      window.HTMLSelectElement.prototype,
      "value"
    )!.set!;
    const sourceSelect = container.querySelector(
      "#glossary-source-lang"
    ) as HTMLSelectElement;
    const _m = container.querySelector("[id=glossary-source-lang]");     act(() => {
      selectSetter.call(sourceSelect, "ja");
      sourceSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => {});

    // 旧语言对的响应此刻才迟到返回
    await act(async () => {
      resolveStale([staleEntry]);
    });

    expect(container.textContent).toContain("桜 → 樱花");
    expect(container.textContent).not.toContain("Apple → 苹果");
    expect(mockedList).toHaveBeenNthCalledWith(1, "en", "zh");
    expect(mockedList).toHaveBeenNthCalledWith(2, "ja", "zh");
  });

  // 失败态与空态分离回归：toast 一闪而过，若读取失败落回「还没有术语」
  // 空态，已有术语的用户会信以为真并重复录入。
  it("读取失败显示原因与重试入口，不冒充空态", async () => {
    mockedList.mockImplementationOnce(() =>
      Promise.reject(new Error("db unavailable"))
    );
    render();
    await act(async () => {});

    expect(container.textContent).toContain("术语表读取失败");
    expect(container.textContent).toContain("db unavailable");
    expect(container.textContent).not.toContain("该语言对还没有术语");

    // 重试成功后回到正常列表渲染
    mockedList.mockImplementationOnce(() => Promise.resolve([]));
    await act(async () => {
      clickByText(container, "重试");
    });
    expect(container.textContent).toContain("该语言对还没有术语");
    expect(container.textContent).not.toContain("术语表读取失败");
  });

  // 破坏性操作守卫回归：删除与导入替换都是不可恢复动作，必须先确认；
  // 与「清空该语言对」的确认标准对齐（历史窗口单条删除同款）。
  it("单条删除先确认：取消不调用，确认才删除", async () => {
    const entry: GlossaryEntry = {
      id: 7,
      source_lang: "en",
      target_lang: "zh",
      source_term: "Apple",
      target_term: "苹果",
      note: "",
      case_sensitive: false,
      created_at: 0,
      updated_at: 0,
    };
    mockedList.mockResolvedValue([entry]);
    mockedDelete.mockResolvedValue(true);
    render();
    await act(async () => {});

    clickByText(container, "删除");
    expect(container.textContent).toContain("删除这条术语？");
    expect(mockedDelete).not.toHaveBeenCalled();

    clickByText(container, "取消");
    expect(mockedDelete).not.toHaveBeenCalled();

    clickByText(container, "删除");
    await act(async () => {
      clickByText(container, "确认删除");
    });
    expect(mockedDelete).toHaveBeenCalledWith(7);
  });

  it("导入先确认再替换：确认后才读取文件内容", async () => {
    mockedList.mockResolvedValue([]);
    mockedImportJson.mockResolvedValue(2);
    render();
    await act(async () => {});

    const input = container.querySelector(
      'input[accept=".json,application/json"]'
    ) as HTMLInputElement;
    const file = new File(
      ['[{"source_term":"A","target_term":"B"}]'],
      "g.json",
      { type: "application/json" }
    );
    Object.defineProperty(input, "files", { value: [file] });
    act(() => {
      input.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(container.textContent).toContain("导入并替换当前语言对的术语？");
    expect(mockedImportJson).not.toHaveBeenCalled();

    await act(async () => {
      clickByText(container, "确认导入");
    });
    expect(mockedImportJson).toHaveBeenCalledWith(
      "en",
      "zh",
      '[{"source_term":"A","target_term":"B"}]'
    );
  });
});
