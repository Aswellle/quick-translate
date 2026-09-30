// src-tauri/src/infra/database.rs
// SQLite 连接初始化、schema migration、数据损坏恢复

use rusqlite::{Connection, Transaction};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{error, info, warn};

use crate::error::AppError;
use crate::types::now_unix_ms;

/// 全库共享的连接句柄。HistoryRepository 内部用它序列化所有 DB 操作。
pub type SharedConnection = Arc<AsyncMutex<Connection>>;

/// 可降级的数据库句柄（计划第 25/26 节：存储故障不得拖垮翻译主链路）。
///
/// init 失败时 `conn = None`：进程照常启动，历史/缓存功能降级为
/// 「读空、写拒绝、配置回内存默认值」，翻译与剪贴板完全不受影响。
/// 各消费方用 `try_conn()` 取连接，按自己的语义决定怎么降级。
#[derive(Clone)]
pub struct Db {
    conn: Option<SharedConnection>,
}

impl Db {
    pub fn available(conn: SharedConnection) -> Self {
        Self { conn: Some(conn) }
    }

    pub fn unavailable() -> Self {
        Self { conn: None }
    }

    pub fn is_available(&self) -> bool {
        self.conn.is_some()
    }

    /// 存储不可用时返回 None。调用方按各自的降级语义处理：
    /// 历史查询 → 空结果/错误，缓存 → 无命中，配置 → 内存默认值。
    pub fn try_conn(&self) -> Option<SharedConnection> {
        self.conn.clone()
    }
}

/// 初始化数据库：打开文件 → integrity check → 执行 schema migration
pub fn init_db(app_data_dir: &Path) -> Result<Connection, AppError> {
    // 确保目录存在
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| AppError::DatabaseError(format!("创建数据目录失败: {}", e)))?;

    let db_path = app_data_dir.join("quicktranslate.db");
    info!("数据库路径: {:?}", db_path);

    // 尝试打开并验证数据库
    match open_and_verify(&db_path) {
        Ok(conn) => {
            let is_fresh = is_fresh_install(&conn);
            run_migrations(&conn)?;
            if is_fresh {
                seed_defaults(&conn)?;
            }
            Ok(conn)
        }
        Err(e) => {
            warn!("数据库验证失败，尝试恢复: {}", e);
            // 先抢救配置（含加密凭证密文），再备份重建 —— 顺序不能反。
            // 计划第 25 节：禁止静默覆盖/清空原 credential。
            let salvaged = salvage_config_rows(&db_path);
            recover_database(&db_path)?;

            let conn = open_connection(&db_path)?;
            run_migrations(&conn)?;
            seed_defaults(&conn)?;
            restore_salvaged_rows(&conn, &salvaged);
            Ok(conn)
        }
    }
}

fn open_and_verify(db_path: &Path) -> Result<Connection, AppError> {
    let conn = open_connection(db_path)?;

    // 执行完整性检查（3MB DB 上耗时 <10ms，不影响 2s 启动预算）
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    if result != "ok" {
        return Err(AppError::DatabaseError(format!(
            "数据库完整性检查失败: {}",
            result
        )));
    }

    Ok(conn)
}

fn open_connection(db_path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(db_path)
        .map_err(|e| AppError::DatabaseError(format!("打开数据库失败: {}", e)))?;

    // 启用 WAL 模式：并发读写性能更好
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
    ",
    )
    .map_err(|e| AppError::DatabaseError(format!("PRAGMA 初始化失败: {}", e)))?;

    Ok(conn)
}

/// 损坏恢复：将损坏文件重命名并重建空数据库
fn recover_database(db_path: &Path) -> Result<(), AppError> {
    let timestamp = now_unix_ms();
    let corrupt_path = db_path.with_extension(format!("db.corrupt.{}", timestamp));

    if db_path.exists() {
        std::fs::rename(db_path, &corrupt_path)
            .map_err(|e| AppError::DatabaseError(format!("备份损坏文件失败: {}", e)))?;
        error!("数据库损坏，已备份至 {:?}，重建空数据库", corrupt_path);
    }

    Ok(())
}

