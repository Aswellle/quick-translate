// src-tauri/src/infra/database.rs
// SQLite 连接初始化、schema migration、数据损坏恢复

use rusqlite::{Connection, Transaction};
use std::path::Path;
use tracing::{error, info, warn};

use crate::error::AppError;
use crate::types::now_unix_ms;

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
            recover_database(&db_path)?;

            let conn = open_connection(&db_path)?;
            run_migrations(&conn)?;
            seed_defaults(&conn)?;
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
    let migrations: &[(i64, MigrationFn)] = &[(1, migrate_v1), (2, migrate_v2), (3, migrate_v3)];

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
    ('auto_start',            'false',           strftime('%s','now')*1000),
    ('history_limit',         '200',             strftime('%s','now')*1000),
    ('theme',                 '"system"',        strftime('%s','now')*1000),
    ('fallback_enabled',      'true',            strftime('%s','now')*1000),
    ('onboarding_completed',  'false',           strftime('%s','now')*1000),
    ('clipboard_monitor_enabled', 'true',        strftime('%s','now')*1000);
"#;
