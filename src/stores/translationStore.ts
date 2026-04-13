// src/stores/translationStore.ts
// 翻译状态管理（Zustand）

import { create } from "zustand";
import type { TranslationResult } from "@/lib/commands";

export type TranslationStatus =
  | "idle"
  | "loading"
  | "success"
  | "error";

interface TranslationState {
  status: TranslationStatus;
  result: TranslationResult | null;
  errorCode: string | null;
  errorMessage: string | null;

  // Actions
  setLoading: () => void;
  setResult: (result: TranslationResult) => void;
  setError: (code: string, message: string) => void;
  reset: () => void;
}

export const useTranslationStore = create<TranslationState>((set) => ({
  status: "idle",
  result: null,
  errorCode: null,
  errorMessage: null,

  setLoading: () =>
    set({
      status: "loading",
      result: null,
      errorCode: null,
      errorMessage: null,
    }),

  setResult: (result) =>
    set({
      status: "success",
      result,
      errorCode: null,
      errorMessage: null,
    }),

  setError: (code, message) =>
    set({
      status: "error",
      result: null,
      errorCode: code,
      errorMessage: message,
    }),

  reset: () =>
    set({
      status: "idle",
      result: null,
      errorCode: null,
      errorMessage: null,
    }),
}));