/// 从损坏的数据库中尽力抢救 app_config 行（用户配置 + 加密凭证密文）。
///
/// SQLite 按页损坏：integrity_check 失败的库仍可能有大片可读页。V2 计划
/// 第 25 节要求「禁止静默覆盖/清空原 credential」，因此在重建空库**之前**
/// 以只读模式把配置表捞出来，重建后由调用方原样写回。密文依赖的
/// per-install 密钥存在独立文件里不受影响，捞回的凭证照常可解密。
///
/// 捞不出（文件彻底不可读 / 表损坏）时返回空表 —— 凭证确实丢了，但
/// 必须留下明确的事件日志，而不是像从前那样无日志地静默重置。
fn salvage_config_rows(db_path: &Path) -> Vec<(String, String)> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    let conn = match Connection::open_with_flags(db_path, flags) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                event = "config_salvage_failed",
                reason = %e,
                "损坏库无法以只读方式打开，配置与凭证无法抢救"
            );
            return Vec::new();
        }
    };

    let rows_result: Result<Vec<(String, String)>, rusqlite::Error> = (|| {
        let mut stmt = conn.prepare("SELECT key, value FROM app_config")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect()
    })();

    let rows = rows_result.unwrap_or_default();

    if rows.is_empty() {
        warn!(
            event = "config_salvage_empty",
            "损坏库中未捞到任何配置行，凭证将重置（用户需重新填写）"
        );
    } else {
        info!(
            event = "config_salvaged",
            count = rows.len(),
            "已从损坏库抢救配置行（含凭证密文），将在重建后写回"
        );
    }
    rows
}

/// 把抢救出的配置行写回重建后的库（覆盖默认种子值）。
fn restore_salvaged_rows(conn: &Connection, rows: &[(String, String)]) {
    let now = now_unix_ms();
    for (key, value) in rows {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, value, now],
        ) {
            warn!(
                event = "config_restore_failed",
                key = %key,
                reason = %e,
                "抢救出的配置行写回失败"
            );
        }
    }
}

/// 判断是否为全新安装（schema_version 表不存在或为空）
fn is_fresh_install(conn: &Connection) -> bool {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);

    if !exists {
        return true;
    }

    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    version == 0
}

/// 执行所有待应用的 schema migrations（按版本顺序，每个在独立事务中执行）
pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    // 确保 schema_version 表本身存在（bootstrap）
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version     INTEGER PRIMARY KEY NOT NULL,
            applied_at  INTEGER NOT NULL
        );",
    )
    .map_err(|e| AppError::DatabaseMigration {
        message: format!("无法创建 schema_version 表: {}", e),
    })?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    info!("当前数据库版本: {}", current_version);

    // 迁移列表：(版本号, 迁移函数)
    type MigrationFn = fn(&Transaction) -> Result<(), AppError>;
    let migrations: &[(i64, MigrationFn)] = &[
        (1, migrate_v1),
        (2, migrate_v2),
        (3, migrate_v3),
        (4, migrate_v4),
        (5, migrate_v5),
        (6, migrate_v6),
    ];

    for &(version, migration_fn) in migrations {
        if current_version >= version {
            continue;
        }

        info!("应用数据库迁移 v{}", version);
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| AppError::DatabaseMigration {
                message: format!("开启事务失败 (v{}): {}", version, e),
            })?;

        migration_fn(&tx).map_err(|e| AppError::DatabaseMigration {
            message: format!("迁移 v{} 失败: {}", version, e),
        })?;

        tx.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now_unix_ms()],
        )
        .map_err(|e| AppError::DatabaseMigration {
            message: format!("记录迁移版本失败 (v{}): {}", version, e),
        })?;

        tx.commit().map_err(|e| AppError::DatabaseMigration {
            message: format!("提交迁移事务失败 (v{}): {}", version, e),
        })?;

        info!("数据库迁移 v{} 完成", version);
    }

    Ok(())
}

/// 仅在全新安装时执行：写入默认配置值
pub fn seed_defaults(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(SEED_SQL)
        .map_err(|e| AppError::DatabaseError(format!("默认配置写入失败: {}", e)))?;
    Ok(())
}

// ──────────── Migration v1 ────────────

/// Migration v1：创建所有基础表（翻译记录、FTS5、配置、触发器、索引）
fn migrate_v1(tx: &Transaction) -> Result<(), AppError> {
    tx.execute_batch(SCHEMA_V1_SQL)
        .map_err(|e| AppError::DatabaseError(format!("Schema v1 初始化失败: {}", e)))
}

// ──────────── Migration v2 ────────────

/// Migration v2：为 translation_records 添加 is_starred 字段（Favorites 功能）
fn migrate_v2(tx: &Transaction) -> Result<(), AppError> {
    tx.execute_batch(
        "ALTER TABLE translation_records ADD COLUMN is_starred INTEGER NOT NULL DEFAULT 0;
         CREATE INDEX IF NOT EXISTS idx_records_starred ON translation_records(is_starred);",
    )
    .map_err(|e| AppError::DatabaseError(format!("Schema v2 迁移失败: {}", e)))
}

// ──────────── Migration v3 ────────────

/// Migration v3：移除 FTS5 虚拟表和同步触发器
/// 搜索改用 LIKE 子串匹配（更好地支持 CJK 字符），FTS5 只有写入开销无查询收益
fn migrate_v3(tx: &Transaction) -> Result<(), AppError> {
    tx.execute_batch(
        "DROP TABLE IF EXISTS translation_records_fts;
         DROP TRIGGER IF EXISTS trg_records_ai;
         DROP TRIGGER IF EXISTS trg_records_ad;",
    )
    .map_err(|e| AppError::DatabaseError(format!("Schema v3 迁移失败: {}", e)))
}

