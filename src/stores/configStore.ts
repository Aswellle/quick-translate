// src/stores/configStore.ts
// 应用配置状态管理（Zustand）

import { create } from "zustand";
import type { AppConfig } from "@/lib/commands";

interface ConfigState {
  config: AppConfig | null;
  isLoaded: boolean;

  setConfig: (config: AppConfig) => void;
}

export const useConfigStore = create<ConfigState>((set) => ({
  config: null,
  isLoaded: false,

  setConfig: (config) => set({ config, isLoaded: true }),
}));
