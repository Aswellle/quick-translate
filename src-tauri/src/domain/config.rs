// src-tauri/src/domain/config.rs
// 配置服务：内存缓存 + SQLite 持久化

use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::infra::crypto;
use crate::types::{now_unix_ms, AppConfig};

/// 需要加密存储的 key 集合
const ENCRYPTED_KEYS: &[&str] = &[
    "deepl_api_key",
    "tencent_secret_id",
    "tencent_secret_key",
    "baidu_app_id",
    "baidu_secret_key",
    "youdao_app_key",
    "youdao_app_secret",
];

fn is_encrypted(key: &str) -> bool {
    ENCRYPTED_KEYS.contains(&key)
}

/// history_limit 的有效范围（含端点）。
/// 低于下限会导致 enforce_limit 删除全部非收藏记录；
/// 过大的值会削弱 FIFO 清理意义并占用内存。
pub const HISTORY_LIMIT_MIN: i64 = 1;
pub const HISTORY_LIMIT_MAX: i64 = 100_000;

pub struct ConfigService {
    db: Arc<Mutex<Connection>>,
    cache: AppConfig,
}

impl ConfigService {
    pub fn load(db: Arc<Mutex<Connection>>) -> Result<Self, AppError> {
        let conn = db.blocking_lock();
        let cache = load_config_from_db(&conn)?;
        drop(conn);
        Ok(ConfigService { db, cache })
    }

    pub fn get_all(&self) -> AppConfig {
        let mut c = self.cache.clone();
        // 脱敏所有凭证
        c.deepl_api_key = crypto::mask_api_key(&c.deepl_api_key);
        c.tencent_secret_id = crypto::mask_api_key(&c.tencent_secret_id);
        c.tencent_secret_key = crypto::mask_api_key(&c.tencent_secret_key);
        c.baidu_app_id = crypto::mask_api_key(&c.baidu_app_id);
        c.baidu_secret_key = crypto::mask_api_key(&c.baidu_secret_key);
        c.youdao_app_key = crypto::mask_api_key(&c.youdao_app_key);
        c.youdao_app_secret = crypto::mask_api_key(&c.youdao_app_secret);
        c
    }

    /// 获取明文凭证（供 Rust 内部使用，不暴露给前端）
    pub fn get_credential(&self, key: &str) -> String {
        match key {
            "deepl_api_key" => self.cache.deepl_api_key.clone(),
            "tencent_secret_id" => self.cache.tencent_secret_id.clone(),
            "tencent_secret_key" => self.cache.tencent_secret_key.clone(),
            "baidu_app_id" => self.cache.baidu_app_id.clone(),
            "baidu_secret_key" => self.cache.baidu_secret_key.clone(),
            "youdao_app_key" => self.cache.youdao_app_key.clone(),
            "youdao_app_secret" => self.cache.youdao_app_secret.clone(),
            _ => String::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "target_lang" => Some(self.cache.target_lang.clone()),
            "provider" => Some(self.cache.provider.clone()),
            "deepl_api_key" => Some(self.cache.deepl_api_key.clone()),
            "tencent_secret_id" => Some(self.cache.tencent_secret_id.clone()),
            "tencent_secret_key" => Some(self.cache.tencent_secret_key.clone()),
            "baidu_app_id" => Some(self.cache.baidu_app_id.clone()),
            "baidu_secret_key" => Some(self.cache.baidu_secret_key.clone()),
            "youdao_app_key" => Some(self.cache.youdao_app_key.clone()),
            "youdao_app_secret" => Some(self.cache.youdao_app_secret.clone()),
            "auto_start" => Some(self.cache.auto_start.to_string()),
            "history_limit" => Some(self.cache.history_limit.to_string()),
            "theme" => Some(self.cache.theme.clone()),
            "fallback_enabled" => Some(self.cache.fallback_enabled.to_string()),
            "onboarding_completed" => Some(self.cache.onboarding_completed.to_string()),
            "clipboard_monitor_enabled" => Some(self.cache.clipboard_monitor_enabled.to_string()),
            _ => None,
        }
    }

    pub async fn set(&mut self, key: &str, value: &str) -> Result<(), AppError> {
        let db_value = if is_encrypted(key) {
            crypto::encrypt(value)?
        } else {
            serde_json::to_string(value)
                .map_err(|e| AppError::ConfigError(format!("序列化失败: {}", e)))?
        };
        let now = now_unix_ms();
        {
            let conn = self.db.lock().await;
            conn.execute(
                "INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![key, db_value, now],
            )
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        }
        self.apply_to_cache(key, value);
        Ok(())
    }

