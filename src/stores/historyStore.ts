// src/stores/historyStore.ts
// 翻译历史状态管理（Zustand）

import { create } from "zustand";
import type { TranslationRecord } from "@/lib/commands";

interface HistoryState {
  records: TranslationRecord[];
  total: number;
  isLoading: boolean;
  searchQuery: string;
  page: number;
  pageSize: number;
  expandedId: string | null;
  starredOnly: boolean;

  // Actions
  setRecords: (records: TranslationRecord[], total: number) => void;
  setLoading: (loading: boolean) => void;
  setSearchQuery: (query: string) => void;
  setPage: (page: number) => void;
  setExpanded: (id: string | null | ((prev: string | null) => string | null)) => void;
  setStarredOnly: (value: boolean) => void;
  clearAll: () => void;
  removeRecord: (id: string) => void;
  toggleStar: (id: string, newValue: boolean) => void;
}

export const useHistoryStore = create<HistoryState>((set, get) => ({
  records: [],
  total: 0,
  isLoading: false,
  searchQuery: "",
  page: 0,
  pageSize: 50,
  expandedId: null,
  starredOnly: false,

  setRecords: (records, total) => {
    // 返回页为空且不是第一页时回退一页：总数在会话中变少（删除、FIFO
    // 清理、另一窗口清空）后，当前页可能已越界 —— 表现为空列表配错误
    // 的空态文案，且翻页条因 total <= pageSize 消失，无法点「上一页」
    const { page, pageSize } = get();
    if (records.length === 0 && page > 0 && total > 0) {
      set({ records, total, page: Math.max(0, page - 1) });
      return;
    }
    set({ records, total });
  },
  setLoading: (isLoading) => set({ isLoading }),
  setSearchQuery: (searchQuery) => set({ searchQuery, page: 0 }),
  setPage: (page) => set({ page }),
  setExpanded: (expandedId) =>
    set((state) => ({
      expandedId:
        typeof expandedId === "function" ? expandedId(state.expandedId) : expandedId,
    })),
  setStarredOnly: (starredOnly) => set({ starredOnly, page: 0 }),
  clearAll: () => set({ records: [], total: 0, expandedId: null }),
  removeRecord: (id) =>
    set((state) => {
      const records = state.records.filter((r) => r.id !== id);
      const total = Math.max(0, state.total - 1);
      // 删掉的是本页最后一条且不在第一页：回退一页重新加载。
      // 不回退的话翻页条会消失（total 降到 pageSize 以下），用户困在
      // 显示着「暂无翻译历史」的空页上 —— 实际上一页还有整页数据。
      if (records.length === 0 && state.page > 0 && total > 0) {
        return { records, total, page: state.page - 1 };
      }
      return { records, total };
    }),
  toggleStar: (id, newValue) =>
    set((state) => ({
      records: state.records.map((r) =>
        r.id === id ? { ...r, is_starred: newValue } : r
      ),
    })),
}));
