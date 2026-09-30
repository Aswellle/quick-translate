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
  /** 当前请求的 request_id（loading 事件下发）；迟到的旧结果据此被丢弃 */
  requestId: string | null;

  // Actions
  setLoading: (requestId: string | null) => void;
  setResult: (result: TranslationResult) => void;
  setError: (code: string, message: string) => void;
  reset: () => void;
}

export const useTranslationStore = create<TranslationState>((set) => ({
  status: "idle",
  result: null,
  errorCode: null,
  errorMessage: null,
  requestId: null,

  setLoading: (requestId) =>
    set({
      status: "loading",
      result: null,
      errorCode: null,
      errorMessage: null,
      requestId,
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