    pub async fn set_batch(&mut self, updates: Vec<(String, String)>) -> Result<(), AppError> {
        let now = now_unix_ms();
        {
            let conn = self.db.lock().await;
            conn.execute_batch("BEGIN")?;
            for (key, value) in &updates {
                let db_value = if is_encrypted(key) {
                    crypto::encrypt(value)?
                } else {
                    serde_json::to_string(value)
                        .map_err(|e| AppError::ConfigError(format!("序列化失败: {}", e)))?
                };
                conn.execute(
                    "INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![key, db_value, now],
                ).map_err(|e| AppError::DatabaseError(e.to_string()))?;
            }
            conn.execute_batch("COMMIT")?;
        }
        for (key, value) in &updates {
            self.apply_to_cache(key, value);
        }
        Ok(())
    }

    fn apply_to_cache(&mut self, key: &str, value: &str) {
        match key {
            "target_lang" => self.cache.target_lang = value.to_string(),
            "provider" => self.cache.provider = value.to_string(),
            "deepl_api_key" => self.cache.deepl_api_key = value.to_string(),
            "tencent_secret_id" => self.cache.tencent_secret_id = value.to_string(),
            "tencent_secret_key" => self.cache.tencent_secret_key = value.to_string(),
            "baidu_app_id" => self.cache.baidu_app_id = value.to_string(),
            "baidu_secret_key" => self.cache.baidu_secret_key = value.to_string(),
            "youdao_app_key" => self.cache.youdao_app_key = value.to_string(),
            "youdao_app_secret" => self.cache.youdao_app_secret = value.to_string(),
            "auto_start" => self.cache.auto_start = value == "true",
            "history_limit" => {
                if let Ok(n) = value.parse::<i64>() {
                    self.cache.history_limit = n.clamp(HISTORY_LIMIT_MIN, HISTORY_LIMIT_MAX);
                }
            }
            "fallback_enabled" => self.cache.fallback_enabled = value == "true",
            "onboarding_completed" => self.cache.onboarding_completed = value == "true",
            "clipboard_monitor_enabled" => self.cache.clipboard_monitor_enabled = value == "true",
            _ => {}
        }
    }
}

