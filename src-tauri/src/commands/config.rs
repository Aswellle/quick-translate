// src-tauri/src/commands/config.rs
// 配置读写 command handlers

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::state::AppState;
use crate::system::tray;
use crate::types::AppConfig;

/// 获取完整配置（API Key 已脱敏）
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, AppError> {
    Ok(state.config.read().await.get_all())
}

/// 更新单个配置项，并触发必要的副作用
///
/// 统一委托批量管线（校验 → 持久化 → 副作用）：此前单项与批量是两套
/// 顺序相反的实现，每新增配置键的作者都要猜该模仿哪套。单项更新就是
/// 长度为 1 的批量。
#[tauri::command]
pub async fn set_config(
    state: State<'_, AppState>,
    app: AppHandle,
    key: String,
    value: String,
) -> Result<(), AppError> {
    set_config_batch_inner(&state, &app, vec![(key, value)]).await
}

/// 批量更新配置（设置面板保存时调用）
#[tauri::command]
pub async fn set_config_batch(
    state: State<'_, AppState>,
    app: AppHandle,
    updates: Vec<(String, String)>,
) -> Result<(), AppError> {
    set_config_batch_inner(&state, &app, updates).await
}

/// 配置写入的单一管线：整批校验 → 持久化 → 幂等副作用。
///
/// 单项与批量共用同一条路径 —— 顺序不一致曾是本仓库最年长的结构性
/// 隐患（副作用先行会在持久化失败后留下引擎/数据库各执一词的状态）。
///
/// 批次语义（QT-010）：**先整批校验，任一项非法则拒绝整批** —— 此前的
/// 「非法项 warn 后跳过、合法项照写」会让前端拿到 Ok 却只保存了一半，
/// 配置 DB、provider 实例、OS 自启动彼此漂移。副作用失败也不再静默：
/// 持久化已发生、无法安全回滚，补偿策略是如实返回错误（副作用全部
/// 幂等，用户重试保存即可收敛），绝不能伪装整体成功。
async fn set_config_batch_inner(
    state: &State<'_, AppState>,
    app: &AppHandle,
    updates: Vec<(String, String)>,
) -> Result<(), AppError> {
    // 静态校验（键清单 + 取值）+ 掩码凭证过滤：任一项非法 → 拒绝整批
    let validated = validate_batch_static(&updates)?;

    // 依赖引擎状态的校验：provider 必须真实存在
    for (key, value) in &validated {
        if key == "provider" {
            let exists = state
                .translator
                .list_providers()
                .await
                .iter()
                .any(|p| p.id == *value);
            if !exists {
                return Err(AppError::ConfigError(format!("未知翻译源: {}", value)));
            }
        }
    }

    // 批量写入 DB（auto_start 单独处理避免双写）
    let db_updates: Vec<(String, String)> = validated
        .iter()
        .filter(|(k, _)| k != "auto_start")
        .cloned()
        .collect();

    if !db_updates.is_empty() {
        state.config.write().await.set_batch(db_updates).await?;
    }

    // 逐键应用副作用：全部幂等，失败收集后如实上报（补偿 = 用户重试保存）
    let mut failed_keys: Vec<String> = Vec::new();
    for (key, value) in &validated {
        if let Err(e) = handle_side_effect(app, state, key, value).await {
            tracing::warn!("[set_config] side_effect[{}] 失败: {}", key, e);
            failed_keys.push(key.clone());
        }
    }
    if !failed_keys.is_empty() {
        return Err(AppError::ConfigError(format!(
            "设置已保存，但以下项生效失败（请重试保存）: {}",
            failed_keys.join(", ")
        )));
    }

    // 持久化与副作用全部落地后广播一次（载荷读取与 get_config 相同的
    // 掩码视图，凭证不会明文出现在事件里）
    emit_config_changed(app, &state.config.read().await.get_all());

    Ok(())
}

// ── 配置键静态契约（QT-010）─────────────────────────────────────────────

