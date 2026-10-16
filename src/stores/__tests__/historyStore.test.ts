// src/stores/__tests__/historyStore.test.ts
// historyStore reducer 测试：removeRecord/setRecords 的页码回退逻辑
// 此前删除末页唯一记录会让用户困在空页上（翻页条同时消失），这类
// 跨字段一致性用组件测试很难触达，直接钉在 store 层。

import { describe, expect, beforeEach, it } from "vitest";
import { useHistoryStore } from "../historyStore";
import type { TranslationRecord } from "@/lib/commands";

function rec(id: string): TranslationRecord {
  return {
    id,
    source_text: `src-${id}`,
    translated_text: `dst-${id}`,
    source_lang: "en",
    target_lang: "zh",
    provider: "deepl",
    created_at: 1730000000000,
    duration_ms: 100,
    is_starred: false,
  };
}

describe("historyStore", () => {
  beforeEach(() => {
    useHistoryStore.setState({
      records: [],
      total: 0,
      page: 0,
      pageSize: 50,
      searchQuery: "",
      starredOnly: false,
      expandedId: null,
      isLoading: false,
    });
  });

  it("removeRecord 移除记录并递减 total", () => {
    const s = useHistoryStore.getState();
    s.setRecords([rec("a"), rec("b")], 2);
    useHistoryStore.getState().removeRecord("a");
    const after = useHistoryStore.getState();
    expect(after.records.map((r) => r.id)).toEqual(["b"]);
    expect(after.total).toBe(1);
  });

  it("删除末页最后一条时页码回退一页（回归）", () => {
    const s = useHistoryStore.getState();
    // 第 2 页（index 1）只有一条，共 51 条
    s.setRecords([rec("last")], 51);
    useHistoryStore.setState({ page: 1 });
    useHistoryStore.getState().removeRecord("last");
    const after = useHistoryStore.getState();
    expect(after.page).toBe(0);
    expect(after.total).toBe(50);
  });

  it("第一页删除不回退页码", () => {
    const s = useHistoryStore.getState();
    s.setRecords([rec("a")], 1);
    useHistoryStore.getState().removeRecord("a");
    expect(useHistoryStore.getState().page).toBe(0);
  });

  it("setRecords 返回空页且非首页时回退一页（另一窗口清空的场景）", () => {
    const s = useHistoryStore.getState();
    useHistoryStore.setState({ page: 2 });
    useHistoryStore.getState().setRecords([], 100);
    const after = useHistoryStore.getState();
    expect(after.page).toBe(1);
    expect(after.total).toBe(100);
  });

  it("首页空结果不触发回退（正常空态）", () => {
    useHistoryStore.getState().setRecords([], 0);
    const after = useHistoryStore.getState();
    expect(after.page).toBe(0);
    expect(after.total).toBe(0);
  });

  it("clearAll 清空记录、total 与展开态", () => {
    const s = useHistoryStore.getState();
    s.setRecords([rec("a")], 1);
    useHistoryStore.setState({ expandedId: "a" });
    useHistoryStore.getState().clearAll();
    const after = useHistoryStore.getState();
    expect(after.records).toEqual([]);
    expect(after.total).toBe(0);
    expect(after.expandedId).toBeNull();
  });
});