fn load_config_from_db(conn: &Connection) -> Result<AppConfig, AppError> {
    let mut config = AppConfig::default();
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_config")
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    // 先读取全部行，避免在 query_map 迭代中借用冲突
    let rows: Vec<(String, String)> = rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    // 记录需要从旧密钥迁移到的新密钥的条目 (key, plaintext)
    let mut to_migrate: Vec<(String, String)> = Vec::new();

    for (key, raw) in rows {
        match key.as_str() {
            k if is_encrypted(k) => {
                // 优先尝试新版密钥解密
                let plain = crypto::decrypt(&raw).unwrap_or_else(|_| {
                    // 新版密钥失败：尝试旧版密钥（迁移兼容）
                    let mut found = String::new();
                    for old_key in crypto::old_key_candidates() {
                        if let Ok(pt) = crypto::decrypt_with_key(&raw, &old_key) {
                            found = pt;
                            break;
                        }
                    }
                    found
                });
                if plain.is_empty() && !raw.is_empty() {
                    // 新旧密钥均无法解密：可能是损坏数据，保留空值
                    tracing::warn!("凭证 {} 无法解密，已重置为空", k);
                } else if crypto::decrypt(&raw).is_err() && !plain.is_empty() {
                    // 旧密钥解密成功 → 需要迁移到新版密钥
                    to_migrate.push((k.to_string(), plain.clone()));
                }
                match k {
                    "deepl_api_key" => config.deepl_api_key = plain,
                    "tencent_secret_id" => config.tencent_secret_id = plain,
                    "tencent_secret_key" => config.tencent_secret_key = plain,
                    "baidu_app_id" => config.baidu_app_id = plain,
                    "baidu_secret_key" => config.baidu_secret_key = plain,
                    "youdao_app_key" => config.youdao_app_key = plain,
                    "youdao_app_secret" => config.youdao_app_secret = plain,
                    _ => {}
                }
            }
            "target_lang" => {
                config.target_lang = ps(&raw).unwrap_or(config.target_lang);
            }
            "provider" => {
                config.provider = ps(&raw).unwrap_or(config.provider);
            }
            "auto_start" => {
                config.auto_start = decode(&raw) == "true";
            }
            "history_limit" => {
                let parsed = decode(&raw).parse::<i64>();
                config.history_limit = match parsed {
                    Ok(n) if n >= HISTORY_LIMIT_MIN && n <= HISTORY_LIMIT_MAX => n,
                    Ok(n) => {
                        // 越界值（含 0/负数）钳制到安全范围，避免全部删除
                        tracing::warn!(
                            "history_limit {} 越界，钳制到 [{}, {}]",
                            n, HISTORY_LIMIT_MIN, HISTORY_LIMIT_MAX
                        );
                        n.clamp(HISTORY_LIMIT_MIN, HISTORY_LIMIT_MAX)
                    }
                    Err(_) => config.history_limit,
                };
            }
            "theme" => {
                config.theme = ps(&raw).unwrap_or(config.theme);
            }
            "fallback_enabled" => {
                config.fallback_enabled = decode(&raw) == "true";
            }
            "onboarding_completed" => {
                config.onboarding_completed = decode(&raw) == "true";
            }
            "clipboard_monitor_enabled" => {
                config.clipboard_monitor_enabled = decode(&raw) == "true";
            }
            _ => {}
        }
    }

    // 迁移：将旧密钥加密的凭证用新版密钥重新加密并持久化
    if !to_migrate.is_empty() {
        let now = now_unix_ms();
        for (key, plain) in &to_migrate {
            match crypto::encrypt(plain) {
                Ok(new_value) => {
                    if let Err(e) = conn.execute(
                        "UPDATE app_config SET value = ?1, updated_at = ?2 WHERE key = ?3",
                        rusqlite::params![new_value, now, key],
                    ) {
                        tracing::warn!("迁移凭证 {} 写入失败: {}", key, e);
                    }
                }
                Err(e) => tracing::warn!("迁移凭证 {} 加密失败: {}", key, e),
            }
        }
        tracing::info!("已迁移 {} 条凭证到新版加密密钥", to_migrate.len());
    }
    Ok(config)
}

fn ps(raw: &str) -> Option<String> {
    serde_json::from_str::<String>(raw).ok()
}

/// 容错解码存储值。
///
/// `set()` / `set_batch()` 用 `serde_json::to_string()` 写入，所以布尔与整数在库中
/// 是带引号的 JSON 字符串（如 `"true"`、`"200"`）。此前布尔/整数读取路径用裸比较
/// （`raw.trim() == "true"`），带引号的值永远不匹配 → 所有布尔配置重启后读成 false，
/// 导致划词翻译被 suspend + onboarding 未完成双重拦截。
///
/// 优先按 JSON 字符串解码，失败则回退裸字符串（兼容手工写入的历史数据）。
fn decode(raw: &str) -> String {
    // 复用 ps() 的 JSON 解码，仅补裸值回退（C2：消除双解码器）
    ps(raw).unwrap_or_else(|| raw.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::decode;

    /// 模拟 set() 的写入编码，确保 write → read 往返一致
    fn encode(value: &str) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn bool_roundtrip_through_json_encoding() {
        // 回归：此前读取用裸比较，encode("true") == "\"true\"" 永远不等于 "true"，
        // 导致 clipboard_monitor_enabled / onboarding_completed 重启后全变 false
        assert_eq!(decode(&encode("true")), "true");
        assert_eq!(decode(&encode("false")), "false");
        assert!(decode(&encode("true")) == "true");
        assert!(decode(&encode("false")) != "true");
    }

    #[test]
    fn int_roundtrip_through_json_encoding() {
        assert_eq!(decode(&encode("200")).parse::<i64>().unwrap(), 200);
    }

    #[test]
    fn string_roundtrip_through_json_encoding() {
        assert_eq!(decode(&encode("zh")), "zh");
        assert_eq!(decode(&encode("system")), "system");
    }

    #[test]
    fn falls_back_to_bare_value_for_legacy_rows() {
        // 手工写入或旧版本遗留的裸值仍需可读
        assert_eq!(decode("true"), "true");
        assert_eq!(decode(" true "), "true");
        assert_eq!(decode("200"), "200");
    }
}