/// 布尔型配置键
const BOOL_KEYS: &[&str] = &[
    "auto_start",
    "fallback_enabled",
    "onboarding_completed",
    "clipboard_monitor_enabled",
    "sensitive_filter_enabled",
    "reduce_transparency",
    "cache_enabled",
    "glossary_enabled",
    "baidu_use_term_base",
];

/// 界面主题的合法取值
const THEME_VALUES: &[&str] = &["system", "light", "dark"];

/// 翻译风格取值（计划 Task 10）。natural = 不下发风格参数；
/// formal/informal 映射 DeepL formality 的 more/less，且仅对支持的目标
/// 语言生效 —— 不支持的翻译源在请求侧忽略，UI 如实标注。
const TRANSLATION_STYLE_VALUES: &[&str] = &["natural", "formal", "informal"];

/// 支持的目标语言（与前端 SUPPORTED_LANGUAGES 一致）
const TARGET_LANGS: &[&str] = crate::types::SUPPORTED_LANGS;

/// 凭证键（值只需非空；掩码值已在更早的入口过滤）
const CREDENTIAL_KEYS: &[&str] = &[
    "deepl_api_key",
    "tencent_secret_id",
    "tencent_secret_key",
    "baidu_app_id",
    "baidu_secret_key",
    "youdao_app_key",
    "youdao_app_secret",
];

/// 单键取值校验（不含 provider 的存在性校验 —— 那需要引擎状态）。
///
/// 未知键一律拒绝：配置键清单就是写入面的边界，放行未知键等于绕过
/// 全部类型与范围约束直接写库。
fn validate_value(key: &str, value: &str) -> Result<(), AppError> {
    let known = matches!(
        key,
        "provider"
            | "target_lang"
            | "theme"
            | "translation_style"
            | "history_limit"
            | "export_dir"
            | "hotkey_translate_clipboard"
            | "hotkey_open_manual"
            | "hotkey_toggle_monitor"
            | "hotkey_toggle_popup"
            | "hotkey_retranslate_last"
            | "youdao_vocab_id"
            | "tencent_term_repo_ids"
    ) || BOOL_KEYS.contains(&key)
        || CREDENTIAL_KEYS.contains(&key);
    if !known {
        return Err(AppError::ConfigError(format!("未知配置键: {}", key)));
    }

    let invalid = || AppError::ConfigError(format!("配置项 {} 的值非法", key));

    match key {
        k if BOOL_KEYS.contains(&k) => {
            if value != "true" && value != "false" {
                return Err(invalid());
            }
        }
        "theme" => {
            if !THEME_VALUES.contains(&value) {
                return Err(invalid());
            }
        }
        "translation_style" => {
            if !TRANSLATION_STYLE_VALUES.contains(&value) {
                return Err(invalid());
            }
        }
        "target_lang" => {
            if !TARGET_LANGS.contains(&value) {
                return Err(invalid());
            }
        }
        "history_limit" => {
            let parsed: i64 = value.parse().map_err(|_| {
                AppError::ConfigError("配置项 history_limit 必须是整数".to_string())
            })?;
            if !(crate::domain::config::HISTORY_LIMIT_MIN
                ..=crate::domain::config::HISTORY_LIMIT_MAX)
                .contains(&parsed)
            {
                return Err(invalid());
            }
        }
        // 导出目录允许清空（空 = 每次导出弹对话框询问）；设值时必须是
        // 绝对路径形态 —— 相对路径会随进程工作目录漂移，写出的文件位置
        // 用户找不到
        "export_dir" => {
            if !value.is_empty() && !std::path::Path::new(value).is_absolute() {
                return Err(invalid());
            }
        }
        // provider 的存在性由管线内依赖引擎的校验负责；这里保证非空
        "provider" if value.trim().is_empty() => return Err(invalid()),
        // 加速键语法由注册时裁决（错误可定位到具体冲突），这里只挡
        // 明显异常的长度，避免把任意大字符串写进配置
        "hotkey_translate_clipboard"
        | "hotkey_open_manual"
        | "hotkey_toggle_monitor"
        | "hotkey_toggle_popup"
        | "hotkey_retranslate_last" => {
            if value.chars().count() > 100 {
                return Err(invalid());
            }
        }
        // 术语表 ID 由有道控制台生成；允许清空（= 不使用）。
        // 逗号/分号/U+0001 是缓存作用域令牌的分隔符（cache::scope_token），
        // ID 内出现会与相邻维度撞键 —— 一个 ID 让两个上下文共享缓存
        "youdao_vocab_id" => {
            if value.chars().count() > 200 || value.contains([',', ';', '\u{1}']) {
                return Err(invalid());
            }
        }
        // 腾讯云术语库 ID 列表（逗号分隔）；允许清空（= 不使用）。
        // 单个 ID 内不允许出现列表分隔符之外的分隔符（分号是 scope_token
        // 里 tc= 维度的连接符，U+0001 是缓存键分隔符）
        "tencent_term_repo_ids" => {
            if value.chars().count() > 500 || value.contains([';', '\u{1}']) {
                return Err(invalid());
            }
        }
        k if CREDENTIAL_KEYS.contains(&k) && value.trim().is_empty() => return Err(invalid()),
        _ => {}
    }
    Ok(())
}

