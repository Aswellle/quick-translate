// src/windows/history/__tests__/FilterBar.test.tsx
// 历史筛选行契约（翻译历史增强）：
// - 选项从库内数据派生（language_pairs/providers 原样渲染，语言对给中文标签）
// - 三个维度任一变化都上抛完整筛选对象；「全部」= null
// - 有激活筛选时出现「清除筛选」，一键归零

// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";
import { FilterBar, type HistoryFilters } from "../FilterBar";
import type { HistoryFilterOptions } from "@/lib/commands";

const OPTIONS: HistoryFilterOptions = {
  language_pairs: ["en→zh", "ja→zh"],
  providers: ["baidu", "deepl", "google"],
};

function selectByLabel(container: HTMLElement, label: string): HTMLSelectElement {
  const select = container.querySelector(
    `select[aria-label="${label}"]`
  ) as HTMLSelectElement;
  expect(select, `筛选下拉「${label}」必须存在`).toBeTruthy();
  return select;
}

function setSelect(select: HTMLSelectElement, value: string) {
  act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

describe("FilterBar", () => {
  let container: HTMLElement;
  let root: Root;
  let latest: HistoryFilters | null;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    latest = null;
  });

  function render(filters: HistoryFilters, options: HistoryFilterOptions | null = OPTIONS) {
    act(() => {
      root.render(
        <FilterBar
          filters={filters}
          options={options}
          onChange={(next) => {
            latest = next;
          }}
        />
      );
    });
  }

  it("从库内数据渲染语言对与翻译源选项，语言对给中文标签", () => {
    render({ source: null, languagePair: null, provider: null });
    const pairSelect = selectByLabel(container, "语言对");
    const texts = Array.from(pairSelect.options).map((o) => o.textContent);
    expect(texts).toContain("英语 → 中文");
    expect(texts).toContain("日语 → 中文");

    const providerSelect = selectByLabel(container, "翻译源");
    const providerTexts = Array.from(providerSelect.options).map((o) => o.textContent);
    expect(providerTexts).toEqual(["全部翻译源", "百度", "DeepL", "Google"]);

    expect(
      (container.textContent ?? "").includes("清除筛选"),
      "无激活筛选时不得出现「清除筛选」"
    ).toBe(false);
  });

  it("维度切换上抛完整筛选对象（受控值由父组件回填）", () => {
    let current: HistoryFilters = { source: null, languagePair: null, provider: null };
    const renderWith = (filters: HistoryFilters) => {
      act(() => {
        root.render(
          <FilterBar
            filters={filters}
            options={OPTIONS}
            onChange={(next) => {
              latest = next;
              current = next; // 模拟 HistoryWindow 把新值写回 store 并重渲染
            }}
          />
        );
      });
    };
    renderWith(current);

    setSelect(selectByLabel(container, "来源类型"), "manual");
    expect(latest).toEqual({ source: "manual", languagePair: null, provider: null });
    renderWith(current); // 父组件把新筛选写回受控值
    expect(container.textContent).toContain("清除筛选");

    setSelect(selectByLabel(container, "语言对"), "ja→zh");
    expect(latest).toEqual({ source: "manual", languagePair: "ja→zh", provider: null });
    renderWith(current);

    setSelect(selectByLabel(container, "翻译源"), "google");
    expect(latest).toEqual({ source: "manual", languagePair: "ja→zh", provider: "google" });
    renderWith(current);

    setSelect(selectByLabel(container, "来源类型"), "");
    expect(latest).toEqual({ source: null, languagePair: "ja→zh", provider: "google" });
  });

  it("点击清除筛选把三个维度一键归零", () => {
    render({ source: "clipboard", languagePair: "en→zh", provider: "deepl" });
    const clear = Array.from(container.querySelectorAll("button")).find(
      (b) => (b.textContent ?? "").trim() === "清除筛选"
    );
    expect(clear, "有激活筛选时必须出现「清除筛选」").toBeTruthy();
    act(() => {
      clear!.click();
    });
    expect(latest).toEqual({ source: null, languagePair: null, provider: null });
  });

  it("选项尚未到达（null）时下拉仍可用（只剩全部项）", () => {
    render({ source: null, languagePair: null, provider: null }, null);
    const pairSelect = selectByLabel(container, "语言对");
    expect(pairSelect.options.length).toBe(1);
    // 来源类型不依赖选项清单，永远可筛
    expect(selectByLabel(container, "来源类型").options.length).toBe(3);
  });
});
