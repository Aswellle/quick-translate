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
  setExpanded: (id: string | null) => void;
  setStarredOnly: (value: boolean) => void;
  clearAll: () => void;
  removeRecord: (id: string) => void;
  toggleStar: (id: string, newValue: boolean) => void;
}

export const useHistoryStore = create<HistoryState>((set) => ({
  records: [],
  total: 0,
  isLoading: false,
  searchQuery: "",
  page: 0,
  pageSize: 50,
  expandedId: null,
  starredOnly: false,

  setRecords: (records, total) => set({ records, total }),
  setLoading: (isLoading) => set({ isLoading }),
  setSearchQuery: (searchQuery) => set({ searchQuery, page: 0 }),
  setPage: (page) => set({ page }),
  setExpanded: (expandedId) => set({ expandedId }),
  setStarredOnly: (starredOnly) => set({ starredOnly, page: 0 }),
  clearAll: () => set({ records: [], total: 0, expandedId: null }),
  removeRecord: (id) =>
    set((state) => ({
      records: state.records.filter((r) => r.id !== id),
      total: Math.max(0, state.total - 1),
    })),
  toggleStar: (id, newValue) =>
    set((state) => ({
      records: state.records.map((r) =>
        r.id === id ? { ...r, is_starred: newValue } : r
      ),
    })),
}));