/// 整批静态校验（QT-010）：掩码凭证过滤 + 逐键取值校验。
///
/// **all-or-nothing**：任意一项非法 → 拒绝整批并返回可定位的错误。
/// 返回的 Vec 已剔除掩码凭证（no-op 语义保持：掩码值永远写不进 DB）。
fn validate_batch_static(updates: &[(String, String)]) -> Result<Vec<(String, String)>, AppError> {
    let mut validated: Vec<(String, String)> = Vec::with_capacity(updates.len());
    for (key, value) in updates {
        if crate::domain::config::is_masked_credential(key, value) {
            tracing::debug!("[set_config] {} 传入掩码值，已跳过", key);
            continue;
        }
        validate_value(key, value).map_err(|e| {
            tracing::warn!("[set_config] 批次校验拒绝 {}: {}", key, e);
            e
        })?;
        validated.push((key.clone(), value.clone()));
    }
    Ok(validated)
}

// ──────────── 副作用处理（set_config 单项用） ────────────

async fn handle_side_effect(
    app: &AppHandle,
    state: &State<'_, AppState>,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    // 掩码凭证已在管线入口（set_config_batch_inner）统一过滤，
    // 这里只处理确认为真实值的键。
    match key {
        "provider" => {
            state.translator.set_active_provider(value).await?;
            tray::refresh_menu(app).await;
        }
        "target_lang" => {
            tray::refresh_menu(app).await;
        }
        "deepl_api_key" => {
            // 与下面三家走同一条「完整凭证集 → 重建实例」路径。
            // 此前 DeepL 走的是单独的就地 mutate 分支，是唯一一个特例。
            let cfg = state.config.read().await;
            let creds = build_creds_map(&cfg, &[("deepl_api_key", "deepl_api_key")], key, value);
            drop(cfg);
            state
                .translator
                .update_provider_credentials("deepl", creds)
                .await?;
            // glossary 是账号隔离的：换 key 后旧映射指向的 glossary_id 对
            // 新账号必然无效，复用会让翻译被 400 拒绝并归为 Permanent。
            // 整表清空让下次翻译按指纹重建。
            if let Err(e) = state.glossary_sync.clear_all().await {
                tracing::warn!(
                    event = "glossary_sync_clear_failed",
                    "[config] 更换 DeepL key 后清空 glossary 映射失败: {}",
                    e
                );
            }
        }
        "tencent_secret_id" | "tencent_secret_key" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &cfg,
                &[
                    ("tencent_secret_id", "tencent_secret_id"),
                    ("tencent_secret_key", "tencent_secret_key"),
                ],
                key,
                value,
            );
            drop(cfg);
            state
                .translator
                .update_provider_credentials("tencent", creds)
                .await?;
        }
        "baidu_app_id" | "baidu_secret_key" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &cfg,
                &[
                    ("baidu_app_id", "baidu_app_id"),
                    ("baidu_secret_key", "baidu_secret_key"),
                ],
                key,
                value,
            );
            drop(cfg);
            state
                .translator
                .update_provider_credentials("baidu", creds)
                .await?;
        }
        "youdao_app_key" | "youdao_app_secret" => {
            let cfg = state.config.read().await;
            let creds = build_creds_map(
                &cfg,
                &[
                    ("youdao_app_key", "youdao_app_key"),
                    ("youdao_app_secret", "youdao_app_secret"),
                ],
                key,
                value,
            );
            drop(cfg);
            state
                .translator
                .update_provider_credentials("youdao", creds)
                .await?;
        }
        "fallback_enabled" => {
            state.translator.set_fallback_enabled(value == "true").await;
        }
        "auto_start" => {
            crate::commands::system::set_autostart(app.clone(), value == "true").await?;
        }
        "theme" => {
            emit_theme_changed(app, value);
        }
        "clipboard_monitor_enabled" => {
            apply_clipboard_monitor_switch(app, state.inner(), value == "true").await;
        }
        "reduce_transparency" => {
            emit_reduce_transparency_changed(app, value);
        }
        // 缓存读写开关：持久化已在上方完成，这里只切换活体实例。
        // 关闭不删除既有条目（「清空缓存」仍是独立操作）。
        "cache_enabled" => {
            state.cache.set_enabled(value == "true");
        }
        _ => {
            // 全局快捷键：持久化后重新注册。失败会作为副作用错误上报
            // （设置页显示真实冲突原因，旧快捷键保持可用）。
            if let Some(action) = crate::system::hotkeys::action_for_config_key(key) {
                state
                    .hotkeys
                    .apply(app, action, value)
                    .map_err(|e| AppError::ConfigError(format!("快捷键未生效: {}", e)))?;
            }
        }
    }
    Ok(())
}

