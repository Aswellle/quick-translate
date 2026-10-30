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
  last_24h: number;
  /** 逐日计数，升序，末位是今天（部分）；覆盖范围由查询参数决定（getStats 的 days / getStatsRange 的区间）。date_ms 为本地时区当日 0 点 */
  daily: Array<{ date_ms: number; count: number; chars: number }>;
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
  /** 应用内「减少透明效果」开关。与系统偏好媒体查询是叠加关系 */
  reduce_transparency: boolean;
  /** 历史导出的默认保存目录；空串 = 每次导出弹原生保存对话框 */
  export_dir: string;
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

/** 翻译源运行时状态（计划第 28/50 节：设置页据此显示七态状态词表） */
export interface ProviderStatus {
  id: string;
  name: string;
  requires_api_key: boolean;
  is_available: boolean;
  /** "healthy" | "degraded" | "open" | "half_open" | "unknown"（snake_case，后端 to_snake_case 下发） */
  health_state: string;
  /** 最近一次错误类别："transient" | "rate_limit" | "auth" | "quota" | "permanent" | "same_language" | null */
  last_error_class: string | null;
}

export async function getProviderStatus(): Promise<ProviderStatus[]> {
  return invoke("get_provider_status");
}

/** 运行时状态快照（计划第 4 节：UI 读它，而不是自己推测） */
export interface RuntimeComponentHealth {
  /** "healthy" | "degraded" | "recovering" | "disabled"（snake_case） */
  state: string;
  last_error_code: string | null;
  last_error_at_ms: number | null;
  last_recovery_at_ms: number | null;
}

/** 本地诊断计数（计划第 33 节）：仅供设置面板展示，不上传 */
export interface RuntimeDiagnostics {
  uptime_ms: number;
  clipboard_restart_count: number;
  translation_success_count: number;
  translation_failure_count: number;
  fallback_count: number;
  /** 回声守卫拦下「译文=原文」的次数 */
  echo_count: number;
}

export async function getRuntimeDiagnostics(): Promise<RuntimeDiagnostics> {
  return invoke("get_runtime_diagnostics");
}

export interface RuntimeStatusSnapshot {
  overall: string; // "healthy" | "degraded" | "recovering"（Rust RuntimeHealth 带 serde rename_all = "snake_case"）
  uptime_ms: number;
  clipboard: RuntimeComponentHealth;
  network: RuntimeComponentHealth;
  popup: RuntimeComponentHealth;
  storage: RuntimeComponentHealth;
}

export async function getRuntimeStatus(): Promise<RuntimeStatusSnapshot> {
  return invoke("get_runtime_status");
}

/** 订阅运行时状态变化（后端有指纹去重，只在状态真正变化时推送） */
export async function onRuntimeStatusChanged(
  handler: (snapshot: RuntimeStatusSnapshot) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<RuntimeStatusSnapshot>("runtime-status-changed", (e) =>
    handler(e.payload)
  );
}

export async function validateProvider(providerId: string): Promise<boolean> {
  return invoke("validate_provider", { providerId });
}

// ---- 配置 Commands ----

