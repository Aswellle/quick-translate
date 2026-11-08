// src/lib/__tests__/events.test.ts
// 事件名契约：Rust `app.emit(...)` 的每个事件名都必须有前端监听方，
// 前端监听的每个名字都必须由 Rust 真实发出。名字漂移 = 监听静默失联
//（不报错、不崩，只是永远收不到），比 404 更难发现，所以用测试锁住。

import { describe, expect, it } from "vitest";
import { EVENTS } from "../constants";

/** 与 Rust 侧 `app.emit(...)` 的字面事件名一一对应（同步维护；
 *  新增事件时两侧同时补）。 emit 点位注释给出 Rust 侧来源，便于核对。 */
const RUST_EMITS = [
  "translation-loading", // system/translation/translation_flow.rs
  "translation-result", // 同上
  "translation-error", // 同上
  "history-changed", // system/persistence.rs
  "runtime-status-changed", // runtime/status.rs
  "toast", // commands/system.rs emit_toast
  "config-changed", // commands/config.rs
  "theme-changed", // commands/config.rs emit_theme_changed
  "reduce-transparency-changed", // commands/config.rs
  "settings-navigate", // tray/system 打开设置到指定页签
] as const;

/** 前端实际监听的完整清单（含未收进 EVENTS 常量的散落字面量）。
 *  hook/组件出处见行尾注释，便于核对。 */
const FRONTEND_LISTENS = [
  EVENTS.TRANSLATION_LOADING,
  EVENTS.TRANSLATION_RESULT,
  EVENTS.TRANSLATION_ERROR,
  EVENTS.HISTORY_CHANGED,
  "runtime-status-changed", // lib/commands.ts onRuntimeStatus
  "toast", // components/ToastManager.tsx
  "config-changed", // windows/settings/SettingsWindow.tsx
  "theme-changed", // hooks/useTheme.ts
  "reduce-transparency-changed", // hooks/useTheme.ts
  "settings-navigate", // windows/settings/SettingsWindow.tsx
] as const;

describe("事件名契约（Rust emit ↔ 前端 listen）", () => {
  it("Rust 发出的每个事件前端都在监听", () => {
    const orphan = RUST_EMITS.filter((e) => !FRONTEND_LISTENS.includes(e));
    expect(orphan).toEqual([]);
  });

  it("前端监听的每个事件 Rust 都会发出", () => {
    const ghost = FRONTEND_LISTENS.filter((e) => !RUST_EMITS.includes(e));
    expect(ghost).toEqual([]);
  });

  it("EVENTS 常量必须是合法 kebab-case，避免两侧手滑改成别的风格", () => {
    for (const name of Object.values(EVENTS)) {
      expect(name).toMatch(/^[a-z][a-z0-9]*(-[a-z0-9]+)*$/);
    }
  });
});