/// 剪贴板监控开关的实际生效动作（设置管线与全局快捷键共用）。
///
/// 只做「活体实例 + 托盘菜单状态」这一步；持久化与 config-changed
/// 广播由调用方负责 —— 两条调用路径的持久化语义不同（命令管线在
/// 副作用之前落库，快捷键动作自己先写库）。
pub(crate) async fn apply_clipboard_monitor_switch(
    app: &AppHandle,
    state: &AppState,
    enabled: bool,
) {
    if enabled {
        state.clipboard_monitor.resume();
    } else {
        state.clipboard_monitor.suspend();
    }
    tray::refresh_menu(app).await;
}

/// 广播配置变更事件：载荷是脱敏后的完整 `AppConfig`（与 get_config 同构）。
///
/// 为什么必须有：设置窗口 hide 而非销毁，草稿只在挂载时加载一次；
/// 托盘切换翻译源/目标语言、开关剪贴板监控这些**绕过命令管线的直写
/// 路径**不广播的话，设置页重开后显示的还是打开时的旧值。
/// 载荷直接用 `AppConfig`，前端按字段合并即可，不必在事件里重复一套
/// 「哪些键是布尔」的类型映射。
pub(crate) fn emit_config_changed(app: &AppHandle, cfg: &AppConfig) {
    let _ = app.emit("config-changed", cfg);
}

/// 广播「减少透明效果」开关变更。
///
/// 为什么必须有这条：浮窗是长期驻留窗口（CloseRequested 只 hide 不销毁），
/// 它读的是自己那份 configStore。设置页保存只更新设置窗口自己的 store，
/// 不广播的话浮窗要等到下次整体重载才响应 —— 用户点完开关看不到任何变化。
///
/// 用独立事件而不是塞进 theme-changed：两者语义不同（一个是配色、一个是
/// 材质与衬底），合并会让将来任何一方增加字段时误触另一方。
fn emit_reduce_transparency_changed(app: &AppHandle, value: &str) {
    #[derive(serde::Serialize, Clone)]
    struct ReduceTransparencyPayload {
        enabled: bool,
    }

    let _ = app.emit(
        "reduce-transparency-changed",
        ReduceTransparencyPayload {
            enabled: value == "true",
        },
    );
    tracing::info!("减少透明效果已切换: {}", value);
}

