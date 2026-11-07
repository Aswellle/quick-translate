// src/lib/providerState.ts
// 翻译源运行时状态的展示词表（计划第 28/51 节）。
//
// 用户不需要看到开发者的 Open/HalfOpen 术语 ——「暂时不可用」「认证失败」
// 这类话配上「将自动恢复」就够了。纯函数、无 React 依赖，可单测。

import type { ProviderStatus } from "./commands";

export type ProviderStateTone = "ok" | "warn" | "err" | "muted";

/**
 * 单个翻译源的状态词。配置状态（有没有凭证）优先于健康状态（熔断机），
 * 二者不能混淆：没配凭证时谈「健康」没有意义。
 *
 * 返回 null 表示该源没有独立状态可显示（Google 兜底源永远健康，由
 * 调用方直接展示「始终可用」，不走这里）。
 */
export function providerStateLabel(
  status: ProviderStatus | undefined,
  hasCredentials: boolean
): { text: string; tone: ProviderStateTone } | null {
  if (!status || !hasCredentials) {
    return hasCredentials ? null : { text: "未配置", tone: "muted" };
  }
  switch (status.health_state) {
    case "healthy":
      return { text: "正常", tone: "ok" };
    case "half_open":
      return { text: "正在恢复…", tone: "warn" };
    case "open":
    case "degraded": {
      // 用最近一次错误的类别细分「暂时不可用」的原因
      switch (status.last_error_class) {
        case "auth":
          return { text: "认证失败，请检查凭证", tone: "err" };
        case "quota":
          return { text: "额度已用尽", tone: "err" };
        case "rate_limit":
          return { text: "请求过于频繁，稍后自动恢复", tone: "warn" };
        default:
          return { text: "暂时不可用，将自动恢复", tone: "warn" };
      }
    }
    default:
      return null;
  }
}

export const STATE_TONE_CLASS: Record<ProviderStateTone, string> = {
  ok: "text-green-500",
  warn: "text-amber-500",
  err: "text-red-500",
  muted: "text-[var(--text-secondary)]",
};
