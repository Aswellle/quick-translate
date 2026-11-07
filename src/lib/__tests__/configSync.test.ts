// src/lib/__tests__/configSync.test.ts
// configSync 纯逻辑：外部真值合并规则 + 快速连续写入的回滚守卫

import { describe, expect, it } from "vitest";
import {
  CREDENTIAL_KEYS,
  isCredentialKey,
  mayRollBackToPrevious,
  mergeExternalConfig,
} from "../configSync";
import type { AppConfig } from "../commands";

const baseConfig = (): AppConfig => ({
  target_lang: "zh",
  provider: "google",
  deepl_api_key: "",
  tencent_secret_id: "",
  tencent_secret_key: "",
  baidu_app_id: "",
  baidu_secret_key: "",
  youdao_app_key: "",
  youdao_app_secret: "",
  auto_start: false,
  history_limit: 200,
  theme: "system",
  fallback_enabled: true,
  onboarding_completed: true,
  clipboard_monitor_enabled: true,
  sensitive_filter_enabled: true,
  reduce_transparency: false,
  export_dir: "",
});

describe("mergeExternalConfig", () => {
  it("把变更的非凭证字段合并进草稿", () => {
    const { merged, changed } = mergeExternalConfig(baseConfig(), {
      provider: "deepl",
      target_lang: "ja",
    });
    expect(changed).toBe(true);
    expect(merged.provider).toBe("deepl");
    expect(merged.target_lang).toBe("ja");
    // 未提及的字段原样保留
    expect(merged.theme).toBe("system");
  });

  it("载荷与现值一致时报告无变化", () => {
    const current = baseConfig();
    const { merged, changed } = mergeExternalConfig(current, { ...current });
    expect(changed).toBe(false);
    expect(merged).toEqual(current);
  });

  it("凭证字段永远不合并 —— masked 占位符不能进草稿", () => {
    const current = baseConfig();
    const incoming = { ...current, deepl_api_key: "********abcd" };
    const { merged } = mergeExternalConfig(current, incoming);
    expect(merged.deepl_api_key).toBe("");
    for (const key of CREDENTIAL_KEYS) {
      expect((merged as unknown as Record<string, unknown>)[key]).toBe("");
    }
  });

  it("auto_start 不合并 —— 注册表才是权威，载荷里的副本可能落后", () => {
    const current = baseConfig();
    const { merged } = mergeExternalConfig(current, { ...current, auto_start: true });
    expect(merged.auto_start).toBe(false);
  });

  it("skip 命中的键不合并 —— 在途乐观写入不被迟到的外部值打回", () => {
    const current = baseConfig();
    const { merged } = mergeExternalConfig(
      current,
      { ...current, provider: "deepl", theme: "dark" },
      (key) => key === "provider" // provider 正在途
    );
    expect(merged.provider).toBe("google");
    expect(merged.theme).toBe("dark");
  });

  it("载荷里 undefined 的键被忽略", () => {
    const current = baseConfig();
    const { merged, changed } = mergeExternalConfig(current, {
      provider: undefined,
    } as Partial<AppConfig>);
    expect(changed).toBe(false);
    expect(merged.provider).toBe("google");
  });
});

describe("CREDENTIAL_KEYS", () => {
  it("覆盖后端 CREDENTIAL_KEYS 的全部七个键", () => {
    expect([...CREDENTIAL_KEYS].sort()).toEqual(
      [
        "deepl_api_key",
        "tencent_secret_id",
        "tencent_secret_key",
        "baidu_app_id",
        "baidu_secret_key",
        "youdao_app_key",
        "youdao_app_secret",
      ].sort()
    );
  });

  it("isCredentialKey 对大小写与拼写敏感", () => {
    expect(isCredentialKey("deepl_api_key")).toBe(true);
    expect(isCredentialKey("deeplApiKey")).toBe(false);
    expect(isCredentialKey("provider")).toBe(false);
  });
});

describe("mayRollBackToPrevious", () => {
  it("草稿仍持有本次写入的值时允许回滚", () => {
    expect(mayRollBackToPrevious("dark", "dark")).toBe(true);
    expect(mayRollBackToPrevious(true, true)).toBe(true);
  });

  it("草稿已被更新的写入接管时禁止回滚", () => {
    // 快速连点开关：第一次写入的迟到失败不能覆盖第二次的乐观值
    expect(mayRollBackToPrevious(false, true)).toBe(false);
    expect(mayRollBackToPrevious("ja", "en")).toBe(false);
  });

  it("草稿为空（undefined）时不回滚", () => {
    expect(mayRollBackToPrevious(undefined, "dark")).toBe(false);
  });
});