/// 广播主题变更事件给所有已打开的 WebviewWindow
fn emit_theme_changed(app: &AppHandle, theme: &str) {
    #[derive(serde::Serialize, Clone)]
    struct ThemePayload {
        theme: String,
    }

    let _ = app.emit(
        "theme-changed",
        ThemePayload {
            theme: theme.to_string(),
        },
    );
    tracing::info!("主题已切换: {}", theme);
}

// ── 辅助函数 ──

/// 构建凭证 HashMap，将当前 config 中的值与刚更新的单个字段合并
fn build_creds_map(
    config: &crate::domain::config::ConfigService,
    keys: &[(&str, &str)],
    updated_key: &str,
    updated_value: &str,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for (config_key, cred_key) in keys {
        let val = if *config_key == updated_key {
            updated_value.to_string()
        } else {
            config.get(config_key).unwrap_or_default()
        };
        map.insert(cred_key.to_string(), val);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(raw: &str, value: &str) -> String {
        validate_value(raw, value).unwrap_err().to_string()
    }

    // ── 已知键 + 合法值放行 ─────────────────────────────────────────────

    #[test]
    fn known_keys_with_valid_values_pass() {
        assert!(validate_value("target_lang", "zh").is_ok());
        assert!(validate_value("target_lang", "zh-tw").is_ok());
        assert!(validate_value("provider", "deepl").is_ok());
        assert!(validate_value("theme", "system").is_ok());
        assert!(validate_value("theme", "dark").is_ok());
        assert!(validate_value("translation_style", "natural").is_ok());
        assert!(validate_value("translation_style", "formal").is_ok());
        assert!(validate_value("translation_style", "informal").is_ok());
        assert!(validate_value("auto_start", "true").is_ok());
        assert!(validate_value("fallback_enabled", "false").is_ok());
        assert!(validate_value("onboarding_completed", "true").is_ok());
        assert!(validate_value("clipboard_monitor_enabled", "false").is_ok());
        assert!(validate_value("reduce_transparency", "true").is_ok());
        assert!(validate_value("history_limit", "200").is_ok());
        assert!(validate_value("history_limit", "50").is_ok());
        // 导出目录：「清空 = 每次询问」恒放行；绝对路径的正向用例按平台
        // 构造 —— Windows 的绝对以盘符开头，Unix 的绝对以 / 开头，
        // 「绝对」判定本身是 std::path 的平台语义
        assert!(validate_value("export_dir", "").is_ok());
        {
            let absolute = if cfg!(windows) {
                r"C:\Users\welle\Documents".to_string()
            } else {
                "/home/welle/Documents".to_string()
            };
            assert!(
                validate_value("export_dir", &absolute).is_ok(),
                "绝对路径应放行: {}",
                absolute
            );
            assert!(
                std::path::Path::new(&absolute).is_absolute(),
                "测试自身的前提：构造的样本必须是平台绝对路径"
            );
        }
        assert!(validate_value("deepl_api_key", "some-key:fx").is_ok());
        assert!(validate_value("tencent_secret_id", "AKIDxx").is_ok());
    }

    #[test]
    fn translation_style_rejects_unknown_values() {
        // 口语/技术文档/商务/学术等需要 AI 类 Provider 的 prompt 能力，
        // 在能力落地前不许写进配置（不假装支持）
        assert!(err("translation_style", "casual").contains("非法"));
        assert!(err("translation_style", "technical").contains("非法"));
        assert!(err("translation_style", "").contains("非法"));
    }

    // ── export_dir 的形态约束 ───────────────────────────────────────────

    #[test]
    fn export_dir_rejects_relative_paths() {
        // 相对路径会随进程工作目录漂移，写出的文件位置用户找不到。
        // 绝对路径的放行用例见 known_keys_with_valid_values_pass ——
        // 「绝对」的判定随平台（Windows 要盘符），正向用例只锁本平台形态。
        assert!(err("export_dir", "Documents/exports").contains("非法"));
        assert!(err("export_dir", ".").contains("非法"));
        assert!(err("export_dir", "exports").contains("非法"));
    }

    // ── 未知键必须拒绝（写入面的边界）────────────────────────────────────

    #[test]
    fn unknown_keys_are_rejected() {
        for key in [
            "unknown_key",
            "targetLang", // 前端驼峰拼写不允许
            "autoStart",
            "",
            "__proto__",
        ] {
            assert!(
                validate_value(key, "anything").is_err(),
                "未知配置键 {} 必须被拒绝",
                key
            );
            assert!(err(key, "anything").contains("未知配置键"));
        }
    }

    // ── 类型与取值范围 ───────────────────────────────────────────────────

    #[test]
    fn bool_keys_reject_non_bool_values() {
        for key in BOOL_KEYS {
            assert!(validate_value(key, "1").is_err(), "{} 拒绝非布尔", key);
            assert!(validate_value(key, "yes").is_err(), "{} 拒绝非布尔", key);
            assert!(validate_value(key, "").is_err(), "{} 拒绝空值", key);
        }
    }

    #[test]
    fn theme_and_target_lang_reject_unknown_values() {
        assert!(err("theme", "sepia").contains("非法"));
        assert!(err("theme", "").contains("非法"));
        assert!(err("target_lang", "xx").contains("非法"));
        assert!(
            err("target_lang", "ZH").contains("非法"),
            "语言代码区分大小写"
        );
    }

    #[test]
    fn history_limit_rejects_non_numeric_and_out_of_range() {
        assert!(err("history_limit", "abc").contains("整数"));
        assert!(err("history_limit", "").contains("整数"));
        assert!(err("history_limit", "0").contains("非法"));
        assert!(err("history_limit", "-5").contains("非法"));
        assert!(err(
            "history_limit",
            &(crate::domain::config::HISTORY_LIMIT_MAX + 1).to_string()
        )
        .contains("非法"));
        // 边界值放行
        assert!(validate_value(
            "history_limit",
            &crate::domain::config::HISTORY_LIMIT_MAX.to_string()
        )
        .is_ok());
        assert!(validate_value(
            "history_limit",
            &crate::domain::config::HISTORY_LIMIT_MIN.to_string()
        )
        .is_ok());
    }

    #[test]
    fn provider_and_credentials_reject_empty_values() {
        assert!(err("provider", "").contains("非法"));
        assert!(err("provider", "   ").contains("非法"));
        for key in CREDENTIAL_KEYS {
            assert!(validate_value(key, "").is_err(), "{} 拒绝空凭证", key);
        }
    }

    // ── 批次语义：all-or-nothing ─────────────────────────────────────────

    #[test]
    fn batch_with_any_invalid_entry_is_rejected_entirely() {
        let updates = vec![
            ("target_lang".to_string(), "zh".to_string()),
            ("theme".to_string(), "not-a-theme".to_string()),
            ("auto_start".to_string(), "true".to_string()),
        ];
        let result = validate_batch_static(&updates);
        assert!(
            result.is_err(),
            "任一项非法必须拒绝整批 —— 旧实现会跳过非法项并写其余项"
        );
        assert!(
            result.unwrap_err().to_string().contains("theme"),
            "错误必须能定位到非法键"
        );
    }

    #[test]
    fn batch_accepts_valid_entries_and_filters_masked_credentials() {
        let updates = vec![
            ("target_lang".to_string(), "zh".to_string()),
            // 掩码凭证：no-op 语义 —— 被过滤但不算非法
            ("deepl_api_key".to_string(), "********abcd".to_string()),
            ("theme".to_string(), "dark".to_string()),
        ];
        let validated = validate_batch_static(&updates).unwrap();
        assert_eq!(validated.len(), 2, "掩码凭证应被过滤");
        assert_eq!(validated[0].0, "target_lang");
        assert_eq!(validated[1].0, "theme");
    }
}