export async function getConfig(): Promise<AppConfig> {
  return invoke("get_config");
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

/**
 * 把导出内容写入文件。
 *
 * 后端按 `export_dir` 配置解析路径：已设置则直接写入（同名自动加时间戳，
 * 不覆盖），未设置则弹原生保存对话框。返回写入的完整路径（展示在成功
 * toast 里）；`null` = 用户取消了保存对话框。
 */
export async function exportHistoryToFile(
  fileName: string,
  contents: string
): Promise<string | null> {
  return invoke("export_history_to_file", { fileName, contents });
}

/** 弹原生目录选择器，返回所选目录（null = 用户取消）。设置导出默认位置用 */
export async function chooseExportDir(): Promise<string | null> {
  return invoke("choose_export_dir");
}

/** 使用统计（days：逐日趋势天数，1..=90，缺省 7；汇总卡恒为全量口径） */
export async function getStats(days?: number): Promise<StatsResult> {
  return invoke("get_stats", { days: days ?? null });
}

/** 自定义区间统计（两端均含其所在本地日；区间跨度上限 90 天） */
export async function getStatsRange(
  startMs: number,
  endMs: number
): Promise<StatsResult> {
  return invoke("get_stats_range", { startMs, endMs });
}

// ---- 系统 Commands ----

export async function copyToClipboard(text: string): Promise<void> {
  return invoke("copy_to_clipboard", { text });
}

export async function hidePopup(): Promise<void> {
  return invoke("hide_popup");
}

/**
 * 用户点击浮窗后把它转为可交互。
 *
 * 浮窗默认是非激活窗口（`focusable(false)`）：显示时不抢焦点，代价是
 * 也收不到键盘事件 —— Esc / 空格 / Enter 全部失效。只有用户真的点了它，
 * 后端才会把窗口设为可激活并聚焦，这批快捷键随之恢复。
 */
export async function activatePopup(): Promise<void> {
  return invoke("activate_popup");
}

/** 拖拽结束后上报窗口位置：下次翻译沿用它而不是跟随光标 */
export async function rememberPopupPosition(): Promise<void> {
  return invoke("remember_popup_position_now");
}

/** 清除位置记忆，恢复「跟随光标」的默认定位 */
export async function resetPopupPosition(): Promise<void> {
  return invoke("reset_popup_position");
}

/**
 * 让浮窗的系统级材质跟随应用主题明暗。
 *
 * DWM 的 Acrylic backdrop 跟随系统主题而非应用主题；应用深色 + 系统浅色
 * 时浮窗背后是一块浅色亚克力。前端在 `.dark` class 变化时调用
 * （与 CSS 主题单一来源对齐）。
 *
 * `followSystem` 告诉后端"当前主题偏好是不是 system"。macOS 侧据此决定
 * 是清空窗口 appearance（让原生材质跟随系统、并保持 webview 的
 * prefers-color-scheme 可被系统变化刷新），还是钉死成 Aqua / DarkAqua。
 * 若在 system 档位也钉死 appearance，浮窗自己的 matchMedia 会永远读到
 * 旧值，等于自断"跟随系统"的检测链路。
 */
export async function setPopupTheme(
  dark: boolean,
  followSystem: boolean
): Promise<void> {
  return invoke("set_popup_theme", { dark, followSystem });
}

/**
 * 用户开启「降低透明度」时清空浮窗的 OS 原生材质，关闭时重新施加。
 *
 * 只改 CSS 压不住窗口背后的 Acrylic / Liquid Glass —— 那层由系统合成，
 * 边缘反光与亮度调制照旧。`enhanced = true` 表示需要更实的外观
 * （即 prefers-reduced-transparency: reduce 生效）。
 */
export async function setPopupTransparency(enhanced: boolean): Promise<void> {
  return invoke("set_popup_transparency", { enhanced });
}

export async function resizePopup(width: number, height: number): Promise<void> {
  return invoke("resize_popup", { width, height });
}

/// 浮窗尺寸契约（后端 popup_geometry 为唯一来源，C3）
export interface PopupGeometry {
  width_normal: number;
  width_wide: number;
  height_collapsed: number;
  height_loading: number;
  min_width: number;
  max_width: number;
  min_height: number;
  max_height: number;
}

export async function getPopupGeometry(): Promise<PopupGeometry> {
  return invoke("get_popup_geometry");
}

// ---- 开机自启动 ----

export async function getAutostart(): Promise<boolean> {
  return invoke("get_autostart");
}

// ---- 应用信息 ----

/** 当前应用版本号（tauri.conf.json 的 version） */
export async function getAppVersion(): Promise<string> {
  return invoke("get_app_version");
}

// ---- 系统浏览器打开链接 ----

export async function openUrl(url: string): Promise<void> {
  return invoke("open_url", { url });
}

/** 打开设置窗口（tab: "provider" 直达翻译源配置页） */
export async function openSettingsWindow(tab?: "provider" | "general"): Promise<void> {
  return invoke("open_settings_window", { tab: tab ?? null });
}

// ---- 剪贴板监控开关 ----

export async function setClipboardMonitorEnabled(enabled: boolean): Promise<void> {
  return invoke("set_clipboard_monitor_enabled", { enabled });
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
