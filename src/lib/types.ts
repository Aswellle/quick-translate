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
  /** 本次翻译请求的完整 UUID，用于 latest-wins 二次校验（计划第 6 节） */
  request_id: string;
}

export interface TranslationResultPayload {
  result: TranslationResult;
  /** 本次翻译请求的完整 UUID，用于 latest-wins 二次校验（计划第 6 节） */
  request_id: string;
}

export interface TranslationErrorPayload {
  code: string;
  message: string;
  /** 见 TranslationResultPayload.request_id */
  request_id: string;
}
