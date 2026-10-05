// src/lib/__tests__/constants.test.ts
// 错误码文案覆盖契约：Rust 侧 AppError 的每个变体 code 都必须有中文
// 文案，否则 Rust 的 to_string() 内部字符串会原样漏进浮窗/设置面板。

import { describe, expect, it } from "vitest";
import { ERROR_MESSAGES } from "../constants";

/** 与 src-tauri/src/error.rs 的 AppError::code() 一一对应（同步维护） */
const RUST_APP_ERROR_CODES = [
  "EMPTY_TEXT",
  "NON_TEXT_CONTENT",
  "SAME_LANGUAGE",
  "NETWORK_ERROR",
  "AUTH_ERROR",
  "RATE_LIMIT",
  "QUOTA_EXHAUSTED",
  "TIMEOUT",
  "PROVIDER_REJECTED",
  "ALL_PROVIDERS_FAILED",
  "CLIPBOARD_ERROR",
  "HOTKEY_CONFLICT",
  "DATABASE_ERROR",
  "CONFIG_ERROR",
  "STORAGE_UNAVAILABLE",
  "DB_MIGRATION_FAILED",
  "WINDOW_ERROR",
  "CRYPTO_ERROR",
  "SERDE_ERROR",
  "CONNECT_ERROR",
  "PROTOCOL_ERROR",
] as const;

describe("ERROR_MESSAGES 覆盖契约", () => {
  it("Rust 侧每个错误码都有中文文案", () => {
    const missing = RUST_APP_ERROR_CODES.filter((code) => !(code in ERROR_MESSAGES));
    expect(missing).toEqual([]);
  });

  it("文案非空且不包含 Rust 调试串痕迹", () => {
    for (const [code, message] of Object.entries(ERROR_MESSAGES)) {
      expect(message.trim().length, code).toBeGreaterThan(0);
      expect(message, code).not.toMatch(/AppError|Error \{|0x[0-9a-f]+/i);
    }
  });
});