// ──────────── Migration v4 ────────────

/// Migration v4：删除遗留的 `hotkey` 配置项
/// 早期快捷键方案的残留，`AppConfig` 已无对应字段，读取时被 `_ => {}` 忽略
fn migrate_v4(tx: &Transaction) -> Result<(), AppError> {
    tx.execute_batch("DELETE FROM app_config WHERE key = 'hotkey';")
        .map_err(|e| AppError::DatabaseError(format!("Schema v4 迁移失败: {}", e)))
}

// ──────────── Migration v5 ────────────

/// Migration v5：将遗留的裸值配置项归一化为 JSON 字符串形式（C4）
///
/// v0.2.2 之前 SEED_SQL 为布尔/整数写入裸值（'false'/'200'），而
/// `ConfigService::set()` 写入 JSON（'"false"'/'"200"'）—— 同一张表并存
/// 两种格式，读取端只能靠容错解码兜着。种子已在同版本改为 JSON，此迁移
/// 补齐已安装用户的历史行，使"所有配置值均为 JSON 编码"真正成为不变量。
///
/// 仅处理非加密键：加密字段存的是 base64 密文，本就不是 JSON，
/// 且走 `is_encrypted` 分支读取，不能加引号。
/// 条件 `value NOT LIKE '"%'` 保证幂等，重复执行无副作用。
fn migrate_v5(tx: &Transaction) -> Result<(), AppError> {
    tx.execute_batch(
        r#"
        UPDATE app_config
           SET value = '"' || value || '"'
         WHERE key IN (
                 'auto_start',
                 'history_limit',
                 'fallback_enabled',
                 'onboarding_completed',
                 'clipboard_monitor_enabled'
               )
           AND value NOT LIKE '"%';
        "#,
    )
    .map_err(|e| AppError::DatabaseError(format!("Schema v5 迁移失败: {}", e)))
}

// ──────────── SQL 常量 ────────────

