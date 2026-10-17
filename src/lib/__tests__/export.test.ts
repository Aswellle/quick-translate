// src/lib/__tests__/export.test.ts
// 历史导出格式化的回归测试：HTML 版此前把 null 耗时渲染成 "null ms"
// （markdown 版是正确的），这类模板逻辑埋在组件文件里时没有任何测试
// 触达。抽到 lib/export.ts 后在这里钉住 null / 注入 / 空表三种边界。

import { describe, expect, it } from "vitest";
import { formatAsHtml, formatAsMarkdown, type ExportRecord } from "../export";

function record(overrides: Partial<ExportRecord> = {}): ExportRecord {
  return {
    id: "r1",
    source_text: "hello",
    translated_text: "你好",
    source_lang: "en",
    target_lang: "zh",
    provider: "deepl",
    created_at: 1730000000000,
    duration_ms: 123,
    is_starred: false,
    ...overrides,
  };
}

describe("formatAsMarkdown", () => {
  it("包含语言方向、引擎、原文与译文", () => {
    const md = formatAsMarkdown([record()]);
    expect(md).toContain("# QuickTranslate 翻译历史");
    expect(md).toContain("## en → zh　`deepl`");
    expect(md).toContain("**原文**");
    expect(md).toContain("hello");
    expect(md).toContain("**译文**");
    expect(md).toContain("你好");
    expect(md).toContain("耗时 123 ms");
  });

  it("null 耗时不渲染耗时字段（与 HTML 版同一口径）", () => {
    const md = formatAsMarkdown([record({ duration_ms: null })]);
    expect(md).not.toContain("耗时");
    expect(md).not.toContain("null");
  });

  it("收藏记录带星标", () => {
    const md = formatAsMarkdown([record({ is_starred: true })]);
    expect(md).toContain("⭐");
  });

  it("空列表导出合法的空文档", () => {
    const md = formatAsMarkdown([]);
    expect(md).toContain("共 0 条记录");
  });
});

describe("formatAsHtml", () => {
  it("包含表格结构与全部字段", () => {
    const html = formatAsHtml([record()]);
    expect(html).toContain("<table>");
    expect(html).toContain("hello");
    expect(html).toContain("你好");
    expect(html).toContain("deepl");
    expect(html).toContain("123 ms");
  });

  it("null 耗时输出空单元格而不是 'null ms'（回归）", () => {
    const html = formatAsHtml([record({ duration_ms: null })]);
    expect(html).not.toContain("null");
  });

  it("用户文本中的 HTML 元素被转义", () => {
    const html = formatAsHtml([
      record({ source_text: "<script>alert(1)</script>", translated_text: "a & b <c>" }),
    ]);
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("a &amp; b &lt;c&gt;");
  });

  it("空列表导出合法的空表格", () => {
    const html = formatAsHtml([]);
    expect(html).toContain("共 0 条记录");
    expect(html).toContain("</html>");
  });
});
