// src/lib/commands.ts
// 类型安全的 Tauri invoke 封装层

import { invoke } from "@tauri-apps/api/core";

// ---- 共享类型 ----

export interface TranslationResult {
  source_text: string;
  translated_text: string;
  detected_source_lang: string;
  target_lang: string;
  provider: string;
  duration_ms: number;
  truncated: boolean;
}

export interface ProviderInfo {
  id: string;
  name: string;
  requires_api_key: boolean;
  is_available: boolean;
}

export interface TranslationRecord {
  id: string;
  source_text: string;
  translated_text: string;
  source_lang: string;
  target_lang: string;
  provider: string;
  created_at: number;
  duration_ms: number | null;
  is_starred: boolean;
}

export interface StatsResult {
  total_records: number;
  total_chars: number;
  by_provider: Record<string, number>;
  last_7_days: number;
  last_30_days: number;
}

export interface AppConfig {
  target_lang: string;
  provider: string;
  // 翻译源凭证（前端收到已脱敏）
  deepl_api_key: string;
  tencent_secret_id: string;
  tencent_secret_key: string;
  baidu_app_id: string;
  baidu_secret_key: string;
  youdao_app_key: string;
  youdao_app_secret: string;
  auto_start: boolean;
  history_limit: number;
  theme: string;
  fallback_enabled: boolean;
  onboarding_completed: boolean;
  clipboard_monitor_enabled: boolean;
}

export interface HistoryQuery {
  search?: string;
  limit: number;
  offset: number;
  starred_only?: boolean;
}

export interface AppError {
  code: string;
  message: string;
}

// ---- 翻译 Commands ----

export async function translateText(
  text: string,
  targetLang?: string
): Promise<TranslationResult> {
  return invoke("translate_text", { text, targetLang });
}

export async function listProviders(): Promise<ProviderInfo[]> {
  return invoke("list_providers");
}

export async function validateProvider(providerId: string): Promise<boolean> {
  return invoke("validate_provider", { providerId });
}

// ---- 配置 Commands ----

export async function getConfig(): Promise<AppConfig> {
  return invoke("get_config");
}

export async function setConfig(key: string, value: string): Promise<void> {
  return invoke("set_config", { key, value });
}

export async function setConfigBatch(
  updates: [string, string][]
): Promise<void> {
  return invoke("set_config_batch", { updates });
}

// ---- 历史 Commands ----

export async function queryHistory(
  params: HistoryQuery
): Promise<TranslationRecord[]> {
  return invoke("query_history", { params });
}

export async function countHistory(
  search?: string,
  starredOnly?: boolean
): Promise<number> {
  return invoke("count_history", { search, starredOnly });
}

export async function clearHistory(): Promise<void> {
  return invoke("clear_history");
}

export async function deleteHistoryRecord(id: string): Promise<void> {
  return invoke("delete_history_record", { id });
}

export async function toggleStarRecord(id: string): Promise<boolean> {
  return invoke("toggle_star_record", { id });
}

export async function exportHistory(): Promise<string> {
  return invoke("export_history");
}

export async function getStats(): Promise<StatsResult> {
  return invoke("get_stats");
}

// ---- 系统 Commands ----

export async function copyToClipboard(text: string): Promise<void> {
  return invoke("copy_to_clipboard", { text });
}

export async function hidePopup(): Promise<void> {
  return invoke("hide_popup");
}

export async function resizePopup(width: number, height: number): Promise<void> {
  return invoke("resize_popup", { width, height });
}

export async function getAppVersion(): Promise<string> {
  return invoke("get_app_version");
}

// ---- Toast ----

export interface ToastPayload {
  message: string;
  kind: "error" | "success" | "warning" | "info";
  duration?: number;
}

export async function notifyToast(payload: ToastPayload): Promise<void> {
  return invoke("notify_toast", { payload });
}

// ---- 开机自启动 ----

export async function getAutostart(): Promise<boolean> {
  return invoke("get_autostart");
}

export async function setAutostart(enabled: boolean): Promise<void> {
  return invoke("set_autostart", { enabled });
}

// ---- 系统浏览器打开链接 ----

export async function openUrl(url: string): Promise<void> {
  return invoke("open_url", { url });
}

// ---- 剪贴板监控开关 ----

export async function setClipboardMonitorEnabled(enabled: boolean): Promise<void> {
  return invoke("set_clipboard_monitor_enabled", { enabled });
}

// ---- 更新检查 ----

export async function checkUpdate(): Promise<void> {
  return invoke("check_update");
}

// ---- 引导向导 ----

export async function checkOnboarding(): Promise<boolean> {
  return invoke("check_onboarding");
}

export async function completeOnboarding(): Promise<void> {
  return invoke("complete_onboarding");
}

export async function openOnboardingWindow(): Promise<void> {
  return invoke("open_onboarding_window");
}
