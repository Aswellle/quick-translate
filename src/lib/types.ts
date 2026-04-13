// src/lib/types.ts
// 前端专用类型：Tauri event payload 结构

import type { TranslationResult } from "./commands";

export interface TranslationLoadingPayload {
  position: {
    x: number;
    y: number;
    monitor_width: number;
    monitor_height: number;
  };
}

export interface TranslationResultPayload {
  result: TranslationResult;
}

export interface TranslationErrorPayload {
  code: string;
  message: string;
}
