// src/lib/__tests__/providerState.test.ts
// 翻译源状态词表（计划 Task 08 实施 1：「配置状态和健康状态，二者不能
// 混淆」）。词表是设置页用户看到的全部健康信息，映射关系必须有契约锁定。

import { describe, expect, it } from "vitest";
import { providerStateLabel, STATE_TONE_CLASS } from "../providerState";
import type { ProviderStatus } from "../commands";

const status = (over: Partial<ProviderStatus> = {}): ProviderStatus => ({
  id: "deepl",
  name: "DeepL",
  requires_api_key: true,
  is_available: true,
  health_state: "healthy",
  last_error_class: null,
  ...over,
});

describe("providerStateLabel", () => {
  it("未配置优先于健康状态 —— 没有凭证时谈健康没有意义", () => {
    // 即使熔断机报 open，未配置就是未配置
    const open = status({ health_state: "open", last_error_class: "auth" });
    expect(providerStateLabel(open, false)).toEqual({
      text: "未配置",
      tone: "muted",
    });
  });

  it("healthy 显示正常", () => {
    expect(providerStateLabel(status(), true)).toEqual({ text: "正常", tone: "ok" });
  });

  it("half_open 显示正在恢复", () => {
    expect(providerStateLabel(status({ health_state: "half_open" }), true)).toEqual({
      text: "正在恢复…",
      tone: "warn",
    });
  });

  it("open/degraded 按最近错误类别细分原因", () => {
    for (const health_state of ["open", "degraded"] as const) {
      expect(
        providerStateLabel(
          status({ health_state, last_error_class: "auth" }),
          true
        )
      ).toEqual({ text: "认证失败，请检查凭证", tone: "err" });
      expect(
        providerStateLabel(
          status({ health_state, last_error_class: "quota" }),
          true
        )
      ).toEqual({ text: "额度已用尽", tone: "err" });
      expect(
        providerStateLabel(
          status({ health_state, last_error_class: "rate_limit" }),
          true
        )
      ).toEqual({ text: "请求过于频繁，稍后自动恢复", tone: "warn" });
      // 网络类（transient/connect/timeout）统一归「暂时不可用」
      expect(
        providerStateLabel(
          status({ health_state, last_error_class: "transient" }),
          true
        )
      ).toEqual({ text: "暂时不可用，将自动恢复", tone: "warn" });
      expect(
        providerStateLabel(status({ health_state, last_error_class: null }), true)
      ).toEqual({ text: "暂时不可用，将自动恢复", tone: "warn" });
    }
  });

  it("无状态记录的已配置源不显示状态词（交由调用方兜底）", () => {
    expect(providerStateLabel(undefined, true)).toBeNull();
    // same_language 是成功而非失败，不该映射到任何错误词
    expect(
      providerStateLabel(status({ last_error_class: "same_language", health_state: "open" }), true)
    ).toEqual({ text: "暂时不可用，将自动恢复", tone: "warn" });
  });
});

describe("STATE_TONE_CLASS", () => {
  it("四种色调都有映射", () => {
    for (const tone of ["ok", "warn", "err", "muted"] as const) {
      expect(STATE_TONE_CLASS[tone].length).toBeGreaterThan(0);
    }
  });
});
