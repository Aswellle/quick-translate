// src/windows/history/__tests__/HistoryChanged.test.tsx
// 历史窗口的实时刷新契约：落盘 worker 每写成功一条历史就广播
// history-changed，本窗口 hide 而非销毁、组件树永不重挂载，监听器
// 必须据此重新拉取当前筛选条件下的列表——没有这条链路，窗口会一直
// 展示打开时的旧快照（「翻译完几次再打开历史缺记录」的根因）。

// @vitest-environment jsdom
import { describe, expect, it, vi, beforeEach } from "vitest";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";

// 命令层 mock：queryHistory 的调用次数就是「列表是否重新拉取」的观测点
vi.mock("@/lib/commands", () => ({
  queryHistory: vi.fn(async () => []),
  countHistory: vi.fn(async () => 0),
  clearHistory: vi.fn(async () => {}),
  exportHistory: vi.fn(async () => "[]"),
  exportHistoryToFile: vi.fn(async () => null),
  deleteHistoryRecord: vi.fn(async () => {}),
  toggleStarRecord: vi.fn(async () => true),
}));

// 事件层 mock：捕获各事件的监听器，测试直接驱动它们
const listeners = new Map<string, (e: unknown) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: unknown) => void) => {
    listeners.set(event, cb);
    return () => {};
  }),
}));

import { HistoryWindow } from "../HistoryWindow";
import { queryHistory } from "@/lib/commands";
import { useHistoryStore } from "@/stores/historyStore";

describe("history-changed 实时刷新", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    listeners.clear();
    vi.mocked(queryHistory).mockClear();
    // 隔离 store：其他测试/上一次渲染可能改过筛选条件
    useHistoryStore.setState({
      records: [],
      total: 0,
      isLoading: false,
      searchQuery: "",
      page: 0,
      starredOnly: false,
      expandedId: null,
    });
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(<HistoryWindow />);
    });
  });

  it("挂载时拉取一次，history-changed 到达后按当前筛选条件重新拉取", async () => {
    const callsAfterMount = vi.mocked(queryHistory).mock.calls.length;
    expect(callsAfterMount).toBeGreaterThan(0);

    // 模拟用户切了筛选：新翻译到达时应按「当前」条件重新拉取
    act(() => {
      useHistoryStore.getState().setSearchQuery("newest");
    });
    vi.mocked(queryHistory).mockClear();

    const handler = listeners.get("history-changed");
    expect(handler).toBeDefined();

    await act(async () => {
      handler!({ payload: null });
    });

    expect(queryHistory).toHaveBeenCalledTimes(1);
    expect(vi.mocked(queryHistory).mock.calls[0][0]).toMatchObject({
      search: "newest",
    });
  });
});