/// Schema v1：基础表结构（不含 is_starred，由 v2 迁移添加）
const SCHEMA_V1_SQL: &str = r#"
-- ============================================================
-- 1. 翻译历史记录表
-- ============================================================
CREATE TABLE IF NOT EXISTS translation_records (
    id              TEXT        PRIMARY KEY NOT NULL,
    source_text     TEXT        NOT NULL,
    translated_text TEXT        NOT NULL,
    source_lang     TEXT        NOT NULL,
    target_lang     TEXT        NOT NULL,
    provider        TEXT        NOT NULL,
    created_at      INTEGER     NOT NULL,
    duration_ms     INTEGER     DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_records_created_at
    ON translation_records (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_records_created_at_asc
    ON translation_records (created_at ASC);

-- ============================================================
-- 2. FTS5 全文搜索虚拟表
-- ============================================================
CREATE VIRTUAL TABLE IF NOT EXISTS translation_records_fts USING fts5(
    source_text,
    translated_text,
    content='translation_records',
    content_rowid='rowid',
    tokenize='unicode61'
);

-- FTS 同步触发器：INSERT
CREATE TRIGGER IF NOT EXISTS trg_records_ai AFTER INSERT ON translation_records BEGIN
    INSERT INTO translation_records_fts (rowid, source_text, translated_text)
    VALUES (NEW.rowid, NEW.source_text, NEW.translated_text);
END;

-- FTS 同步触发器：DELETE
CREATE TRIGGER IF NOT EXISTS trg_records_ad AFTER DELETE ON translation_records BEGIN
    INSERT INTO translation_records_fts (translation_records_fts, rowid, source_text, translated_text)
    VALUES ('delete', OLD.rowid, OLD.source_text, OLD.translated_text);
END;

-- ============================================================
-- 3. 应用配置表（KV 结构）
-- ============================================================
CREATE TABLE IF NOT EXISTS app_config (
    key             TEXT        PRIMARY KEY NOT NULL,
    value           TEXT        NOT NULL,
    updated_at      INTEGER     NOT NULL
);
"#;

/// 默认配置种子数据（仅全新安装时执行）
const SEED_SQL: &str = r#"
INSERT OR IGNORE INTO app_config (key, value, updated_at) VALUES
    ('target_lang',           '"zh"',            strftime('%s','now')*1000),
    ('provider',              '"google"',         strftime('%s','now')*1000),
    ('deepl_api_key',         '""',              strftime('%s','now')*1000),
    ('tencent_secret_id',     '""',              strftime('%s','now')*1000),
    ('tencent_secret_key',    '""',              strftime('%s','now')*1000),
    ('baidu_app_id',          '""',              strftime('%s','now')*1000),
    ('baidu_secret_key',      '""',              strftime('%s','now')*1000),
    ('youdao_app_key',        '""',              strftime('%s','now')*1000),
    ('youdao_app_secret',     '""',              strftime('%s','now')*1000),
    -- 布尔/整数同样写 JSON 字符串形式，与 ConfigService::set() 的
    -- serde_json::to_string() 编码保持一致（C4）。此前种子写裸值
    -- 'false'/'200'，用户改过设置后被重写成 '"false"'/'"200"'，
    -- 同一张表长期并存两种格式。
    ('auto_start',            '"false"',         strftime('%s','now')*1000),
    ('history_limit',         '"200"',           strftime('%s','now')*1000),
    ('theme',                 '"system"',        strftime('%s','now')*1000),
    ('fallback_enabled',      '"true"',          strftime('%s','now')*1000),
    ('onboarding_completed',  '"false"',         strftime('%s','now')*1000),
    ('clipboard_monitor_enabled', '"true"',      strftime('%s','now')*1000);
"#;

/// 翻译缓存的建表语句（schema v6）。
///
/// 导出给 `domain::cache` 的测试共用 —— 让测试建的表与真实表结构不可能漂移。
pub(crate) const CACHE_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS translation_cache (
    -- 归一化文本 + 目标语言。刻意不含 provider：同一段文本换个源译出来的
    -- 意思是一样的，离线时能命中远比「必须是同一个源」有用。
    cache_key       TEXT PRIMARY KEY,
    source_text     TEXT NOT NULL,
    translated_text TEXT NOT NULL,
    -- 检测到的源语言：结果视图要显示"原文语言 → 目标语言"的方向标签，
    -- 缓存命中时没有 provider 帮我们重新检测，只能存下来
    source_lang     TEXT NOT NULL DEFAULT '',
    provider        TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    -- LRU 记账：淘汰时按此列升序
    last_hit_at     INTEGER NOT NULL,
    hit_count       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_translation_cache_last_hit
    ON translation_cache(last_hit_at);
"#;

/// v6：新增翻译缓存表（计划第 16 节）。
///
/// 独立于 translation_records：历史受 history_limit（默认 200）约束，
/// 且用户「清空历史」会把它整个删掉 —— 离线缓存不该被这两件事影响。
fn migrate_v6(tx: &Transaction) -> Result<(), AppError> {
    tx.execute_batch(CACHE_TABLE_SQL)
        .map_err(|e| AppError::DatabaseError(format!("Schema v6 迁移失败: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "qt-db-test-{}-{}-{}",
            tag,
            std::process::id(),
            now_unix_ms()
        ));
        std::fs::create_dir_all(&dir).expect("建测试目录失败");
        dir
    }

    /// 计划第 25 节：文件级损坏（非 SQLite 文件）走完整恢复链 ——
    /// 抢救降级为空表 + 事件日志，重建后返回可用库，绝不 panic。
    #[test]
    fn corrupted_file_rebuilds_with_graceful_salvage() {
        let dir = temp_dir("corrupt");
        let db_path = dir.join("quicktranslate.db");
        std::fs::write(&db_path, vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42]).unwrap();

        let conn = init_db(&dir).expect("损坏文件必须走恢复链后成功重建");

        let version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(version >= 1, "重建后的库必须完成迁移");
    }

    /// 抢救逻辑：可读库中的 app_config 行（含凭证密文）能被原样捞出。
    #[test]
    fn salvage_reads_config_rows_from_readable_db() {
        let dir = temp_dir("salvage");
        {
            let conn = open_connection(&dir.join("quicktranslate.db")).unwrap();
            run_migrations(&conn).unwrap();
            conn.execute(
                "INSERT INTO app_config (key, value, updated_at) VALUES ('deepl_api_key', 'cipher-xyz', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO app_config (key, value, updated_at) VALUES ('target_lang', '\"fr\"', 2)",
                [],
            )
            .unwrap();
        } // drop(conn)：释放文件句柄

        let rows = salvage_config_rows(&dir.join("quicktranslate.db"));
        assert!(rows.contains(&("deepl_api_key".to_string(), "cipher-xyz".to_string())));
        assert!(rows.contains(&("target_lang".to_string(), "\"fr\"".to_string())));
    }

    /// 重建后写回：抢救行覆盖默认种子值（凭证不得被种子清空）。
    #[test]
    fn restored_rows_override_seed_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        seed_defaults(&conn).unwrap();

        restore_salvaged_rows(
            &conn,
            &[("deepl_api_key".to_string(), "cipher-salvaged".to_string())],
        );

        let value: String = conn
            .query_row(
                "SELECT value FROM app_config WHERE key = 'deepl_api_key'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, "cipher-salvaged", "抢救行必须覆盖默认空值");
    }
}
