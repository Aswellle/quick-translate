// src/lib/configSync.ts
// 配置同步的纯逻辑：设置窗口草稿与后端真实值的合并规则。
//
// 背景：设置窗口 hide 而非销毁，草稿只在挂载时加载一次；托盘切换翻译源/
// 目标语言、开关剪贴板监控等带外写入通过 config-changed 事件下发后端
// 快照。这里定义「哪些字段允许被外部真值改写」与「快速连续写入时谁有
// 权回滚」——纯函数、无 Tauri 依赖，可单测。

import type { AppConfig } from "./commands";
import { PROVIDERS } from "./constants";

/** 凭证字段：后端回读时是掩码值（********efgh），绝不能进草稿 ——
 *  否则「输入框始终从空白开始」的约定被掩码占位符破坏，掩码值还会被
 *  误当成新密钥写回。推导规则与 SettingsWindow 的加载路径一致。 */
export const CREDENTIAL_KEYS: readonly string[] = PROVIDERS
  .filter((p) => p.requiresApiKey)
  .flatMap((p) => p.credentialFields.map((f) => f.key));

export function isCredentialKey(key: string): boolean {
  return (CREDENTIAL_KEYS as readonly string[]).includes(key);
}

/**
 * 把外部真值（config-changed 载荷 / 失败后的 get_config 对账）合并进
 * current，返回合并结果与是否有变化。
 *
 * 三类键不合并：
 * - 凭证键（masked 占位符不能进草稿）；
 * - auto_start（注册表才是权威，载荷里的配置库副本可能落后于系统状态；
 *   需要它时由 getAutostart 单独供给）；
 * - skip(key) 为 true 的键（在途乐观写入 —— 迟到的外部值不能把它打回去）。
 */
export function mergeExternalConfig(
  current: AppConfig,
  incoming: Partial<AppConfig>,
  skip: (key: string) => boolean = () => false
): { merged: AppConfig; changed: boolean } {
  const merged: Record<string, unknown> = {
    ...(current as unknown as Record<string, unknown>),
  };
  let changed = false;
  for (const [key, value] of Object.entries(incoming)) {
    if (value === undefined) continue;
    if (key === "auto_start" || isCredentialKey(key) || skip(key)) continue;
    if (!Object.is(merged[key], value)) {
      merged[key] = value;
      changed = true;
    }
  }
  return { merged: merged as unknown as AppConfig, changed };
}

/**
 * 失败回滚守卫：只有草稿仍持有本次写入的值时，才允许就地回退到 previous。
 *
 * 同键快速连续写入时，前一次请求的**迟到失败**若无条件回滚，会把后一次
 * 乐观写入的新值打回旧值 —— 后端却以后一次为准，界面就此与后端漂移。
 */
export function mayRollBackToPrevious(
  draftValue: unknown,
  writtenValue: unknown
): boolean {
  return Object.is(draftValue, writtenValue);
}
