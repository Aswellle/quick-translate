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

  // Actions
  setRecords: (records: TranslationRecord[], total: number) => void;
  setLoading: (loading: boolean) => void;
  setSearchQuery: (query: string) => void;
  setPage: (page: number) => void;
  setExpanded: (id: string | null) => void;
  clearAll: () => void;
}

export const useHistoryStore = create<HistoryState>((set) => ({
  records: [],
  total: 0,
  isLoading: false,
  searchQuery: "",
  page: 0,
  pageSize: 50,
  expandedId: null,

  setRecords: (records, total) => set({ records, total }),
  setLoading: (isLoading) => set({ isLoading }),
  setSearchQuery: (searchQuery) => set({ searchQuery, page: 0 }),
  setPage: (page) => set({ page }),
  setExpanded: (expandedId) => set({ expandedId }),
  clearAll: () => set({ records: [], total: 0, expandedId: null }),
}));
